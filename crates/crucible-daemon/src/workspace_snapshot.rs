//! Workspace snapshot/restore for turn-level undo.
//!
//! Captures workspace state before each agent turn so that an `undo`
//! operation can revert any file edits made by tools during that turn.
//!
//! Two backends:
//!
//! * **Git mode** — when the workspace is a git repo. Stages the worktree
//!   into a *throwaway* index ([`capture_tree`]) and wraps the resulting
//!   tree in an orphan commit. This is deliberately not `git stash
//!   create`, which cannot capture untracked files — a tool that wrote a
//!   new file would not be undoable. Restore replays the tree via
//!   `git restore --source <sha> -- .`, recreating both index and
//!   worktree — scoped to the workspace, because capture is.
//! * **Journal mode** — when the workspace is not a git repo. Walks the
//!   workspace and stores every file's bytes in memory. Capped at
//!   `DEFAULT_JOURNAL_CAP` (5 MiB) to bound memory; over the cap we
//!   record a "skipped" snapshot and restore is a no-op.
//!
//! `create` never propagates failures upward — a failure at snapshot
//! time degrades to `skipped: true` (with a `warn!` log) so it can never
//! abort the agent turn. Callers that need to know a capture failed —
//! the review ledger, which must not silently fall back to walking the
//! whole workspace per tool call — use [`capture_tree`] directly.

use dashmap::DashMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;
use tracing::{debug, warn};

const DEFAULT_JOURNAL_CAP: usize = 5 * 1024 * 1024;

/// Per-session, per-turn snapshot store.
///
/// Keyed by `(session_id, node_id)` where `node_id` is the
/// `ConversationTree` node that was `current` at the moment the
/// snapshot was taken — i.e. the cursor position the tree returns to
/// after `undo_turns(n)`. On undo, the daemon reads the new cursor and
/// looks up the snapshot under that key.
///
/// Cleanup: snapshots are dropped opportunistically on undo (the
/// snapshot for the node we restored is consumed) and on session end.
/// We accept that abandoned tree branches leave dangling entries — this
/// is bounded by the number of turns in a session lifetime.
#[derive(Default)]
pub struct SnapshotMap {
    inner: DashMap<(String, u32), Arc<WorkspaceSnapshot>>,
}

impl SnapshotMap {
    pub fn insert(&self, session_id: String, node_id: u32, snap: WorkspaceSnapshot) {
        self.inner.insert((session_id, node_id), Arc::new(snap));
    }

    pub fn get(&self, session_id: &str, node_id: u32) -> Option<Arc<WorkspaceSnapshot>> {
        self.inner
            .get(&(session_id.to_string(), node_id))
            .map(|r| Arc::clone(r.value()))
    }

    pub fn remove(&self, session_id: &str, node_id: u32) -> Option<Arc<WorkspaceSnapshot>> {
        self.inner
            .remove(&(session_id.to_string(), node_id))
            .map(|(_, v)| v)
    }

    /// Drop every snapshot for a session, releasing their keep refs.
    ///
    /// Callers are sync (`AgentManager::cleanup_session`), so the git work
    /// is spawned. A ref that outlives its snapshot costs one unreachable
    /// tree until the sweeper catches it — teardown must not block on git.
    pub fn clear_session(&self, session_id: &str) {
        let mut released = Vec::new();
        self.inner.retain(|(s, node), snap| {
            if s == session_id {
                released.push((*node, Arc::clone(snap)));
                false
            } else {
                true
            }
        });
        if released.is_empty() {
            return;
        }
        let session = session_id.to_string();
        tokio::spawn(async move {
            for (node, snap) in released {
                snap.release(&session, node).await;
            }
        });
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether this session has no snapshots left. The keys are
    /// `(session_id, node_id)`, so `is_empty` cannot answer this — and the
    /// post-teardown invariant is per session, not global.
    pub fn is_empty_for(&self, session_id: &str) -> bool {
        !self.inner.iter().any(|entry| entry.key().0 == session_id)
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Captured workspace state for a single turn.
///
/// One of three shapes:
///
/// * `tree_id = Some(sha)`   — captured tree. Restore replays it.
/// * `journal = Some(map)`   — in-memory file → bytes map.
/// * `skipped = true`        — snapshot was bypassed (e.g. cap exceeded
///   or git op failed unrecoverably). `restore` is a no-op.
///
/// The default value is the third (no-op) form, used when there's
/// nothing to snapshot (empty workspace, git unavailable, etc.).
#[derive(Debug, Clone, Default)]
pub struct WorkspaceSnapshot {
    /// The captured tree. This is what `restore` replays and what the
    /// session's keep ref pins against `git gc`.
    pub tree_id: Option<String>,
    /// Repo the capture came from, so cleanup can find the keep ref
    /// again. `None` outside git mode.
    pub root: Option<PathBuf>,
    /// Path → original bytes map, populated for non-git workspaces.
    /// Paths are relative to the workspace root.
    pub journal: Option<HashMap<PathBuf, Vec<u8>>>,
    /// True when snapshotting was deliberately skipped (cap exceeded,
    /// git op failed, etc.). Restore is a no-op in that case.
    pub skipped: bool,
}

/// Ref pinning one turn's captured tree.
///
/// **Tree-valued, deliberately.** A commit-valued ref under `refs/` shows
/// up in `git log --all`, putting crucible's bookkeeping in the user's
/// history; a tree-valued one does not, because `--all` walks commits.
pub(crate) fn keep_ref(session_id: &str, node_id: u32) -> String {
    format!("refs/crucible/snapshots/{session_id}/{node_id}")
}

impl WorkspaceSnapshot {
    /// Capture workspace state, picking git or journal mode automatically.
    pub async fn create(workspace: &Path, session_id: &str, node_id: u32) -> Self {
        Self::create_with_cap(workspace, session_id, node_id, DEFAULT_JOURNAL_CAP).await
    }

    /// As [`Self::create`] but with an explicit journal byte cap. Mainly
    /// for tests; production code uses the default.
    pub async fn create_with_cap(
        workspace: &Path,
        session_id: &str,
        node_id: u32,
        journal_cap: usize,
    ) -> Self {
        if is_git_repo(workspace).await {
            match capture_tree(workspace).await {
                Ok(tree) => {
                    // Without this the captured tree is unreachable from any
                    // ref, so `git gc --prune=now` in the agent's own repo —
                    // or any gc once the object passes the two-week default
                    // expiry — deletes it and undo silently restores nothing.
                    if let Err(e) =
                        update_keep(workspace, &keep_ref(session_id, node_id), &tree).await
                    {
                        warn!(
                            workspace = %workspace.display(),
                            error = %e,
                            "snapshot keep ref failed; the capture is gc-collectable"
                        );
                    }
                    Self {
                        tree_id: Some(tree),
                        root: Some(workspace.to_path_buf()),
                        journal: None,
                        skipped: false,
                    }
                }
                Err(e) => {
                    warn!(
                        workspace = %workspace.display(),
                        error = %e,
                        "workspace tree capture failed; falling back to journal"
                    );
                    build_journal(workspace, journal_cap)
                }
            }
        } else {
            build_journal(workspace, journal_cap)
        }
    }

    /// Release the keep ref, making the captured tree collectable again.
    /// Failure is logged, not propagated: a stale ref costs one unreachable
    /// tree, while a failed cleanup must never block session teardown.
    pub(crate) async fn release(&self, session_id: &str, node_id: u32) {
        let Some(root) = self.root.as_deref() else {
            return;
        };
        if let Err(e) = drop_keep(root, &keep_ref(session_id, node_id)).await {
            debug!(error = %e, "dropping snapshot keep ref failed");
        }
    }

    /// Restore the workspace to the captured state.
    ///
    /// Errors propagate from filesystem / git failures; callers should
    /// log and continue rather than abort an undo flow on restore failure.
    pub async fn restore(&self, workspace: &Path) -> std::io::Result<()> {
        if self.skipped {
            return Ok(());
        }
        if let Some(sha) = self.tree_id.as_ref() {
            // Scoped to the workspace, NOT `:(top)`.
            //
            // Capture and restore have to agree on what they cover, and
            // capture cannot cover more than the workspace: `capture_tree`
            // runs `add -A .` from here, so paths outside the workspace keep
            // whatever the *seeded index* held. Restoring `:(top)` therefore
            // rewrote the whole worktree from a tree that only knew about the
            // workspace — reverting a user's uncommitted edits in sibling
            // directories to their committed content, silently, on an undo
            // they asked for about the agent's work.
            //
            // `.` resolves against `current_dir` below, so this is the whole
            // repo when the workspace *is* the repo root, and exactly the
            // agent's subtree when it is not.
            let out = Command::new("git")
                .args(["restore", "--source", sha, "--worktree", "--staged", "."])
                .current_dir(workspace)
                .output()
                .await?;
            if !out.status.success() {
                // Older git versions (< 2.23) lack `git restore`. Fall
                // back to checkout; this restores tracked files but
                // won't remove files the turn newly created. That's a
                // graceful degradation rather than a hard failure.
                let out2 = Command::new("git")
                    .args(["checkout", sha, "--", "."])
                    .current_dir(workspace)
                    .output()
                    .await?;
                if !out2.status.success() {
                    return Err(std::io::Error::other(format!(
                        "git restore/checkout failed: {}",
                        String::from_utf8_lossy(&out2.stderr)
                    )));
                }
            }
            Ok(())
        } else if let Some(j) = &self.journal {
            for (rel_path, bytes) in j {
                let abs = workspace.join(rel_path);
                if let Some(parent) = abs.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&abs, bytes)?;
            }
            Ok(())
        } else {
            // Default-shaped snapshot (no tree_id, no journal, not
            // skipped): there was nothing to snapshot, so nothing to do.
            Ok(())
        }
    }
}

/// True when `p` is inside a git worktree.
///
/// `rev-parse` walks *up*, so a plain directory nested inside a repo
/// answers true and takes the git path. That is intentional: the tree it
/// produces still describes that directory's contents.
pub async fn is_git_repo(p: &Path) -> bool {
    let out = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(p)
        .output()
        .await;
    matches!(out, Ok(o) if o.status.success())
}

/// Stage everything under `p` into a *throwaway* index and return the
/// resulting tree SHA. The worktree and the repository's real index are
/// both left untouched.
///
/// The real index must not be involved: this runs once per turn for undo
/// and once per tool call for the review ledger, and `git add -A` against
/// the live index races anyone running git in a terminal beside us. Worse,
/// mid-merge it collapses every unmerged stage — a snapshot would silently
/// mark the user's conflicts resolved.
///
/// The temp index is *seeded* from the real one. That is not just a warm
/// cache: `git write-tree` always emits the full index tree, so an empty
/// seed under a workspace nested inside a larger repo would write a tree
/// missing every sibling directory, and restoring it would delete them.
pub async fn capture_tree(p: &Path) -> std::io::Result<String> {
    let temp = tempfile::TempDir::new()?;
    let index_path = temp.path().join("index");

    // Absolute: git resolves a relative GIT_INDEX_FILE against the worktree
    // top level, not the current directory.
    let git_dir = run_git(p, &["rev-parse", "--absolute-git-dir"], None).await?;
    let real_index = Path::new(git_dir.trim()).join("index");
    if let Ok(source) = std::fs::metadata(&real_index) {
        std::fs::copy(&real_index, &index_path)?;
        // The copy's own mtime is what git compares entries against to decide
        // they are "racily clean" and must be re-hashed. Letting it default to
        // now silently disarms that check: an edit written in the same second
        // as the real index, with the same file size, keeps matching stat data
        // and never reaches `write-tree`. Same second, same size is exactly
        // what a tool call rewriting a line produces.
        if let Ok(mtime) = source.modified() {
            std::fs::File::options()
                .write(true)
                .open(&index_path)?
                .set_modified(mtime)?;
        }
    }

    let index_env = index_path.as_os_str();
    // Additions, modifications and deletions under `p`. `.gitignore` still
    // applies, so build output never lands in the tree.
    run_git(p, &["add", "-A", "."], Some(index_env)).await?;
    let tree = run_git(p, &["write-tree"], Some(index_env)).await?;
    Ok(tree.trim().to_string())
}

/// Point `name` at `tree`, pinning it against `git gc`.
async fn update_keep(root: &Path, name: &str, tree: &str) -> std::io::Result<()> {
    run_git(root, &["update-ref", name, tree], None)
        .await
        .map(|_| ())
}

/// Delete `name`, if it still exists.
async fn drop_keep(root: &Path, name: &str) -> std::io::Result<()> {
    run_git(root, &["update-ref", "-d", name], None)
        .await
        .map(|_| ())
}

/// Run git in `dir`, optionally against an alternate index, and return
/// stdout. A non-zero exit is an error carrying git's stderr.
async fn run_git(
    dir: &Path,
    args: &[&str],
    index_file: Option<&std::ffi::OsStr>,
) -> std::io::Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(dir);
    if let Some(index) = index_file {
        cmd.env("GIT_INDEX_FILE", index);
    }
    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(std::io::Error::other(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Walk `workspace` and collect every file's bytes into an in-memory
/// journal. Returns a `skipped: true` snapshot if total bytes would
/// exceed `cap`. Skips well-known noisy directories (`.git`,
/// `node_modules`, `target`).
fn build_journal(workspace: &Path, cap: usize) -> WorkspaceSnapshot {
    use walkdir::WalkDir;
    let mut journal: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    let mut total = 0usize;
    for entry in WalkDir::new(workspace).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = match path.strip_prefix(workspace) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        // Skip noisy / large dirs that aren't user content.
        if rel.components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some(".git" | "node_modules" | "target")
            )
        }) {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if total + bytes.len() > cap {
            warn!(
                workspace = %workspace.display(),
                cap_bytes = cap,
                "workspace journal cap exceeded; skipping snapshot"
            );
            return WorkspaceSnapshot {
                skipped: true,
                ..Default::default()
            };
        }
        total += bytes.len();
        journal.insert(rel, bytes);
    }
    WorkspaceSnapshot {
        journal: Some(journal),
        skipped: false,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    async fn git_status(p: &Path) -> String {
        let out = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(p)
            .output()
            .await
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    async fn git_init(p: &Path) {
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(p)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t"])
            .current_dir(p)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "t"])
            .current_dir(p)
            .output()
            .await
            .unwrap();
        // Initial commit so HEAD exists.
        fs::write(p.join(".gitkeep"), b"").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(p)
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(p)
            .output()
            .await
            .unwrap();
    }

    /// The captured tree is unreachable from any branch, so without a keep
    /// ref `git gc --prune=now` in the agent's own repo collects it and undo
    /// silently restores nothing — reporting success while doing so.
    #[tokio::test]
    async fn a_snapshot_survives_an_aggressive_prune() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        fs::write(dir.path().join("a.txt"), b"before").unwrap();

        let snap = WorkspaceSnapshot::create(dir.path(), "sess", 7).await;
        fs::write(dir.path().join("a.txt"), b"after the turn").unwrap();

        let out = Command::new("git")
            .args(["gc", "--prune=now", "--aggressive", "-q"])
            .current_dir(dir.path())
            .output()
            .await
            .unwrap();
        assert!(out.status.success(), "gc: {:?}", out);

        snap.restore(dir.path()).await.expect("restore after gc");
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "before",
            "the turn was not undone: gc collected the captured tree"
        );
    }

    /// Undo must not reach outside the workspace.
    ///
    /// When the workspace is a subdirectory, capture only refreshes paths
    /// under it (`add -A .`). Restoring `:(top)` replayed the whole worktree
    /// from that partial tree, so a user's uncommitted edit in a sibling
    /// directory was silently reverted to its committed content by an undo
    /// about the agent's work.
    #[tokio::test]
    async fn undo_leaves_uncommitted_work_outside_the_workspace_alone() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        let ws = dir.path().join("ws");
        let sibling = dir.path().join("sibling");
        fs::create_dir(&ws).unwrap();
        fs::create_dir(&sibling).unwrap();
        fs::write(ws.join("a.txt"), b"before").unwrap();
        fs::write(sibling.join("s.txt"), b"committed").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .await
            .unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(dir.path())
            .output()
            .await
            .unwrap();

        // The user is editing elsewhere in the repo while the agent works.
        fs::write(sibling.join("s.txt"), b"the user's uncommitted work").unwrap();

        let snap = WorkspaceSnapshot::create(&ws, "sess", 1).await;
        fs::write(ws.join("a.txt"), b"what the agent wrote").unwrap();
        snap.restore(&ws).await.unwrap();

        assert_eq!(
            fs::read_to_string(ws.join("a.txt")).unwrap(),
            "before",
            "the agent's edit should have been undone"
        );
        assert_eq!(
            fs::read_to_string(sibling.join("s.txt")).unwrap(),
            "the user's uncommitted work",
            "undo destroyed the user's work outside the workspace"
        );
    }

    /// Releasing must actually unpin, or every session a daemon ever ran
    /// keeps its trees alive forever.
    #[tokio::test]
    async fn releasing_a_snapshot_drops_its_keep_ref() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        fs::write(dir.path().join("a.txt"), b"before").unwrap();
        let snap = WorkspaceSnapshot::create(dir.path(), "sess", 7).await;

        let name = keep_ref("sess", 7);
        let present = |name: String| {
            let d = dir.path().to_path_buf();
            async move {
                Command::new("git")
                    .args(["rev-parse", "--verify", "--quiet", &name])
                    .current_dir(&d)
                    .output()
                    .await
                    .unwrap()
                    .status
                    .success()
            }
        };
        assert!(present(name.clone()).await, "keep ref was never written");
        snap.release("sess", 7).await;
        assert!(!present(name).await, "keep ref outlived its snapshot");
    }

    #[tokio::test]
    async fn snapshot_git_captures_uncommitted_change_and_restores() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let snap = WorkspaceSnapshot::create(dir.path(), "sess", 1).await;
        assert!(snap.tree_id.is_some(), "expected a captured tree sha");
        assert!(!snap.skipped);

        // Mutate then restore.
        fs::write(dir.path().join("a.txt"), b"world").unwrap();
        snap.restore(dir.path()).await.unwrap();
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn snapshot_non_git_uses_journal() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let snap = WorkspaceSnapshot::create(dir.path(), "sess", 1).await;
        assert!(snap.journal.is_some());
        assert!(snap.tree_id.is_none());
        assert!(!snap.skipped);

        fs::write(dir.path().join("a.txt"), b"world").unwrap();
        snap.restore(dir.path()).await.unwrap();
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn snapshot_journal_cap_skips_large_workspace() {
        let dir = tempdir().unwrap();
        // 6 MiB file — single file exceeds 5 MiB default cap.
        fs::write(dir.path().join("big.bin"), vec![0u8; 6 * 1024 * 1024]).unwrap();
        let snap = WorkspaceSnapshot::create_with_cap(dir.path(), "sess", 1, 5 * 1024 * 1024).await;
        assert!(snap.skipped);
        // Restore is a no-op; the post-mutation content survives.
        fs::write(dir.path().join("big.bin"), b"changed").unwrap();
        snap.restore(dir.path()).await.unwrap();
        assert_eq!(fs::read(dir.path().join("big.bin")).unwrap(), b"changed");
    }

    #[tokio::test]
    async fn snapshot_restore_creates_missing_files_in_journal() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let snap = WorkspaceSnapshot::create(dir.path(), "sess", 1).await;
        // Delete the file — restore should re-create it.
        fs::remove_file(dir.path().join("a.txt")).unwrap();
        snap.restore(dir.path()).await.unwrap();
        assert_eq!(fs::read(dir.path().join("a.txt")).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn snapshot_journal_skips_vcs_and_build_dirs() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git").join("HEAD"), b"junk").unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        fs::write(dir.path().join("target").join("debug.bin"), b"junk").unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();

        let snap = WorkspaceSnapshot::create(dir.path(), "sess", 1).await;
        let journal = snap.journal.expect("expected journal mode");
        assert!(journal.contains_key(&PathBuf::from("a.txt")));
        assert!(!journal
            .keys()
            .any(|k| k.starts_with(".git") || k.starts_with("target")));
    }

    /// The real index is the user's; a capture that touches it races anyone
    /// running git beside us and destroys unmerged stages mid-merge.
    #[tokio::test]
    async fn capture_leaves_the_real_index_alone() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        fs::write(dir.path().join("untracked.txt"), b"new").unwrap();
        let index_before = fs::read(dir.path().join(".git").join("index")).unwrap();
        let status_before = git_status(dir.path()).await;

        capture_tree(dir.path()).await.unwrap();

        assert_eq!(
            fs::read(dir.path().join(".git").join("index")).unwrap(),
            index_before,
            "capture wrote through to the repository index"
        );
        assert_eq!(git_status(dir.path()).await, status_before);
    }

    #[tokio::test]
    async fn capture_tree_includes_untracked_files() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        fs::write(dir.path().join("brand_new.txt"), b"hi").unwrap();

        let tree = capture_tree(dir.path()).await.unwrap();
        let listing = Command::new("git")
            .args(["ls-tree", "-r", "--name-only", &tree])
            .current_dir(dir.path())
            .output()
            .await
            .unwrap();
        let listing = String::from_utf8_lossy(&listing.stdout);
        assert!(
            listing.contains("brand_new.txt"),
            "untracked file missing from tree: {listing}"
        );
    }

    /// A no-op tool call must not produce an interval. Tree SHAs are the
    /// dedupe key, so two captures with no edit between them must match.
    #[tokio::test]
    async fn capture_tree_is_stable_across_no_op_captures() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();

        let first = capture_tree(dir.path()).await.unwrap();
        let second = capture_tree(dir.path()).await.unwrap();
        assert_eq!(first, second);

        fs::write(dir.path().join("a.txt"), b"changed").unwrap();
        assert_ne!(capture_tree(dir.path()).await.unwrap(), first);
    }

    /// Git compares cached stat data at one-second granularity, so an edit
    /// that keeps a file's size and lands in the same second as the last
    /// index write is only caught by the "racily clean" re-hash — which is
    /// keyed on the index file's own mtime. A capture taken a second later
    /// must still see it.
    #[tokio::test]
    async fn capture_tree_sees_a_same_size_edit_made_in_the_index_second() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        fs::write(dir.path().join("a.txt"), b"aaaa\n").unwrap();
        for args in [&["add", "a.txt"][..], &["commit", "-q", "-m", "a"][..]] {
            Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .await
                .unwrap();
        }

        fs::write(dir.path().join("a.txt"), b"bbbb\n").unwrap();
        // Push the capture past the second boundary the write shares with the
        // index, which is what makes the stale stat data look trustworthy.
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;

        let captured = capture_tree(dir.path()).await.unwrap();
        let head_tree = Command::new("git")
            .args(["rev-parse", "HEAD^{tree}"])
            .current_dir(dir.path())
            .output()
            .await
            .unwrap();
        let head_tree = String::from_utf8_lossy(&head_tree.stdout)
            .trim()
            .to_string();
        assert_ne!(captured, head_tree, "capture silently dropped the edit");
    }

    #[tokio::test]
    async fn capture_tree_errors_outside_a_repo() {
        let dir = tempdir().unwrap();
        assert!(capture_tree(dir.path()).await.is_err());
    }

    #[tokio::test]
    async fn snapshot_default_restore_is_noop() {
        // Default snapshot (clean tree / empty workspace path) restores cleanly.
        let dir = tempdir().unwrap();
        let snap = WorkspaceSnapshot::default();
        snap.restore(dir.path()).await.unwrap();
    }
}
