//! Snapshots of a review root that is not in a git repository.
//!
//! The git backend records a tree SHA per capture and lets `git` answer every
//! later question about it. A kiln outside a repository has no such store, so
//! this module is the second half of the same contract: capture a root, get an
//! id back, and read the content of any path at that id afterwards.
//!
//! A snapshot is a **manifest** — one line per file, `path` and the blake3 of
//! its bytes — and its id is the blake3 of the manifest. Content is stored once
//! per hash under `blobs/`, so two snapshots that share a file share its blob.
//! Two captures of an unchanged root therefore produce the same id, which is
//! what `compose_root` needs to answer "nothing changed" without reading a
//! single blob.
//!
//! Hashing every file on every capture would be the obvious spelling and the
//! wrong one: a capture runs on both sides of every bracketed tool call. A
//! stat key — size, mtime and inode — stands in for the content, exactly as
//! git's index does, so an unchanged file is stat'd and never read. The one
//! case a stat key cannot decide is git's "racy" one: a file written in the
//! same second as the capture that recorded it may change again inside that
//! second with no visible stat change. Those entries are simply not cached,
//! so the next capture reads them.

use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crucible_core::session::SnapshotId;
use crucible_core::EXCLUDED_DIRS;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::debug;
use walkdir::WalkDir;

use super::error::{ReviewError, ReviewResult};
use super::git::ChangeKind;

/// What one snapshot says the root held: relative path → blake3 of the bytes.
///
/// A `BTreeMap` rather than a `HashMap` because the id is derived from the
/// entries in order, and `changed_paths` walks two manifests in lockstep.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Manifest {
    pub(super) files: BTreeMap<String, String>,
}

impl Manifest {
    /// The id of this manifest: the blake3 of its `path\0hash\n` lines.
    ///
    /// Derived rather than assigned, so an unchanged root captures to the same
    /// id on every machine and after every restart.
    pub(super) fn id(&self) -> SnapshotId {
        let mut hasher = blake3::Hasher::new();
        for (path, hash) in &self.files {
            hasher.update(path.as_bytes());
            hasher.update(b"\0");
            hasher.update(hash.as_bytes());
            hasher.update(b"\n");
        }
        SnapshotId::plain(hasher.finalize().to_hex().to_string())
    }
}

/// A content-addressed snapshot store under one directory.
///
/// The state sits behind an `Arc` because a capture runs on a blocking thread
/// and the stat cache has to outlive the call that started it.
pub(super) struct PlainStore {
    inner: Arc<Inner>,
}

struct Inner {
    root: PathBuf,
    /// What each absolute path looked like the last time this store hashed it.
    stat_cache: DashMap<PathBuf, StatKey>,
    hashed: AtomicU64,
}

/// The evidence a capture accepts instead of reading a file.
///
/// Size, mtime and inode together, because no one of them is enough: an editor
/// that writes in place keeps the inode and the size, and one that writes a
/// temporary file and renames it changes the inode while a restored mtime hides
/// the write.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StatKey {
    size: u64,
    mtime: SystemTime,
    inode: u64,
    hash: String,
}

impl PlainStore {
    pub(super) fn new(root: PathBuf) -> Self {
        Self {
            inner: Arc::new(Inner {
                root,
                stat_cache: DashMap::new(),
                hashed: AtomicU64::new(0),
            }),
        }
    }

    /// Snapshot every file under `root`, storing what is not stored already.
    ///
    /// Blocking work — a directory walk and, for what changed, a read and a
    /// hash — so it runs off the runtime thread.
    pub(super) async fn capture(&self, root: &Path) -> ReviewResult<SnapshotId> {
        let inner = Arc::clone(&self.inner);
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || inner.capture(&root))
            .await
            .map_err(|e| ReviewError::Io(std::io::Error::other(e)))?
    }

    /// How many files this store has read and hashed since it was built.
    ///
    /// The one number that says whether the stat cache is working: a capture of
    /// an unchanged root must not move it. Only the tests read the running
    /// total; an operator reads the per-capture figure off the `debug` line
    /// [`Inner::capture`] emits.
    #[cfg(test)]
    pub(super) fn files_hashed(&self) -> u64 {
        self.inner.hashed.load(Ordering::Relaxed)
    }

    /// The manifest `id` names.
    pub(super) async fn manifest(&self, id: &SnapshotId) -> ReviewResult<Manifest> {
        let path = self.inner.snapshot_path(self.inner.snapshot_hash(id)?);
        let bytes = tokio::fs::read(&path).await?;
        serde_json::from_slice(&bytes).map_err(|e| {
            ReviewError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: {e}", path.display()),
            ))
        })
    }

    /// The content stored under `hash`.
    ///
    /// `Ok(None)` means the bytes are not UTF-8 — a binary file, which has no
    /// line hunks to review. The same answer [`super::git::blob`] gives, so
    /// composition treats both backends alike.
    pub(super) async fn blob(&self, hash: &str) -> ReviewResult<Option<String>> {
        let bytes = tokio::fs::read(self.inner.blob_path(hash)).await?;
        Ok(String::from_utf8(bytes).ok())
    }

    /// The content of `path` inside the snapshot `snap` names.
    ///
    /// `Ok(None)` means the bytes are not UTF-8, as [`Self::blob`] answers. A
    /// path the snapshot does not hold is an error, the way `git cat-file`
    /// errors on a path that is not in the tree: the caller asked for one side
    /// of a change the manifest says exists, so an empty answer would show as
    /// a deletion nobody made.
    pub(super) async fn blob_at(
        &self,
        snap: &SnapshotId,
        path: &str,
    ) -> ReviewResult<Option<String>> {
        let manifest = self.manifest(snap).await?;
        let Some(hash) = manifest.files.get(path) else {
            return Err(ReviewError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{path} is not in snapshot {snap}"),
            )));
        };
        self.blob(hash).await
    }

    /// Paths differing between two snapshots, with the kind of change.
    ///
    /// Renames are not detected, for the reason [`super::git::changed_paths`]
    /// records: a rename reviews better as a delete plus an add.
    pub(super) async fn changed_paths(
        &self,
        before: &SnapshotId,
        after: &SnapshotId,
    ) -> ReviewResult<Vec<(String, ChangeKind)>> {
        let (before, after) = (self.manifest(before).await?, self.manifest(after).await?);
        let mut changes = Vec::new();
        for (path, hash) in &before.files {
            match after.files.get(path) {
                Some(other) if other == hash => {}
                Some(_) => changes.push((path.clone(), ChangeKind::Modified)),
                None => changes.push((path.clone(), ChangeKind::Deleted)),
            }
        }
        for path in after.files.keys() {
            if !before.files.contains_key(path) {
                changes.push((path.clone(), ChangeKind::Added));
            }
        }
        changes.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(changes)
    }

    /// Whether this store still holds the snapshot `id` names.
    ///
    /// A git tree answers `false` rather than an error, the way
    /// [`super::git::tree_exists`] answers `false` for a plain id: the question
    /// is whether *this* store holds it, and it does not.
    pub(super) async fn exists(&self, id: &SnapshotId) -> bool {
        let Ok(hash) = self.inner.snapshot_hash(id) else {
            return false;
        };
        tokio::fs::metadata(self.inner.snapshot_path(hash))
            .await
            .is_ok()
    }
}

impl Inner {
    /// The plain-store hash inside `id`, or an error naming the mismatch.
    ///
    /// The mirror of [`super::git::tree_sha`], and the one door between a
    /// [`SnapshotId`] and this store's file names. A git tree SHA used as a
    /// file name here would name nothing, and the caller reached the wrong
    /// seam to ask.
    fn snapshot_hash<'a>(&self, id: &'a SnapshotId) -> ReviewResult<&'a str> {
        match id {
            SnapshotId::Plain(hash) => Ok(hash),
            SnapshotId::Git(_) => Err(ReviewError::WrongBackend {
                root: self.root.clone(),
                id: id.clone(),
            }),
        }
    }

    fn snapshot_path(&self, hash: &str) -> PathBuf {
        self.root.join("snapshots").join(format!("{hash}.json"))
    }

    fn blob_path(&self, hash: &str) -> PathBuf {
        self.root.join("blobs").join(hash)
    }

    fn capture(&self, root: &Path) -> ReviewResult<SnapshotId> {
        // Read before the walk, so a file written while the walk is running
        // counts as racy too.
        let captured_at = SystemTime::now();
        let hashed_before = self.hashed.load(Ordering::Relaxed);
        let mut files = BTreeMap::new();
        for entry in walk(root) {
            let entry = entry.map_err(walkdir_io)?;
            if !entry.file_type().is_file() {
                continue;
            }
            let Ok(relative) = entry.path().strip_prefix(root) else {
                continue;
            };
            let Some(relative) = relative.to_str() else {
                // A path this daemon cannot spell is a path the review surface
                // cannot show or revert.
                debug!(path = %entry.path().display(), "skipping a non-UTF-8 path");
                continue;
            };
            let hash = self.hash_of(entry.path(), captured_at)?;
            files.insert(relative.to_string(), hash);
        }

        let manifest = Manifest { files };
        // What this capture cost, which is the one figure that says whether the
        // stat cache is working on this filesystem.
        debug!(
            root = %root.display(),
            files = manifest.files.len(),
            read = self.hashed.load(Ordering::Relaxed) - hashed_before,
            "captured a plain review root"
        );
        let id = manifest.id();
        self.write_atomically(
            &self.snapshot_path(self.snapshot_hash(&id)?),
            &serde_json::to_vec(&manifest).map_err(|e| {
                ReviewError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?,
        )?;
        Ok(id)
    }

    /// The hash of one file, from the stat cache when the cache can answer.
    ///
    /// A cache hit still checks that the blob is on disk: the sweeper removes
    /// blobs no live journal names, and a manifest naming a blob that is gone
    /// is a snapshot nothing can read back.
    ///
    /// A file whose mtime falls in the second this capture started, or later,
    /// is hashed and then *not* cached — git's "racy" rule. Its stat key cannot
    /// distinguish "written just before the capture" from "written again just
    /// after it", so keeping the key would let a second edit of the same size
    /// inside that second disappear from every later snapshot.
    fn hash_of(&self, path: &Path, captured_at: SystemTime) -> ReviewResult<String> {
        let meta = std::fs::metadata(path)?;
        let key = |hash: String| StatKey {
            size: meta.len(),
            mtime: meta.modified().unwrap_or(UNIX_EPOCH),
            inode: meta.ino(),
            hash,
        };
        if let Some(cached) = self.stat_cache.get(path) {
            if *cached == key(cached.hash.clone()) && self.blob_path(&cached.hash).exists() {
                return Ok(cached.hash.clone());
            }
        }

        let hash = self.store(path)?;
        let entry = key(hash.clone());
        if seconds(entry.mtime) < seconds(captured_at) {
            self.stat_cache.insert(path.to_path_buf(), entry);
        } else {
            self.stat_cache.remove(path);
        }
        Ok(hash)
    }

    /// Read one file, hash it and store its bytes, answering the hash.
    fn store(&self, path: &Path) -> ReviewResult<String> {
        let bytes = std::fs::read(path)?;
        self.hashed.fetch_add(1, Ordering::Relaxed);
        let hash = blake3::hash(&bytes).to_hex().to_string();
        let blob = self.blob_path(&hash);
        // Content-addressed: a blob already on disk holds these exact bytes.
        if !blob.exists() {
            self.write_atomically(&blob, &bytes)?;
        }
        Ok(hash)
    }

    /// Write `bytes` to `path` through a temporary file in the same directory.
    ///
    /// A half-written blob would be indistinguishable from a whole one, and
    /// every later read of that snapshot would show truncated content as the
    /// agent's work.
    fn write_atomically(&self, path: &Path, bytes: &[u8]) -> ReviewResult<()> {
        let dir = path.parent().unwrap_or(&self.root);
        std::fs::create_dir_all(dir)?;
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        std::io::Write::write_all(&mut tmp, bytes)?;
        tmp.persist(path).map_err(|e| ReviewError::Io(e.error))?;
        Ok(())
    }
}

/// Every entry under `root`, minus the directories a kiln never indexes.
///
/// `follow_links(false)` is what keeps symlinks out, and it is load-bearing
/// twice over: a link out of the root would put content the review cannot
/// revert into a snapshot, and a link inside it would store the same file twice
/// under two paths. Unfollowed, a symlink is neither a file nor a directory to
/// this walk, so the caller's `is_file` check drops it.
fn walk(root: &Path) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| EXCLUDED_DIRS.contains(&name))
        })
}

/// Whole seconds since the epoch, which is the granularity a stat key can
/// trust: many filesystems store no more, and the ones that do disagree with
/// each other about the rest.
fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn walkdir_io(error: walkdir::Error) -> ReviewError {
    match error.into_io_error() {
        Some(io) => ReviewError::Io(io),
        None => ReviewError::Git("directory walk did not finish".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    struct Fixture {
        store_dir: TempDir,
        root: TempDir,
        store: PlainStore,
    }

    impl Fixture {
        fn new() -> Self {
            let store_dir = TempDir::new().unwrap();
            let root = TempDir::new().unwrap();
            let store = PlainStore::new(store_dir.path().join("review-snapshots"));
            Self {
                store_dir,
                root,
                store,
            }
        }

        /// Write a file and date it, so the first capture of it is never
        /// "racy" and the stat cache may keep it.
        fn write_aged(&self, name: &str, contents: &str) {
            let path = self.root.path().join(name);
            std::fs::write(&path, contents).unwrap();
            age(&path);
        }
    }

    /// Push a file's timestamps an hour into the past.
    fn age(path: &Path) {
        let file = std::fs::File::options().write(true).open(path).unwrap();
        let hour_ago = SystemTime::now() - std::time::Duration::from_secs(3600);
        file.set_times(
            std::fs::FileTimes::new()
                .set_accessed(hour_ago)
                .set_modified(hour_ago),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn a_plain_snapshot_changes_when_a_file_changes_and_a_stat_walk_re_hashes_nothing_unchanged(
    ) {
        let fixture = Fixture::new();
        fixture.write_aged("a.md", "one\n");
        fixture.write_aged("b.md", "two\n");

        let first = fixture.store.capture(fixture.root.path()).await.unwrap();
        let after_first = fixture.store.files_hashed();
        assert_eq!(after_first, 2, "both files are new to the store");

        let second = fixture.store.capture(fixture.root.path()).await.unwrap();
        assert_eq!(second, first, "an unchanged root captures to the same id");
        assert_eq!(
            fixture.store.files_hashed(),
            after_first,
            "a stat walk over an unchanged root reads no file"
        );

        fixture.write_aged("a.md", "one changed\n");
        let third = fixture.store.capture(fixture.root.path()).await.unwrap();
        assert_ne!(third, second, "an edited file changes the snapshot id");
        assert_eq!(
            fixture.store.files_hashed(),
            after_first + 1,
            "only the edited file is read"
        );

        std::fs::remove_file(fixture.root.path().join("b.md")).unwrap();
        let fourth = fixture.store.capture(fixture.root.path()).await.unwrap();
        assert_ne!(fourth, third, "a deleted file changes the snapshot id");
    }

    /// git's "racy" case. A file written in the same second as the capture that
    /// recorded it can change again inside that second with the same size and
    /// the same mtime, so its stat key is no evidence at all.
    #[tokio::test]
    async fn a_same_size_edit_in_the_same_second_is_still_seen() {
        let fixture = Fixture::new();
        let path = fixture.root.path().join("a.md");
        std::fs::write(&path, "one\n").unwrap();
        let stat = std::fs::metadata(&path).unwrap();

        let first = fixture.store.capture(fixture.root.path()).await.unwrap();

        std::fs::write(&path, "two\n").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_times(
            std::fs::FileTimes::new()
                .set_accessed(stat.accessed().unwrap())
                .set_modified(stat.modified().unwrap()),
        )
        .unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            stat.modified().unwrap(),
            "the edit must be invisible to a stat key for this test to mean anything"
        );

        let second = fixture.store.capture(fixture.root.path()).await.unwrap();
        assert_ne!(
            second, first,
            "the edit inside one second must still be seen"
        );
    }

    #[tokio::test]
    async fn a_snapshot_reads_back_as_the_paths_and_the_content_it_captured() {
        let fixture = Fixture::new();
        fixture.write_aged("a.md", "one\n");
        std::fs::create_dir_all(fixture.root.path().join("sub")).unwrap();
        fixture.write_aged("sub/b.md", "two\n");

        let id = fixture.store.capture(fixture.root.path()).await.unwrap();
        let manifest = fixture.store.manifest(&id).await.unwrap();
        assert_eq!(
            manifest.files.keys().collect::<Vec<_>>(),
            vec!["a.md", "sub/b.md"]
        );
        let hash = manifest.files.get("sub/b.md").unwrap();
        assert_eq!(
            fixture.store.blob(hash).await.unwrap(),
            Some("two\n".to_string())
        );
        assert!(fixture.store.exists(&id).await);
    }

    #[tokio::test]
    async fn a_binary_blob_reads_as_none_the_way_a_git_blob_does() {
        let fixture = Fixture::new();
        std::fs::write(fixture.root.path().join("a.bin"), [0xff, 0xfe, 0x00]).unwrap();

        let id = fixture.store.capture(fixture.root.path()).await.unwrap();
        let manifest = fixture.store.manifest(&id).await.unwrap();
        let hash = manifest.files.get("a.bin").unwrap();
        assert_eq!(fixture.store.blob(hash).await.unwrap(), None);
    }

    #[tokio::test]
    async fn the_walk_skips_excluded_directories_and_symlinks() {
        let fixture = Fixture::new();
        fixture.write_aged("a.md", "one\n");
        for excluded in EXCLUDED_DIRS {
            std::fs::create_dir_all(fixture.root.path().join(excluded)).unwrap();
            std::fs::write(fixture.root.path().join(excluded).join("x.md"), "no\n").unwrap();
        }
        std::os::unix::fs::symlink(
            fixture.root.path().join("a.md"),
            fixture.root.path().join("link.md"),
        )
        .unwrap();

        let id = fixture.store.capture(fixture.root.path()).await.unwrap();
        let manifest = fixture.store.manifest(&id).await.unwrap();
        assert_eq!(manifest.files.keys().collect::<Vec<_>>(), vec!["a.md"]);
    }

    #[tokio::test]
    async fn changed_paths_names_an_add_a_modify_and_a_delete() {
        let fixture = Fixture::new();
        fixture.write_aged("kept.md", "same\n");
        fixture.write_aged("edited.md", "before\n");
        fixture.write_aged("gone.md", "bye\n");
        let before = fixture.store.capture(fixture.root.path()).await.unwrap();

        fixture.write_aged("edited.md", "after\n");
        std::fs::remove_file(fixture.root.path().join("gone.md")).unwrap();
        fixture.write_aged("added.md", "new\n");
        let after = fixture.store.capture(fixture.root.path()).await.unwrap();

        assert_eq!(
            fixture.store.changed_paths(&before, &after).await.unwrap(),
            vec![
                ("added.md".to_string(), ChangeKind::Added),
                ("edited.md".to_string(), ChangeKind::Modified),
                ("gone.md".to_string(), ChangeKind::Deleted),
            ]
        );
    }

    /// The sweeper removes blobs no live journal names. A stat key that still
    /// matched would then answer with a hash whose content is gone, and every
    /// later read of that snapshot would fail on a blob that is not there.
    #[tokio::test]
    async fn a_cache_hit_whose_blob_was_swept_reads_the_file_again() {
        let fixture = Fixture::new();
        fixture.write_aged("a.md", "one\n");
        let id = fixture.store.capture(fixture.root.path()).await.unwrap();
        let hash = fixture
            .store
            .manifest(&id)
            .await
            .unwrap()
            .files
            .remove("a.md")
            .unwrap();
        let after_first = fixture.store.files_hashed();

        std::fs::remove_file(fixture.store.inner.blob_path(&hash)).unwrap();
        let again = fixture.store.capture(fixture.root.path()).await.unwrap();

        assert_eq!(again, id, "the file did not change, so neither does the id");
        assert_eq!(
            fixture.store.files_hashed(),
            after_first + 1,
            "a blob that is gone has to be written again"
        );
        assert_eq!(
            fixture.store.blob(&hash).await.unwrap(),
            Some("one\n".to_string())
        );
    }

    /// The mirror of `a_plain_id_handed_to_git_is_an_error_not_a_panic`: this
    /// store is the other side of the same seam, and a git tree used as a file
    /// name here would name nothing.
    #[tokio::test]
    async fn a_git_id_handed_to_the_plain_store_is_an_error_not_a_panic() {
        let fixture = Fixture::new();
        let git = SnapshotId::git("0123456789abcdef0123456789abcdef01234567");

        assert!(matches!(
            fixture.store.manifest(&git).await,
            Err(ReviewError::WrongBackend { .. })
        ));
        assert!(!fixture.store.exists(&git).await);
        let _ = &fixture.store_dir;
    }
}
