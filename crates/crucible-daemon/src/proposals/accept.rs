//! Accept and resolve: the two ways that a proposal writes its files.
//!
//! Both call `write_many_for_roots`, so the files of one proposal change as
//! one set. When one file has a merge conflict, the daemon writes no file,
//! and the proposal becomes `Conflicted` with a `FileConflict` for each
//! file that conflicts.

use std::path::PathBuf;

use crucible_core::file_write::ExpectedBase;
use crucible_core::note_edit::disk_hash;
use crucible_core::note_merge::merge3;
use crucible_core::proposal::{
    FileConflict, Proposal, ProposalFile, ProposalId, ProposalState, ProposedWrite,
};
use serde_json::Value;

use super::stale::{base_holds, read_disk, target, Disk};
use super::{ProposalError, ProposalResult, ProposalStore};
use crate::file_write::{write_many_for_roots, CheckedPut};

impl ProposalStore {
    /// Write every file of the proposal `id`.
    ///
    /// `kilns` are the roots that the daemon admits for a write. A file whose
    /// disk moved since its base merges. If a file conflicts, the daemon
    /// writes nothing, and the proposal becomes `Conflicted`.
    pub async fn accept(&self, id: &ProposalId, kilns: &[PathBuf]) -> ProposalResult<Proposal> {
        self.accept_files(id, &[], &[], kilns).await
    }

    /// Write `text` for the conflicted file `path` of the proposal `id`.
    ///
    /// The settled text replaces the proposed text, and the disk text of the
    /// conflict becomes the base. When no other file conflicts, the daemon
    /// writes the whole proposal. If the disk moved again, the proposal
    /// stays `Conflicted` with the new regions.
    pub async fn resolve(
        &self,
        id: &ProposalId,
        path: &str,
        text: &str,
        kilns: &[PathBuf],
    ) -> ProposalResult<Proposal> {
        self.resolve_file(id, path, None, text, kilns).await
    }

    /// Resolve a root-qualified file, or a unique legacy path.
    pub async fn resolve_file(
        &self,
        id: &ProposalId,
        path: &str,
        root: Option<&crucible_core::session::PhysicalRoot>,
        text: &str,
        kilns: &[PathBuf],
    ) -> ProposalResult<Proposal> {
        let _reservation = self.reserve(id)?;
        let mut proposal = self.get(id)?;
        let selected = match root {
            Some(root) => super::split::select_files(
                &proposal,
                &[],
                &[ProposalFile {
                    root: root.clone(),
                    path: path.to_string(),
                }],
            )?,
            None => super::split::select_files(&proposal, &[path.to_string()], &[])?,
        };
        let selected = &selected[0];
        let ProposalState::Conflicted { files } = &mut proposal.state else {
            return Err(ProposalError::NoConflict(*id, path.to_string()));
        };
        let Some(index) = files
            .iter()
            .position(|c| c.path == selected.path && c.root == selected.root)
        else {
            return Err(ProposalError::NoConflict(*id, path.to_string()));
        };
        let settled = files.remove(index);
        let remaining = !files.is_empty();
        let write = proposal
            .writes
            .iter_mut()
            .find(|w| w.root == settled.root && w.path == settled.path)
            .ok_or_else(|| ProposalError::NoConflict(*id, path.to_string()))?;
        // The settled text keeps the file, also when the proposal deleted it.
        // A move whose old file stays is a copy, not a move.
        write.new_text = text.to_string();
        let kept = std::mem::take(&mut write.remove).then(|| write.path.clone());
        write.base = ExpectedBase::Text {
            hash: disk_hash(&settled.disk_text),
            text: settled.disk_text,
        };
        if let Some(kept) = kept {
            for other in proposal.writes.iter_mut() {
                if other.root == settled.root && other.moved_from.as_deref() == Some(&kept) {
                    other.moved_from = None;
                }
            }
        }
        if remaining {
            // Another file still waits for the user. Keep the settled text,
            // and write nothing until every conflict has a settled text.
            let saved = self.save(proposal)?;
            self.announce(saved.id);
            return Ok(saved);
        }
        self.write_all(proposal, kilns).await
    }

    /// Write every file of `proposal` as one set. Then keep the new state
    /// and announce it. The caller holds a reservation.
    pub(super) async fn write_all(
        &self,
        mut proposal: Proposal,
        kilns: &[PathBuf],
    ) -> ProposalResult<Proposal> {
        let puts = proposal
            .writes
            .iter()
            .map(|w| CheckedPut {
                path: target(w).to_string_lossy().into_owned(),
                content: w.new_text.clone(),
                base: w.base.clone(),
                remove: w.remove,
            })
            .collect();
        let answer = write_many_for_roots(puts, kilns, &[]).await;
        proposal.state = if answer["ok"] == true {
            ProposalState::Accepted
        } else if is_conflict(&answer) {
            let mut files: Vec<FileConflict> =
                proposal.writes.iter().filter_map(conflict_of).collect();
            if files.is_empty() {
                // The disk moved between the write and this check. The failed
                // answer still holds the conflict of its path.
                files.extend(conflict_from_answer(&proposal, &answer));
            }
            ProposalState::Conflicted { files }
        } else {
            return Err(ProposalError::WriteFailed(failure_message(&answer)));
        };
        let saved = self.save(proposal)?;
        self.announce(saved.id);
        Ok(saved)
    }

    /// Keep `proposal` as it is now, in its file.
    fn save(&self, proposal: Proposal) -> ProposalResult<Proposal> {
        let _write = self.write.lock().unwrap_or_else(|e| e.into_inner());
        let id = proposal.id;
        self.files
            .update(&id, |stored| {
                *stored = proposal;
                Ok(stored.clone())
            })?
            .ok_or(ProposalError::NotFound(id))
    }
}

/// Whether a failed answer of the checked write is a merge conflict, and not
/// a refusal or an I/O error.
fn is_conflict(answer: &Value) -> bool {
    answer.get("failure").is_none()
}

fn failure_message(answer: &Value) -> String {
    let message = answer["message"].as_str().unwrap_or("the write failed");
    match answer["path"].as_str() {
        Some(path) => format!("{path}: {message}"),
        None => message.to_string(),
    }
}

/// The conflict of `write` against the disk now, or `None` when the write
/// merges cleanly. This repeats the merge rule of the checked write.
fn conflict_of(write: &ProposedWrite) -> Option<FileConflict> {
    let disk = match read_disk(write)? {
        Disk::Text(text) => Some(text),
        Disk::Absent => None,
        // The checked write refuses a file that is not text. That is not a
        // conflict that a person can settle in the merge view.
        Disk::NotText => return None,
    };
    let state = disk.clone().map_or(Disk::Absent, Disk::Text);
    if base_holds(&write.base, &state) {
        return None;
    }
    let disk = disk.unwrap_or_default();
    if write.remove {
        // A deletion does not merge with an edit: the person settles from the
        // disk text, and a settled text keeps the file.
        return Some(FileConflict {
            root: write.root.clone(),
            path: write.path.clone(),
            merged_text: disk.clone(),
            disk_text: disk,
            regions: vec![],
        });
    }
    let (merge_base, always) = match &write.base {
        ExpectedBase::Text { text, .. } => (text.as_str(), false),
        // No base text to merge from: any file on disk is a conflict. A merge
        // with an empty base still shows the regions.
        ExpectedBase::Absent | ExpectedBase::Hash { .. } => ("", true),
        ExpectedBase::Unchecked => return None,
    };
    let merge = merge3(merge_base, &write.new_text, &disk);
    (always || !merge.regions.is_empty()).then(|| FileConflict {
        root: write.root.clone(),
        path: write.path.clone(),
        disk_text: disk,
        merged_text: merge.text,
        regions: merge.regions,
    })
}

/// The conflict that the failed answer of the checked write names.
fn conflict_from_answer(proposal: &Proposal, answer: &Value) -> Option<FileConflict> {
    let failed = answer["path"].as_str()?;
    let write = proposal
        .writes
        .iter()
        .find(|w| target(w).to_string_lossy() == failed)?;
    Some(FileConflict {
        root: write.root.clone(),
        path: write.path.clone(),
        disk_text: answer["current_content"].as_str()?.to_string(),
        merged_text: answer["merged_content"].as_str()?.to_string(),
        regions: serde_json::from_value(answer["regions"].clone()).ok()?,
    })
}
