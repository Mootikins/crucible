//! The proposal files: one JSON file for each proposal under
//! `<data_home>/proposals/`.
//!
//! Each file is a [`RegistryStore`]: a sidecar lock, a read, a change and an
//! atomic rename. A reader without the lock sees a complete file. The store
//! never removes a file, so the rejection history survives.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use crucible_core::proposal::{Proposal, ProposalId};

use crate::registry_store::RegistryStore;

/// The name of the store directory in the daemon data home.
const DIR: &str = "proposals";

/// Where a daemon with the data home `data_home` keeps its proposals.
pub fn proposals_root(data_home: &Path) -> PathBuf {
    data_home.join(DIR)
}

/// The proposal store of the daemon whose review snapshots are in
/// `snapshot_root`.
///
/// The daemon passes `<data_home>/review-snapshots` as the snapshot root, so
/// the store is `<data_home>/proposals`. A snapshot root with no parent keeps
/// the store inside itself.
pub fn root_beside_snapshots(snapshot_root: &Path) -> PathBuf {
    match snapshot_root.parent() {
        Some(data_home) => proposals_root(data_home),
        None => snapshot_root.join(DIR),
    }
}

/// The proposal files of one daemon.
#[derive(Debug, Clone)]
pub(super) struct ProposalFiles {
    dir: PathBuf,
}

impl ProposalFiles {
    pub(super) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The proposal with `id`, or `None` when no file has that id.
    pub(super) fn read(&self, id: &ProposalId) -> Result<Option<Proposal>> {
        self.file(id).read()
    }

    /// Write a new proposal. The store refuses an id that it already holds.
    pub(super) fn create(&self, proposal: &Proposal) -> Result<()> {
        self.file(&proposal.id).update(|slot| {
            if slot.is_some() {
                bail!("proposal {} is already stored", proposal.id);
            }
            *slot = Some(proposal.clone());
            Ok(())
        })
    }

    /// Change the proposal with `id` under its lock.
    ///
    /// The result is `None` when no file has that id. Then the store writes
    /// nothing.
    pub(super) fn update<R>(
        &self,
        id: &ProposalId,
        change: impl FnOnce(&mut Proposal) -> Result<R>,
    ) -> Result<Option<R>> {
        let file = self.file(id);
        // `update` writes the value back. Thus a read first keeps an absent
        // id absent, and does not write a `null` file for it.
        if file.read()?.is_none() {
            return Ok(None);
        }
        file.update(|slot| match slot.as_mut() {
            Some(proposal) => change(proposal).map(Some),
            None => Ok(None),
        })
    }

    /// Every stored proposal, oldest first.
    ///
    /// A file that does not parse is left out with a warning, so that one
    /// bad file does not hide every other proposal.
    pub(super) fn all(&self) -> Result<Vec<Proposal>> {
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(e).with_context(|| format!("failed to read {}", self.dir.display()))
            }
        };
        let mut proposals = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<ProposalId>().ok())
            else {
                continue;
            };
            match self.read(&id) {
                Ok(Some(proposal)) => proposals.push(proposal),
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %format!("{e:#}"), "skipping a proposal file"),
            }
        }
        proposals.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        Ok(proposals)
    }

    /// The file of one proposal. A `ProposalId` is a UUID, so the name never
    /// leaves the directory.
    fn file(&self, id: &ProposalId) -> RegistryStore<Option<Proposal>> {
        RegistryStore::new(self.dir.join(format!("{id}.json")))
    }
}
