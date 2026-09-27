//! Accept or reject some of the files of a proposal.
//!
//! The daemon moves the named files into a new proposal of the same author,
//! then accepts or rejects that proposal. Each proposal keeps its own file,
//! so the history keeps each decision and the files that it covered.

use std::path::PathBuf;

use crucible_core::proposal::{FileConflict, Proposal, ProposalFile, ProposalId, ProposalState};

use super::{
    ensure_available, extend, state_name, ProposalError, ProposalResult, ProposalStore, Reservation,
};

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
        self.accept_files(id, paths, &[], kilns).await
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
        self.reject_files(id, paths, &[], reason)
    }

    /// Accept an explicit selection, reserving both halves before file I/O.
    pub async fn accept_files(
        &self,
        id: &ProposalId,
        paths: &[String],
        files: &[ProposalFile],
        kilns: &[PathBuf],
    ) -> ProposalResult<Proposal> {
        let (_reservation, selected) = self.reserve_selection(id, paths, files)?;
        let proposal = self.get(&selected)?;
        self.write_all(proposal, kilns).await
    }

    /// Reject a selection atomically with respect to other proposal mutations.
    pub fn reject_files(
        &self,
        id: &ProposalId,
        paths: &[String],
        files: &[ProposalFile],
        reason: Option<String>,
    ) -> ProposalResult<Proposal> {
        let (_reservation, selected) = self.reserve_selection(id, paths, files)?;
        self.settle_locked(&selected, ProposalState::Rejected { reason })
    }

    /// Hold `id`, split the selection into its own proposal, and hold that
    /// proposal too. Returns the reservation and the id that holds the
    /// selection: `id` itself for an empty or a whole selection.
    ///
    /// One lock covers the check, the split and the reservation, so no other
    /// decision can take either half between them.
    fn reserve_selection(
        &self,
        id: &ProposalId,
        paths: &[String],
        files: &[ProposalFile],
    ) -> ProposalResult<(Reservation<'_>, ProposalId)> {
        let mut reserved = self.write.lock().unwrap_or_else(|e| e.into_inner());
        ensure_available(&reserved, id)?;
        let selected = select_files(&self.get(id)?, paths, files)?;
        let split = self.split_locked(id, &selected)?;
        let mut ids = vec![*id];
        if split != *id {
            ids.push(split);
        }
        reserved.held.extend(ids.iter().copied());
        Ok((Reservation { store: self, ids }, split))
    }

    fn split_locked(&self, id: &ProposalId, files: &[ProposalFile]) -> ProposalResult<ProposalId> {
        let original = self.get(id)?;
        if !original.state.is_pending() {
            return Err(ProposalError::Settled(*id, state_name(&original.state)));
        }
        let (taken, kept): (Vec<_>, Vec<_>) = original
            .writes
            .iter()
            .cloned()
            .partition(|w| files.iter().any(|f| f.root == w.root && f.path == w.path));
        if files.is_empty() || kept.is_empty() {
            return Ok(*id);
        }
        let (moved, stays) = conflicts_of(&original.state, files);

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

/// Resolve legacy names against the stored proposal before any mutation.
pub(super) fn select_files(
    proposal: &Proposal,
    paths: &[String],
    files: &[ProposalFile],
) -> ProposalResult<Vec<ProposalFile>> {
    if !paths.is_empty() && !files.is_empty() {
        return Err(ProposalError::MixedSelection);
    }
    for file in files {
        if !proposal.writes_path(&file.root, &file.path) {
            return Err(ProposalError::NoWrite(
                proposal.id,
                format!("{}:{}", file.root.as_path().display(), file.path),
            ));
        }
    }
    let mut selected = files.to_vec();
    named(proposal, paths, &mut selected)?;
    // A move is one decision: its two halves go together.
    let partners: Vec<ProposalFile> = selected
        .iter()
        .filter_map(|f| {
            let write = proposal
                .writes
                .iter()
                .find(|w| w.root == f.root && w.path == f.path)?;
            proposal
                .moved_from(write)
                .or_else(|| proposal.moved_to(write))
                .map(|w| ProposalFile {
                    root: w.root.clone(),
                    path: w.path.clone(),
                })
        })
        .collect();
    for partner in partners {
        if !selected.contains(&partner) {
            selected.push(partner);
        }
    }
    Ok(selected)
}

/// Add the writes that `paths` name to `selected`.
fn named(
    proposal: &Proposal,
    paths: &[String],
    selected: &mut Vec<ProposalFile>,
) -> ProposalResult<()> {
    for path in paths {
        let matches: Vec<_> = proposal.writes.iter().filter(|w| &w.path == path).collect();
        match matches.as_slice() {
            [] => return Err(ProposalError::NoWrite(proposal.id, path.clone())),
            [write] => selected.push(ProposalFile {
                root: write.root.clone(),
                path: path.clone(),
            }),
            _ => {
                return Err(ProposalError::Ambiguous(
                    path.clone(),
                    matches
                        .iter()
                        .map(|w| w.root.as_path().display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                ))
            }
        }
    }
    Ok(())
}

/// The states of the two parts of a split: the part that holds `paths`, and
/// the part that keeps the other files.
fn conflicts_of(
    state: &ProposalState,
    selected: &[ProposalFile],
) -> (ProposalState, ProposalState) {
    let ProposalState::Conflicted { files } = state else {
        return (state.clone(), state.clone());
    };
    let (moved, stays): (Vec<FileConflict>, Vec<FileConflict>) =
        files.iter().cloned().partition(|c| {
            selected
                .iter()
                .any(|f| f.root == c.root && f.path == c.path)
        });
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
        let (moved, stays) = conflicts_of(
            &state,
            &[ProposalFile {
                root: PhysicalRoot::from_top_level("/kiln"),
                path: "a.md".into(),
            }],
        );
        assert_eq!(moved, state);
        assert_eq!(stays, ProposalState::Open);
    }
}
