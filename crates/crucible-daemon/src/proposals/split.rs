//! Accept or reject some of the files of a proposal.
//!
//! The daemon moves the named files into a new proposal of the same author,
//! then accepts or rejects that proposal. Each proposal keeps its own file,
//! so the history keeps each decision and the files that it covered.

use std::path::PathBuf;

use crucible_core::proposal::{FileConflict, Proposal, ProposalId, ProposalState};

use super::{extend, state_name, ProposalError, ProposalResult, ProposalStore};

impl ProposalStore {
    /// Write the files `paths` of the proposal `id`. The other files stay in
    /// `id`. With no paths, write every file, as [`ProposalStore::accept`].
    ///
    /// The answer is the proposal that holds the files `paths`. When a file
    /// conflicts, that proposal becomes `Conflicted`.
    pub async fn accept_paths(
        &self,
        id: &ProposalId,
        paths: &[String],
        kilns: &[PathBuf],
    ) -> ProposalResult<Proposal> {
        let id = self.split_off(id, paths)?;
        self.accept(&id, kilns).await
    }

    /// Reject the files `paths` of the proposal `id`. The other files stay
    /// in `id`. With no paths, reject every file, as
    /// [`ProposalStore::reject`].
    pub fn reject_paths(
        &self,
        id: &ProposalId,
        paths: &[String],
        reason: Option<String>,
    ) -> ProposalResult<Proposal> {
        let id = self.split_off(id, paths)?;
        self.reject(&id, reason)
    }

    /// Move the writes of `paths` out of the proposal `id` into a new
    /// proposal. Returns the id of the proposal that holds exactly those
    /// writes: `id` itself when `paths` is empty or names every write.
    ///
    /// Each conflict of a moved file moves with it. A proposal with no
    /// conflict left becomes `Open`; the stale check then compares it with
    /// the disk again.
    pub(crate) fn split_off(
        &self,
        id: &ProposalId,
        paths: &[String],
    ) -> ProposalResult<ProposalId> {
        let _write = self.write.lock().unwrap_or_else(|e| e.into_inner());
        let original = self.get(id)?;
        if !original.state.is_pending() {
            return Err(ProposalError::Settled(*id, state_name(&original.state)));
        }
        if let Some(missing) = paths
            .iter()
            .find(|path| !original.writes.iter().any(|w| &w.path == *path))
        {
            return Err(ProposalError::NoWrite(*id, missing.clone()));
        }
        let (taken, kept): (Vec<_>, Vec<_>) = original
            .writes
            .iter()
            .cloned()
            .partition(|w| paths.contains(&w.path));
        if paths.is_empty() || kept.is_empty() {
            return Ok(*id);
        }
        let (moved, stays) = conflicts_of(&original.state, paths);

        let mut split = Proposal {
            id: ProposalId::generate(),
            state: moved,
            writes: Vec::new(),
            ..original.clone()
        };
        for write in taken {
            extend(&mut split, write);
        }
        self.files.create(&split)?;
        self.files
            .update(id, |proposal| {
                proposal.writes.clear();
                for write in kept {
                    extend(proposal, write);
                }
                proposal.state = stays;
                Ok(())
            })?
            .ok_or(ProposalError::NotFound(*id))?;
        self.announce(split.id);
        self.announce(*id);
        Ok(split.id)
    }
}

/// The states of the two parts of a split: the part that holds `paths`, and
/// the part that keeps the other files.
fn conflicts_of(state: &ProposalState, paths: &[String]) -> (ProposalState, ProposalState) {
    let ProposalState::Conflicted { files } = state else {
        return (state.clone(), state.clone());
    };
    let (moved, stays): (Vec<FileConflict>, Vec<FileConflict>) =
        files.iter().cloned().partition(|c| paths.contains(&c.path));
    let state = |files: Vec<FileConflict>| {
        if files.is_empty() {
            ProposalState::Open
        } else {
            ProposalState::Conflicted { files }
        }
    };
    (state(moved), state(stays))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::file_write::ExpectedBase;
    use crucible_core::proposal::ProposalAuthor;
    use crucible_core::session::{PhysicalRoot, SessionId};
    use tempfile::TempDir;

    /// A store and a kiln, and one open proposal that adds `a.md` and `b.md`.
    fn two_files() -> (TempDir, ProposalStore, PathBuf, Proposal) {
        let dir = TempDir::new().unwrap();
        let store = ProposalStore::new(dir.path().join("proposals"));
        let kiln = dir.path().join("kiln");
        std::fs::create_dir_all(&kiln).unwrap();
        let kiln = kiln.canonicalize().unwrap();
        let mut proposal = None;
        for path in ["a.md", "b.md"] {
            proposal = Some(
                store
                    .record_write(
                        ProposalAuthor::Plugin {
                            name: "reflection".into(),
                        },
                        &SessionId::parse("aux-1").unwrap(),
                        PhysicalRoot::from_top_level(&kiln),
                        path,
                        ExpectedBase::Absent,
                        format!("{path}\n"),
                    )
                    .unwrap(),
            );
        }
        (dir, store, kiln, proposal.unwrap())
    }

    fn paths(proposal: &Proposal) -> Vec<&str> {
        proposal.writes.iter().map(|w| w.path.as_str()).collect()
    }

    #[tokio::test]
    async fn accept_of_one_file_writes_only_that_file() {
        let (_dir, store, kiln, proposal) = two_files();

        let accepted = store
            .accept_paths(&proposal.id, &["b.md".into()], std::slice::from_ref(&kiln))
            .await
            .unwrap();

        assert_ne!(accepted.id, proposal.id);
        assert_eq!(accepted.state, ProposalState::Accepted);
        assert_eq!(paths(&accepted), vec!["b.md"]);
        assert_eq!(accepted.title, "Change b.md");
        assert_eq!(
            std::fs::read_to_string(kiln.join("b.md")).unwrap(),
            "b.md\n"
        );
        assert!(!kiln.join("a.md").exists());

        let rest = store.get(&proposal.id).unwrap();
        assert_eq!(rest.state, ProposalState::Open);
        assert_eq!(paths(&rest), vec!["a.md"]);
        assert_eq!(rest.title, "Change a.md");
    }

    #[tokio::test]
    async fn reject_of_one_file_keeps_the_other_file_open() {
        let (_dir, store, kiln, proposal) = two_files();

        let rejected = store
            .reject_paths(&proposal.id, &["a.md".into()], Some("no".into()))
            .unwrap();
        assert_eq!(
            rejected.state,
            ProposalState::Rejected {
                reason: Some("no".into())
            }
        );
        assert_eq!(paths(&rejected), vec!["a.md"]);
        assert_eq!(paths(&store.get(&proposal.id).unwrap()), vec!["b.md"]);
        assert!(!kiln.join("a.md").exists());

        // A path that the proposal does not write is the caller's error.
        let error = store
            .reject_paths(&proposal.id, &["a.md".into()], None)
            .unwrap_err();
        assert!(matches!(error, ProposalError::NoWrite(..)), "{error:?}");

        // Every path is the whole proposal: no split.
        let whole = store
            .reject_paths(&proposal.id, &["b.md".into()], None)
            .unwrap();
        assert_eq!(whole.id, proposal.id);
        assert_eq!(store.list(true).unwrap().len(), 2);
    }

    #[test]
    fn a_conflict_moves_with_its_file() {
        let conflict = |path: &str| FileConflict {
            root: PhysicalRoot::from_top_level("/kiln"),
            path: path.into(),
            disk_text: String::new(),
            merged_text: String::new(),
            regions: vec![],
        };
        let state = ProposalState::Conflicted {
            files: vec![conflict("a.md")],
        };
        let (moved, stays) = conflicts_of(&state, &["a.md".into()]);
        assert_eq!(moved, state);
        assert_eq!(stays, ProposalState::Open);
    }
}
