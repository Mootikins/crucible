//! A proposal as a diffset: the base of each write, to its new text.
//!
//! The base side is the text that the writer read before it proposed the
//! write. The current side is the text that an accept puts on disk. A
//! proposal does not delete or rename a file, so each file is added or
//! modified.

use crucible_core::diff::{DiffFileEntry, DiffFileText, FileStatus};
use crucible_core::file_write::ExpectedBase;
use crucible_core::note_edit::disk_hash;
use crucible_core::proposal::{Proposal, ProposalId, ProposedWrite};
use crucible_core::session::PhysicalRoot;
use crucible_core::types::acp::MAX_DIFF_BYTES;

use super::stale::{read_disk, Disk};
use super::{ProposalError, ProposalResult, ProposalStore};
use crate::review::line_counts;

impl ProposalStore {
    /// The files of the proposal `id`, in the order of its writes, with
    /// their counts.
    pub(crate) fn diff_files(&self, id: &ProposalId) -> ProposalResult<Vec<DiffFileEntry>> {
        let proposal = self.get(id)?;
        Ok(proposal.changes().map(|w| entry(&proposal, w)).collect())
    }

    /// The two texts of the file `root`/`path` of the proposal `id`.
    pub(crate) fn diff_text(
        &self,
        id: &ProposalId,
        root: &PhysicalRoot,
        path: &str,
    ) -> ProposalResult<DiffFileText> {
        let proposal = self.get(id)?;
        let write = proposal
            .writes
            .iter()
            .find(|w| w.root == *root && w.path == path)
            .ok_or_else(|| ProposalError::NoWrite(*id, path.to_string()))?;
        Ok(DiffFileText {
            base_text: old_text(&proposal, write).filter(|t| t.len() <= MAX_DIFF_BYTES),
            current_text: (!write.remove)
                .then(|| write.new_text.clone())
                .filter(|t| t.len() <= MAX_DIFF_BYTES),
        })
    }
}

/// The text of the base side of `write`, or `None` when the file was absent.
///
/// A hash base has no text. The disk text shows in its place while the disk
/// still has that hash. An unchecked base is the disk as it is now.
fn base_text(write: &ProposedWrite) -> Option<String> {
    let disk = || match read_disk(write) {
        Some(Disk::Text(text)) => Some(text),
        Some(Disk::Absent | Disk::NotText) | None => None,
    };
    match &write.base {
        ExpectedBase::Absent => None,
        ExpectedBase::Text { text, .. } => Some(text.clone()),
        ExpectedBase::Hash { hash } => disk().filter(|text| disk_hash(text) == *hash),
        ExpectedBase::Unchecked => disk(),
    }
}

/// The old side of `write`: for the new half of a move, the text that the
/// moved file held.
fn old_text(proposal: &Proposal, write: &ProposedWrite) -> Option<String> {
    base_text(proposal.moved_from(write).unwrap_or(write))
}

fn entry(proposal: &Proposal, write: &ProposedWrite) -> DiffFileEntry {
    let base = old_text(proposal, write);
    let status = match (&write.base, &base) {
        _ if write.remove => FileStatus::Deleted,
        _ if write.moved_from.is_some() => FileStatus::Renamed {
            from: write.moved_from.clone().unwrap_or_default(),
        },
        (ExpectedBase::Absent, _) | (ExpectedBase::Unchecked, None) => FileStatus::Added,
        _ => FileStatus::Modified,
    };
    let before = base.as_deref().unwrap_or_default();
    let too_large = before.len() > MAX_DIFF_BYTES || write.new_text.len() > MAX_DIFF_BYTES;
    let (added, removed) = if too_large {
        (0, 0)
    } else {
        line_counts(before, &write.new_text)
    };
    DiffFileEntry {
        root: write.root.clone(),
        path: write.path.clone(),
        status,
        added,
        removed,
        binary: false,
        too_large,
    }
}
