//! Proposals: note writes that wait for the user.
//!
//! A session in `propose` mode does not write a note. It records the write
//! here, and the user accepts, rejects or dismisses the proposal later. A
//! proposal never expires. The daemon keeps every proposal file, also after
//! the proposal leaves the Inbox.
//!
//! See `docs/Meta/Analysis/Diff Review and Proposals.md`, sections 7.1 and 7.3.

mod accept;
mod rpc;
mod stale;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use chrono::Utc;
use crucible_core::file_write::ExpectedBase;
use crucible_core::proposal::{Proposal, ProposalAuthor, ProposalId, ProposalState, ProposedWrite};
use crucible_core::protocol::SessionEventMessage;
use crucible_core::session::{PhysicalRoot, SessionId};
use tokio::sync::broadcast;

pub(crate) use rpc::{
    handle_proposal_accept, handle_proposal_dismiss, handle_proposal_get, handle_proposal_list,
    handle_proposal_reject, handle_proposal_resolve,
};
pub use stale::spawn_stale_watch;
pub use store::{proposals_root, root_beside_snapshots};

/// Why a proposal operation failed.
#[derive(Debug, thiserror::Error)]
pub enum ProposalError {
    /// No proposal file has this id.
    #[error("no proposal has the id {0}")]
    NotFound(ProposalId),
    /// The proposal left the Inbox, so the user cannot decide on it again.
    #[error("proposal {0} is already {1}")]
    Settled(ProposalId, &'static str),
    /// A resolve named a file that has no conflict in the proposal.
    #[error("proposal {0} has no conflict in {1}")]
    NoConflict(ProposalId, String),
    /// The checked write refused a file for a reason that is not a merge
    /// conflict, such as a root that the daemon does not admit.
    #[error("the daemon could not write the proposal: {0}")]
    WriteFailed(String),
    /// The proposal files could not be read or written.
    #[error(transparent)]
    Store(#[from] anyhow::Error),
}

pub type ProposalResult<T> = Result<T, ProposalError>;

/// The proposals of one daemon.
///
/// The files hold the proposals. The store also remembers, in memory, the
/// proposal of the turn that each session runs now. A second write of the
/// same turn extends that proposal and does not make a new one.
#[derive(Debug)]
pub struct ProposalStore {
    files: store::ProposalFiles,
    /// The proposal of the current turn, by session id.
    turns: Mutex<HashMap<String, ProposalId>>,
    /// One writer at a time. A write can change several files: the proposal
    /// of the turn and each older proposal that it supersedes.
    write: Mutex<()>,
    /// One accept or resolve at a time. It stays locked across the file
    /// writes, so two accepts of one proposal cannot both write.
    settle: tokio::sync::Mutex<()>,
    /// The event bus of the daemon. The server sets it at bind. A store
    /// without a bus, as in a unit test, changes its files and sends nothing.
    events: OnceLock<broadcast::Sender<SessionEventMessage>>,
}

impl ProposalStore {
    /// A store over `dir`. The store creates `dir` at the first write.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            files: store::ProposalFiles::new(dir),
            turns: Mutex::new(HashMap::new()),
            write: Mutex::new(()),
            settle: tokio::sync::Mutex::new(()),
            events: OnceLock::new(),
        }
    }

    /// Send `proposal_changed` on `events` after each change. Returns false
    /// when the store has a bus already; the first bus stays.
    pub fn set_events(&self, events: broadcast::Sender<SessionEventMessage>) -> bool {
        self.events.set(events).is_ok()
    }

    /// Tell each client that the proposal `id` changed. The store calls this
    /// after it writes the file, so a client that reads the proposal again
    /// sees the change.
    fn announce(&self, id: ProposalId) {
        if let Some(events) = self.events.get() {
            crate::event_emitter::emit_event(events, crate::event_map::proposal_changed(id));
        }
    }

    /// Record one proposed write of `session`.
    ///
    /// The write extends the proposal of the current turn of `session`, when
    /// that proposal is still open and has the same author. Else the write
    /// starts a new proposal. Each older pending proposal of the same author
    /// that writes the same path becomes `Superseded`, with a link to this one.
    ///
    /// A second write of the same path in one turn replaces the new text and
    /// keeps the first base, because the disk did not change between the two.
    pub fn record_write(
        &self,
        author: ProposalAuthor,
        session: &SessionId,
        root: PhysicalRoot,
        path: &str,
        base: ExpectedBase,
        new_text: String,
    ) -> ProposalResult<Proposal> {
        let _write = self.write.lock().unwrap_or_else(|e| e.into_inner());
        let write = ProposedWrite {
            root: root.clone(),
            path: path.to_string(),
            base,
            new_text,
        };
        let turn = self.turn_proposal(session, &author)?;
        let proposal = match turn {
            Some(id) => self
                .files
                .update(&id, |proposal| {
                    extend(proposal, write);
                    Ok(proposal.clone())
                })?
                .ok_or(ProposalError::NotFound(id))?,
            None => {
                let mut proposal = Proposal {
                    id: ProposalId::generate(),
                    author: author.clone(),
                    session: Some(session.clone()),
                    title: String::new(),
                    rationale: None,
                    created_at: Utc::now(),
                    state: ProposalState::Open,
                    writes: Vec::new(),
                };
                extend(&mut proposal, write);
                self.files.create(&proposal)?;
                self.turns
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(session.as_str().to_string(), proposal.id);
                proposal
            }
        };
        let superseded = self.supersede(&proposal, &root, path)?;
        self.announce(proposal.id);
        for id in superseded {
            self.announce(id);
        }
        Ok(proposal)
    }

    /// The new text that the open turn proposal of `session` holds for
    /// `root`/`path`, or `None` when the turn proposes no write of that path.
    ///
    /// A second note write of the same path in one turn builds on this text,
    /// because the disk does not hold the first write.
    pub fn turn_text(
        &self,
        author: &ProposalAuthor,
        session: &SessionId,
        root: &PhysicalRoot,
        path: &str,
    ) -> ProposalResult<Option<String>> {
        let Some(id) = self.turn_proposal(session, author)? else {
            return Ok(None);
        };
        Ok(self.files.read(&id)?.and_then(|p| {
            p.writes
                .into_iter()
                .find(|w| w.root == *root && w.path == path)
                .map(|w| w.new_text)
        }))
    }

    /// Forget the proposal of the current turn of `session`. The next write
    /// of `session` starts a new proposal.
    pub fn end_turn(&self, session: &SessionId) {
        self.forget_session(session.as_str());
    }

    /// Forget the turn proposal of a session that ended.
    pub(crate) fn forget_session(&self, session_id: &str) {
        self.turns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session_id);
    }

    /// Whether the store remembers a turn proposal of `session_id`.
    pub(crate) fn has_turn(&self, session_id: &str) -> bool {
        self.turns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(session_id)
    }

    /// The proposals in the Inbox, oldest first. With `all`, every stored
    /// proposal, also the accepted, rejected and dismissed ones.
    pub fn list(&self, all: bool) -> ProposalResult<Vec<Proposal>> {
        let mut proposals = self.files.all()?;
        if !all {
            proposals.retain(|p| p.state.is_listed());
        }
        Ok(proposals)
    }

    /// The proposal with `id`.
    pub fn get(&self, id: &ProposalId) -> ProposalResult<Proposal> {
        self.files.read(id)?.ok_or(ProposalError::NotFound(*id))
    }

    /// Reject the proposal. The files stay as they are, and the proposal
    /// keeps the reason.
    pub fn reject(&self, id: &ProposalId, reason: Option<String>) -> ProposalResult<Proposal> {
        self.settle(id, ProposalState::Rejected { reason })
    }

    /// Take the proposal out of the Inbox with no decision. The daemon keeps
    /// its file.
    pub fn dismiss(&self, id: &ProposalId) -> ProposalResult<Proposal> {
        self.settle(id, ProposalState::Dismissed)
    }

    /// The open proposal of the current turn of `session`, when it has the
    /// same author.
    fn turn_proposal(
        &self,
        session: &SessionId,
        author: &ProposalAuthor,
    ) -> ProposalResult<Option<ProposalId>> {
        let current = self
            .turns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(session.as_str())
            .copied();
        let Some(id) = current else {
            return Ok(None);
        };
        Ok(self
            .files
            .read(&id)?
            .filter(|p| p.state == ProposalState::Open && p.author == *author)
            .map(|p| p.id))
    }

    /// Mark each older pending proposal of the same author that writes
    /// `root`/`path` as superseded by `newer`. Returns the ids that changed.
    fn supersede(
        &self,
        newer: &Proposal,
        root: &PhysicalRoot,
        path: &str,
    ) -> ProposalResult<Vec<ProposalId>> {
        let mut superseded = Vec::new();
        for older in self.files.all()? {
            if older.id == newer.id
                || older.author != newer.author
                || !older.state.is_pending()
                || !older.writes_path(root, path)
            {
                continue;
            }
            self.files.update(&older.id, |p| {
                p.state = ProposalState::Superseded { by: newer.id };
                Ok(())
            })?;
            superseded.push(older.id);
        }
        Ok(superseded)
    }

    /// Move a proposal in the Inbox to a state out of the Inbox.
    fn settle(&self, id: &ProposalId, state: ProposalState) -> ProposalResult<Proposal> {
        let _write = self.write.lock().unwrap_or_else(|e| e.into_inner());
        let settled = self.files.update(id, |proposal| {
            if !proposal.state.is_listed() {
                return Ok(Err(ProposalError::Settled(
                    *id,
                    state_name(&proposal.state),
                )));
            }
            proposal.state = state;
            Ok(Ok(proposal.clone()))
        })?;
        let settled = settled.ok_or(ProposalError::NotFound(*id))??;
        self.announce(settled.id);
        Ok(settled)
    }
}

/// Add `write` to `proposal`, or replace the new text of its earlier write of
/// the same path. Then name the proposal after its paths.
fn extend(proposal: &mut Proposal, write: ProposedWrite) {
    match proposal
        .writes
        .iter_mut()
        .find(|w| w.root == write.root && w.path == write.path)
    {
        Some(earlier) => earlier.new_text = write.new_text,
        None => proposal.writes.push(write),
    }
    proposal.title = match proposal.writes.as_slice() {
        [only] => format!("Change {}", only.path),
        writes => format!("Change {} notes", writes.len()),
    };
}

/// The wire name of a state, for a refusal.
fn state_name(state: &ProposalState) -> &'static str {
    match state {
        ProposalState::Open => "open",
        ProposalState::Stale => "stale",
        ProposalState::Conflicted { .. } => "conflicted",
        ProposalState::Accepted => "accepted",
        ProposalState::Rejected { .. } => "rejected",
        ProposalState::Superseded { .. } => "superseded",
        ProposalState::Dismissed => "dismissed",
    }
}

#[cfg(test)]
mod tests;
