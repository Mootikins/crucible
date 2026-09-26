//! The stale check: compare the disk with the base of each proposed write.
//!
//! A proposal is `Stale` when a file on disk no longer matches the base that
//! its writer read. The daemon checks at each file-watch event, at
//! `proposal.list`, because the daemon does not watch a closed kiln.
//! Acceptance rechecks the bases in the checked write itself. The check moves a proposal only between `Open` and `Stale`. An
//! accept decides `Conflicted`, and a newer proposal decides `Superseded`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crucible_core::file_write::ExpectedBase;
use crucible_core::note_edit::disk_hash;
use crucible_core::proposal::{Proposal, ProposalId, ProposalState, ProposedWrite};
use crucible_core::protocol::session_events::{SessionEventPayload, SystemPayload};
use crucible_core::protocol::SessionEventMessage;
use tokio::sync::broadcast;

use super::{ProposalResult, ProposalStore};

/// The disk state of one proposed path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Disk {
    Absent,
    Text(String),
    /// A file that is not UTF-8 text. No text base can match it.
    NotText,
}

/// The absolute path of `write`.
pub(super) fn target(write: &ProposedWrite) -> PathBuf {
    write.root.as_path().join(&write.path)
}

/// Read the disk state of `write`. An I/O error other than an absent file
/// gives `None`, because the check cannot know the state.
pub(super) fn read_disk(write: &ProposedWrite) -> Option<Disk> {
    let path = target(write);
    match std::fs::read(&path) {
        Ok(bytes) => Some(match String::from_utf8(bytes) {
            Ok(text) => Disk::Text(text),
            Err(_) => Disk::NotText,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Disk::Absent),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "could not read a proposed file");
            None
        }
    }
}

/// Whether `disk` still matches `base`. The checked write hashes an absent
/// file to `""`, so this check does the same.
pub(super) fn base_holds(base: &ExpectedBase, disk: &Disk) -> bool {
    match base {
        ExpectedBase::Unchecked => true,
        ExpectedBase::Absent => *disk == Disk::Absent,
        ExpectedBase::Hash { hash } | ExpectedBase::Text { hash, .. } => match disk {
            Disk::Absent => hash.is_empty(),
            Disk::Text(text) => disk_hash(text) == *hash,
            Disk::NotText => false,
        },
    }
}

/// The state that the stale check gives `proposal`, or `None` when the check
/// does not change it.
fn checked_state(proposal: &Proposal) -> Option<ProposalState> {
    let stale_now = match proposal.state {
        ProposalState::Open => false,
        ProposalState::Stale => true,
        _ => return None,
    };
    let mut moved = false;
    for write in &proposal.writes {
        // An unknown disk state keeps the current state.
        let disk = read_disk(write)?;
        moved |= !base_holds(&write.base, &disk);
    }
    match (stale_now, moved) {
        (false, true) => Some(ProposalState::Stale),
        (true, false) => Some(ProposalState::Open),
        _ => None,
    }
}

impl ProposalStore {
    /// Run the stale check on every open or stale proposal. Returns the ids
    /// that changed, and announces each of them.
    pub fn check_stale(&self) -> ProposalResult<Vec<ProposalId>> {
        self.check_stale_where(|_| true)
    }

    /// Run the stale check on each open or stale proposal that writes one of
    /// `paths`. The file watcher reports canonical paths, so the check also
    /// compares the canonical form of each target.
    pub fn check_stale_at(&self, paths: &[PathBuf]) -> ProposalResult<Vec<ProposalId>> {
        let touches = |write: &ProposedWrite| {
            let target = target(write);
            paths.contains(&target)
                || target
                    .canonicalize()
                    .ok()
                    .or_else(|| canonical_parent(&target))
                    .is_some_and(|canonical| paths.contains(&canonical))
        };
        self.check_stale_where(|proposal| proposal.writes.iter().any(touches))
    }

    fn check_stale_where(
        &self,
        select: impl Fn(&Proposal) -> bool,
    ) -> ProposalResult<Vec<ProposalId>> {
        let changed = {
            let reserved = self.write.lock().unwrap_or_else(|e| e.into_inner());
            let mut changed = Vec::new();
            for proposal in self.files.all()? {
                if reserved.contains(&proposal.id)
                    || !select(&proposal)
                    || checked_state(&proposal).is_none()
                {
                    continue;
                }
                // Check again under the file lock, so the change applies to
                // the proposal as it is on disk now.
                let updated = self.files.update(&proposal.id, |p| {
                    Ok(checked_state(p).map(|state| p.state = state).is_some())
                })?;
                if updated == Some(true) {
                    changed.push(proposal.id);
                }
            }
            changed
        };
        for id in &changed {
            self.announce(*id);
        }
        Ok(changed)
    }
}

/// The path with its parent resolved, for a file that no longer exists.
fn canonical_parent(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?.canonicalize().ok()?;
    Some(parent.join(path.file_name()?))
}

/// The paths that a file-watch event names, or none for another event.
fn event_paths(msg: &SessionEventMessage) -> Vec<PathBuf> {
    match msg.payload() {
        Ok(SessionEventPayload::System(payload)) => match payload {
            SystemPayload::FileChanged { path, .. } | SystemPayload::FileDeleted { path } => {
                vec![path]
            }
            SystemPayload::FileMoved { from, to } => vec![from, to],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Run the stale check at each file-watch event on the bus.
///
/// A lagged receiver runs the check on every proposal, because it does not
/// know which paths it missed.
pub fn spawn_stale_watch(
    mut rx: broadcast::Receiver<SessionEventMessage>,
    store: Arc<ProposalStore>,
) {
    tokio::spawn(async move {
        loop {
            let result = match rx.recv().await {
                Ok(msg) => {
                    let paths = event_paths(&msg);
                    if paths.is_empty() {
                        continue;
                    }
                    let store = store.clone();
                    tokio::task::spawn_blocking(move || store.check_stale_at(&paths)).await
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let store = store.clone();
                    tokio::task::spawn_blocking(move || store.check_stale()).await
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };
            match result {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => tracing::warn!(error = %e, "the proposal stale check failed"),
                Err(e) => tracing::warn!(error = %e, "the proposal stale check stopped"),
            }
        }
    });
}
