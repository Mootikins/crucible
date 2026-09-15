//! Which store holds a review root's snapshots.
//!
//! A review root is either inside a git repository or it is not. Inside one,
//! every snapshot is a tree `git write-tree` produced and git answers every
//! later question about it. Outside one, [`super::plain_store`] answers the
//! same questions over a manifest of content hashes. Those two are the whole
//! set, and this enum is the only place the daemon chooses between them.
//!
//! **The snapshot id is the source of the backend, not a stored field.**
//! [`RootBackend::of`] reads the arm of a [`SnapshotId`], so a root recorded
//! by an older build is routed by the id its journal already holds. Adding a
//! `backend` field to `RootBase` would have been a second source that a
//! replayed journal could disagree with.
//!
//! [`RootBackend::detect`] is the one place a *path* chooses, and it runs
//! exactly twice: when a ledger opens, and when a human rebases one. Every
//! other caller already holds an id.
//!
//! Every method here is one exhaustive `match self`. The two module-level
//! denies below are what stops a third arm — or a variant added later — from
//! being silenced with `_ =>`; both are needed, for the reason
//! [`crate::tools::surface`] records at length: clippy reports a wildcard
//! covering one remaining variant under a different lint than one covering
//! two or more, and a two-variant enum only ever produces the first.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use std::path::Path;

use crucible_core::session::{PhysicalRoot, SnapshotId};

use super::error::ReviewResult;
use super::git::{self, ChangeKind, IgnoredEntries};
use super::plain_store::PlainStore;
use crate::workspace_snapshot;

/// Where one review root's snapshots live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub(crate) enum RootBackend {
    /// The root is inside a git repository; snapshots are tree SHAs.
    Git,
    /// The root is not; snapshots are manifests in [`PlainStore`].
    Plain,
}

impl RootBackend {
    /// The backend that can read `id`.
    ///
    /// Total, and the one source of the answer (see the module docs).
    pub(crate) fn of(id: &SnapshotId) -> Self {
        match id {
            SnapshotId::Git(_) => Self::Git,
            SnapshotId::Plain(_) => Self::Plain,
        }
    }

    /// How a root must be spelled, and which backend will snapshot it.
    ///
    /// `Git` when `git rev-parse --show-toplevel` answers, so a kiln inside a
    /// repository keeps the backend it has always had and its journal keeps
    /// replaying. Otherwise `Plain`, over the canonical path — which is what
    /// [`PhysicalRoot`] means for a root git cannot resolve symlinks for, and
    /// it has to hold because the root is the first field hashed into a
    /// [`crucible_core::session::HunkId`].
    ///
    /// A root that is not there is an error rather than a `Plain` root: there
    /// is nothing to walk, and `open` must be able to tell the two apart.
    pub(super) async fn detect(root: &Path) -> ReviewResult<(PhysicalRoot, Self)> {
        if let Ok(top) = git::top_level(root).await {
            return Ok((top, Self::Git));
        }
        let canonical = tokio::fs::canonicalize(root).await?;
        Ok((PhysicalRoot::from_top_level(canonical), Self::Plain))
    }

    /// The backend for a root the caller holds only a path to.
    ///
    /// For the worktree watch, which sees paths and never snapshots. A root
    /// that has gone answers `Plain`, whose [`Self::ignored`] is empty — the
    /// same set git would have produced for a directory it cannot read.
    pub(crate) async fn for_path(root: &Path) -> Self {
        match Self::detect(root).await {
            Ok((_, backend)) => backend,
            Err(_) => Self::Plain,
        }
    }

    /// Snapshot `root` as it is now.
    pub(super) async fn capture(self, store: &PlainStore, root: &Path) -> ReviewResult<SnapshotId> {
        match self {
            Self::Git => Ok(SnapshotId::git(
                workspace_snapshot::capture_tree(root).await?,
            )),
            Self::Plain => store.capture(root).await,
        }
    }

    /// Paths differing between two snapshots of `root`, with the kind of
    /// change. Renames are not detected on either side; see
    /// [`git::changed_paths`].
    pub(super) async fn changed_paths(
        self,
        store: &PlainStore,
        root: &Path,
        from: &SnapshotId,
        to: &SnapshotId,
    ) -> ReviewResult<Vec<(String, ChangeKind)>> {
        match self {
            Self::Git => git::changed_paths(root, from, to).await,
            Self::Plain => store.changed_paths(from, to).await,
        }
    }

    /// Absence and an existing empty file are different restore targets.
    pub(super) async fn contains_path(
        self,
        store: &PlainStore,
        root: &Path,
        snap: &SnapshotId,
        path: &str,
    ) -> ReviewResult<bool> {
        match self {
            Self::Git => git::contains_path(root, snap, path).await,
            Self::Plain => store.contains_path(snap, path).await,
        }
    }

    /// File content at `path` inside `snap`.
    ///
    /// `Ok(None)` means the bytes are not UTF-8 on either backend, so
    /// composition skips a binary file without knowing which store answered.
    pub(super) async fn blob(
        self,
        store: &PlainStore,
        root: &Path,
        snap: &SnapshotId,
        path: &str,
    ) -> ReviewResult<Option<String>> {
        match self {
            Self::Git => git::blob(root, snap, path).await,
            Self::Plain => store.blob_at(snap, path).await,
        }
    }

    /// One side of a change: the blob, or empty text when [`ChangeKind`] says
    /// the path does not exist on that side.
    ///
    /// Asking either store for a path it does not hold is an error, not an
    /// empty answer, so the `exists` short-circuit is what lets an add and a
    /// delete go through the same code path as a modification. `Ok(None)`
    /// still means binary — see [`Self::blob`].
    pub(super) async fn blob_or_empty(
        self,
        store: &PlainStore,
        root: &Path,
        snap: &SnapshotId,
        path: &str,
        exists: bool,
    ) -> ReviewResult<Option<String>> {
        if !exists {
            return Ok(Some(String::new()));
        }
        self.blob(store, root, snap, path).await
    }

    /// Whether the store behind this backend still holds `id`.
    ///
    /// `false` rather than an error for an id from the other store: the
    /// question is whether *this* root's store holds it, and `degraded_reason`
    /// — the one caller — has no rendering for a third answer.
    pub(super) async fn snapshot_exists(
        self,
        store: &PlainStore,
        root: &Path,
        id: &SnapshotId,
    ) -> bool {
        match self {
            Self::Git => git::tree_exists(root, id).await,
            Self::Plain => store.exists(id).await,
        }
    }

    /// Claim `snapshots` against collection, for as long as the session holds
    /// them.
    ///
    /// Total rather than incremental on both backends: the claim is rebuilt
    /// from the full list on every call, so a ledger that lost an interval
    /// also loses its claim on that interval's snapshots. The two collectors
    /// differ only in who runs them — `git gc` is the user's, and
    /// [`PlainStore::sweep`] is the daemon's.
    pub(super) async fn keep(
        self,
        store: &PlainStore,
        root: &Path,
        session_id: &str,
        snapshots: &[SnapshotId],
    ) -> ReviewResult<()> {
        match self {
            Self::Git => git::update_keep(root, session_id, snapshots).await,
            Self::Plain => store.keep(root, session_id, snapshots).await,
        }
    }

    /// What must not be watched under `root`, beyond the fixed exclusions the
    /// watch applies itself.
    ///
    /// Empty for a plain root: git's ignore rules are the only source of this
    /// list, and a root outside a repository has none. The watch's own
    /// `always_excluded` already covers [`crucible_core::EXCLUDED_DIRS`], so
    /// the fixed names are still pruned.
    pub(crate) async fn ignored(self, root: &Path) -> ReviewResult<IgnoredEntries> {
        match self {
            Self::Git => git::ignored_entries(root).await,
            Self::Plain => Ok(IgnoredEntries::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;
    use tempfile::TempDir;

    /// A root of the kind `backend` names, holding one file.
    async fn root_for(backend: RootBackend) -> TempDir {
        let dir = TempDir::new().unwrap();
        match backend {
            RootBackend::Git => {
                crate::test_support::init_repo(dir.path(), &[("a.md", "one\n")]).await
            }
            // No `git init`: the premise is that the system temp directory is
            // not inside a checkout. `detect` below asserts it.
            RootBackend::Plain => std::fs::write(dir.path().join("a.md"), "one\n").unwrap(),
        }
        dir
    }

    /// Walked from the compiler's own variant list rather than a literal pair,
    /// so a third backend that forgot one of these seams fails here rather
    /// than at the first kiln that uses it.
    #[tokio::test]
    async fn every_backend_arm_answers_a_capture() {
        for backend in RootBackend::iter() {
            let snaps = TempDir::new().unwrap();
            let store = PlainStore::new(snaps.path().to_path_buf());
            let dir = root_for(backend).await;

            let (root, detected) = RootBackend::detect(dir.path()).await.unwrap();
            assert_eq!(detected, backend, "detect disagrees for {backend:?}");
            assert_eq!(RootBackend::for_path(dir.path()).await, backend);

            let before = backend.capture(&store, &root).await.unwrap();
            assert_eq!(
                RootBackend::of(&before),
                backend,
                "a capture must mint an id its own backend can read"
            );
            assert!(backend.snapshot_exists(&store, &root, &before).await);

            std::fs::write(root.join("a.md"), "two\n").unwrap();
            let after = backend.capture(&store, &root).await.unwrap();
            assert_ne!(after, before, "an edited file changes the snapshot");
            assert_eq!(
                backend
                    .changed_paths(&store, &root, &before, &after)
                    .await
                    .unwrap(),
                vec![("a.md".to_string(), ChangeKind::Modified)]
            );
            assert_eq!(
                backend
                    .blob_or_empty(&store, &root, &before, "a.md", true)
                    .await
                    .unwrap(),
                Some("one\n".to_string())
            );

            backend
                .keep(&store, &root, "sess", &[before, after])
                .await
                .unwrap_or_else(|e| panic!("{backend:?} cannot claim its snapshots: {e}"));
            backend
                .ignored(&root)
                .await
                .unwrap_or_else(|e| panic!("{backend:?} cannot list what it ignores: {e}"));
        }
    }

    /// A root that is not there is neither backend's: `open` has to be able to
    /// tell "outside git" from "not a directory".
    #[tokio::test]
    async fn a_root_that_is_not_there_cannot_be_detected() {
        let dir = TempDir::new().unwrap();
        assert!(RootBackend::detect(&dir.path().join("gone")).await.is_err());
    }
}
