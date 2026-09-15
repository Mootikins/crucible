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
//!
//! Nothing outside this daemon collects a plain snapshot — there is no `git
//! gc` under a kiln that is not a repository — so the store collects its own.
//! Each session writes a **keep** per root it tracks: the snapshots that
//! root's ledger still names. [`PlainStore::sweep`] removes every snapshot and
//! blob no keep claims, and every keep whose session directory is gone. That
//! is the git side's rule one level down — a claim is released by its session
//! going away, never by age.

use std::collections::{BTreeMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crucible_core::session::SnapshotId;
use crucible_core::EXCLUDED_DIRS;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
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

/// One session's claim on one root's snapshots.
///
/// The plain analogue of a git keep ref, and total for the same reason
/// [`super::git::update_keep`] records: the claim is rewritten from the
/// ledger's full list on every call, so a ledger that lost an interval also
/// loses its claim on that interval's snapshots.
///
/// One file per *root* rather than one per session, because a session can
/// track several roots and each is claimed by its own call. A single file
/// would make two of those calls a read-modify-write race whose loser silently
/// unclaims a live root's snapshots.
///
/// `root` is written for the operator reading the directory; nothing reads it
/// back. The file name is the hash of the root path, which is the part that
/// has to be unique and spellable.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Keep {
    root: String,
    snapshots: Vec<SnapshotId>,
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

    pub(super) async fn contains_path(&self, snap: &SnapshotId, path: &str) -> ReviewResult<bool> {
        Ok(self.manifest(snap).await?.files.contains_key(path))
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

    /// Claim `snapshots` for `session_id` under `root`, against [`Self::sweep`].
    ///
    /// Idempotent and total — see [`Keep`]. A claim on an id from the other
    /// backend is refused rather than written: the sweep could not match it to
    /// a file here, so the claim would silently protect nothing.
    pub(super) async fn keep(
        &self,
        root: &Path,
        session_id: &str,
        snapshots: &[SnapshotId],
    ) -> ReviewResult<()> {
        for id in snapshots {
            self.inner.snapshot_hash(id)?;
        }
        let keep = Keep {
            root: root.display().to_string(),
            snapshots: snapshots.to_vec(),
        };
        let bytes = serde_json::to_vec(&keep).map_err(|e| {
            ReviewError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        let path = self.inner.keep_path(session_id, root);
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.write_atomically(&path, &bytes))
            .await
            .map_err(|e| ReviewError::Io(std::io::Error::other(e)))?
    }

    /// Release every claim `session_id` holds, leaving its snapshots to the
    /// next sweep.
    ///
    /// A session with no claims is not an error: the delete path runs for every
    /// session, and most never tracked a plain root at all.
    pub(super) async fn drop_keep(&self, session_id: &str) -> ReviewResult<()> {
        match tokio::fs::remove_dir_all(self.inner.keeps_dir().join(session_id)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(ReviewError::Io(e)),
        }
    }

    /// Remove every snapshot and blob no live session claims, and every claim
    /// whose session directory is gone. Answers how many files it removed.
    ///
    /// The backstop [`super::sweep_review_refs`] is for git, over the same
    /// rule: a session's directory going away is the only thing that makes its
    /// snapshots garbage. Age never does — a month-old review is still a
    /// review.
    ///
    /// **Fails closed.** Every step that cannot prove what is claimed — an
    /// unreadable sessions root, a claim file that will not parse, a manifest
    /// the pass must account for and cannot read — abandons the whole pass
    /// with nothing removed. The cost of waiting for the next tick is disk; the cost of
    /// guessing is a review that can no longer be computed.
    ///
    /// The one window it shares with `git gc` — a capture writes its snapshot
    /// before the ledger records the interval that claims it — is closed the
    /// way git closes it, with a grace period: nothing younger than
    /// [`PRUNE_GRACE`] is collected, however unclaimed it looks. There is no
    /// recovery without one. A later capture reproduces a current state, never
    /// an interval's `before_tree`, which is a past state of the disk, so a
    /// snapshot taken mid-bracket leaves that session's hunks uncomputable
    /// until a rebase.
    ///
    /// The grace period covers the whole snapshot, contents included: a young
    /// unclaimed manifest is read for its blobs like a claimed one. Blobs are
    /// content-addressed, so a capture that finds one on disk writes nothing
    /// and the file keeps the age of the capture that first stored it — old
    /// enough to take, while a snapshot the pass has just spared names it.
    pub(super) async fn sweep(&self, sessions_root: &Path) -> usize {
        let Some((live_snapshots, dead_keeps)) = self.claims(sessions_root).await else {
            return 0;
        };
        for path in dead_keeps {
            if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                debug!(path = %path.display(), error = %e, "stale review claim not removed");
            }
        }

        // Classify before removing anything: a live manifest that will not
        // read leaves its blobs unaccounted for, and a blob removed on that
        // reading is content no snapshot can produce again.
        let mut dead_snapshots = Vec::new();
        let mut live_blobs = HashSet::new();
        let mut entries = match tokio::fs::read_dir(self.inner.root.join("snapshots")).await {
            Ok(entries) => entries,
            // No snapshots at all: a store nothing has captured into.
            Err(_) => return 0,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(hash) = name.strip_suffix(".json").filter(|h| is_content_hash(h)) else {
                continue;
            };
            // A snapshot the grace period still protects is read like a live
            // one: keeping it while counting its blobs as garbage would leave
            // it naming content nothing can produce again.
            if !live_snapshots.contains(hash) && old_enough(&entry).await {
                dead_snapshots.push(entry.path());
                continue;
            }
            match self.manifest(&SnapshotId::plain(hash)).await {
                Ok(manifest) => live_blobs.extend(manifest.files.into_values()),
                Err(e) => {
                    warn!(
                        snapshot = hash,
                        error = %e,
                        "a review snapshot the pass must account for will not read; sweeping nothing this pass"
                    );
                    return 0;
                }
            }
        }

        let mut dead_blobs = Vec::new();
        if let Ok(mut entries) = tokio::fs::read_dir(self.inner.root.join("blobs")).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_content_hash(&name) && !live_blobs.contains(&name) && old_enough(&entry).await
                {
                    dead_blobs.push(entry.path());
                }
            }
        }

        let mut removed = 0;
        for path in dead_snapshots.into_iter().chain(dead_blobs) {
            match tokio::fs::remove_file(&path).await {
                Ok(()) => removed += 1,
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "unclaimed review file not removed")
                }
            }
        }
        removed
    }

    /// Every snapshot hash a live session claims, and the claim directories
    /// whose session is gone.
    ///
    /// `None` means the question could not be answered, and the caller must
    /// remove nothing — see [`Self::sweep`].
    async fn claims(&self, sessions_root: &Path) -> Option<(HashSet<String>, Vec<PathBuf>)> {
        if !tokio::fs::metadata(sessions_root)
            .await
            .is_ok_and(|m| m.is_dir())
        {
            warn!(
                sessions_root = %sessions_root.display(),
                "no sessions root to read; sweeping no review snapshots"
            );
            return None;
        }
        // Nothing has claimed anything yet, which is also true of a store
        // nothing has captured into.
        let mut sessions = tokio::fs::read_dir(self.inner.keeps_dir()).await.ok()?;

        let mut live = HashSet::new();
        let mut dead = Vec::new();
        while let Ok(Some(session)) = sessions.next_entry().await {
            let session_id = session.file_name().to_string_lossy().into_owned();
            if !tokio::fs::try_exists(sessions_root.join(&session_id))
                .await
                .unwrap_or(true)
            {
                dead.push(session.path());
                continue;
            }
            let mut claims = tokio::fs::read_dir(session.path()).await.ok()?;
            while let Ok(Some(claim)) = claims.next_entry().await {
                // A claim this store wrote, rather than a temporary a crash
                // left mid-write: an unparseable file here would stall every
                // later sweep, not just this one.
                if claim.path().extension().is_none_or(|e| e != "json") {
                    continue;
                }
                let bytes = tokio::fs::read(claim.path()).await.ok()?;
                let keep: Keep = serde_json::from_slice(&bytes)
                    .map_err(|e| {
                        warn!(
                            path = %claim.path().display(),
                            error = %e,
                            "a review claim will not parse; sweeping nothing this pass"
                        );
                    })
                    .ok()?;
                for id in &keep.snapshots {
                    if let Ok(hash) = self.inner.snapshot_hash(id) {
                        live.insert(hash.to_string());
                    }
                }
            }
        }
        Some((live, dead))
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

    fn keeps_dir(&self) -> PathBuf {
        self.root.join("keeps")
    }

    /// Where one session's claim on one root is written.
    ///
    /// The root is hashed rather than spelled: a path holds separators, and the
    /// file name only has to be unique per root and readable back by the
    /// sweep, which never asks which root a claim was for.
    fn keep_path(&self, session_id: &str, root: &Path) -> PathBuf {
        let root = blake3::hash(root.as_os_str().as_encoded_bytes()).to_hex();
        self.keeps_dir()
            .join(session_id)
            .join(format!("{root}.json"))
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
        if blob.exists() {
            // Touch it, so the sweep's grace period covers a blob this capture
            // re-references. A revert can make an old blob current again, and
            // an untouched one would carry the mtime of the capture that first
            // stored it — old enough to collect while a bracket names it.
            touch(&blob);
        } else {
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

/// How long a file is too young to collect, however unclaimed it looks.
///
/// A capture writes its snapshot before the ledger records the interval that
/// claims it, and a bracket stays open for as long as the tool call runs. A
/// sweep that landed in that window would take the call's `before_tree`, which
/// is a past state no later capture reproduces, so the session's hunks would
/// never list again. Git answers the same problem the same way: `git gc`
/// prunes only objects older than `gc.pruneExpire`, two weeks by default.
const PRUNE_GRACE: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Whether this file is old enough to collect.
///
/// An unreadable or future mtime answers `false`: the question is whether the
/// file is provably old, and clock skew must protect rather than collect.
async fn old_enough(entry: &tokio::fs::DirEntry) -> bool {
    let Ok(meta) = entry.metadata().await else {
        return false;
    };
    meta.modified()
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .is_some_and(|age| age >= PRUNE_GRACE)
}

/// Set a file's modification time to now, best effort.
///
/// A failure here only narrows the sweep's grace window for one blob, and the
/// sweep still refuses to take anything a live journal names.
fn touch(path: &Path) {
    let now = std::fs::FileTimes::new().set_modified(SystemTime::now());
    if let Ok(file) = std::fs::File::options().write(true).open(path) {
        let _ = file.set_times(now);
    }
}

/// Whether a file name is one this store wrote, rather than the temporary a
/// capture is writing right now.
///
/// The sweep walks directories a live capture also writes into, and every
/// write here lands as a `NamedTempFile` beside its target first. A temporary
/// swept mid-write fails the capture that was making it.
fn is_content_hash(name: &str) -> bool {
    name.len() == blake3::OUT_LEN * 2
        && name.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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

    /// Blobs are content-addressed, so a capture that finds one already on
    /// disk writes nothing. Its mtime would then be the age of the capture that
    /// first stored it — old enough for the sweep to collect while a bracket in
    /// flight names it. A revert is the ordinary way an old blob becomes
    /// current again.
    #[tokio::test]
    async fn a_blob_a_capture_reuses_is_young_again() {
        let fixture = Fixture::new();
        fixture.write_aged("a.md", "one\n");
        let first = fixture.store.capture(fixture.root.path()).await.unwrap();
        let blob = fixture
            .store
            .inner
            .blob_path(&fixture.store.manifest(&first).await.unwrap().files["a.md"]);

        fixture.write_aged("a.md", "two\n");
        fixture.store.capture(fixture.root.path()).await.unwrap();
        age(&blob);
        assert!(!within_grace(&blob), "the blob must start out collectable");

        // The revert: the same bytes again, so the store finds the blob there.
        fixture.write_aged("a.md", "one\n");
        let third = fixture.store.capture(fixture.root.path()).await.unwrap();
        assert_eq!(
            third, first,
            "the same content must reach the same snapshot"
        );
        assert!(
            within_grace(&blob),
            "a reused blob kept the age of the capture that first stored it, \
             so the sweep may take it while a bracket names it"
        );
    }

    /// Whether the sweep's grace period still protects this file —
    /// [`old_enough`] over a path, inverted.
    fn within_grace(path: &Path) -> bool {
        let modified = std::fs::metadata(path).unwrap().modified().unwrap();
        SystemTime::now()
            .duration_since(modified)
            .is_ok_and(|age| age < PRUNE_GRACE)
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
