//! Workspace snapshot/restore for turn-level undo.
//!
//! Captures workspace state before each agent turn so that an `undo`
//! operation can revert any file edits made by tools during that turn.
//!
//! Two backends:
//!
//! * **Git mode** — when the workspace is a git repo. Uses
//!   `git stash create` to produce a stash commit SHA *without* applying
//!   it. Restore replays that stash via `git restore --source <sha>`,
//!   recreating both the index and worktree.
//! * **Journal mode** — when the workspace is not a git repo. Walks the
//!   workspace and stores every file's bytes in memory. Capped at
//!   `DEFAULT_JOURNAL_CAP` (5 MiB) to bound memory; over the cap we
//!   record a "skipped" snapshot and restore is a no-op.
//!
//! `create` never propagates failures upward — a failure at snapshot
//! time degrades to `skipped: true` (with a `warn!` log) so it can never
//! abort the agent turn.

use dashmap::DashMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;
use tracing::warn;

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

    /// Drop every snapshot for a session. Called at session end.
    pub fn clear_session(&self, session_id: &str) {
        self.inner.retain(|(s, _), _| s != session_id);
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Captured workspace state for a single turn.
///
/// One of three shapes:
///
/// * `commit_id = Some(sha)` — git stash SHA. Restore replays it.
/// * `journal = Some(map)`   — in-memory file → bytes map.
/// * `skipped = true`        — snapshot was bypassed (e.g. cap exceeded
///   or git op failed unrecoverably). `restore` is a no-op.
///
/// The default value is the third (no-op) form, used when there's
/// nothing to snapshot (clean git tree, empty workspace, etc.).
#[derive(Debug, Clone, Default)]
pub struct WorkspaceSnapshot {
    /// Git stash commit SHA when the workspace is a git repo.
    pub commit_id: Option<String>,
    /// Path → original bytes map, populated for non-git workspaces.
    /// Paths are relative to the workspace root.
    pub journal: Option<HashMap<PathBuf, Vec<u8>>>,
    /// True when snapshotting was deliberately skipped (cap exceeded,
    /// git op failed, etc.). Restore is a no-op in that case.
    pub skipped: bool,
}

impl WorkspaceSnapshot {
    /// Capture workspace state, picking git or journal mode automatically.
    pub async fn create(workspace: &Path) -> Self {
        Self::create_with_cap(workspace, DEFAULT_JOURNAL_CAP).await
    }

    /// As [`Self::create`] but with an explicit journal byte cap. Mainly
    /// for tests; production code uses the default.
    pub async fn create_with_cap(workspace: &Path, journal_cap: usize) -> Self {
        if is_git_repo(workspace).await {
            match git_stash_create(workspace).await {
                Ok(sha) if !sha.is_empty() => Self {
                    commit_id: Some(sha),
                    journal: None,
                    skipped: false,
                },
                Ok(_) => {
                    // Empty SHA = clean working tree, nothing to snapshot.
                    // Restore is a no-op which is correct: there were no
                    // changes to revert.
                    Self::default()
                }
                Err(e) => {
                    warn!(
                        workspace = %workspace.display(),
                        error = %e,
                        "git stash create failed; falling back to journal"
                    );
                    build_journal(workspace, journal_cap)
                }
            }
        } else {
            build_journal(workspace, journal_cap)
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
        if let Some(sha) = &self.commit_id {
            // `git restore --source <sha> --worktree --staged :(top)`
            // replays both the worktree and the index from the stash
            // commit, which is what we want: any files the turn created,
            // modified, or staged are reverted.
            let out = Command::new("git")
                .args([
                    "restore",
                    "--source",
                    sha,
                    "--worktree",
                    "--staged",
                    ":(top)",
                ])
                .current_dir(workspace)
                .output()
                .await?;
            if !out.status.success() {
                // Older git versions (< 2.23) lack `git restore`. Fall
                // back to checkout; this restores tracked files but
                // won't remove files the turn newly created. That's a
                // graceful degradation rather than a hard failure.
                let out2 = Command::new("git")
                    .args(["checkout", sha, "--", ":/"])
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
            // Default-shaped snapshot (no commit_id, no journal, not
            // skipped): there was nothing to snapshot, so nothing to do.
            Ok(())
        }
    }
}

async fn is_git_repo(p: &Path) -> bool {
    let out = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(p)
        .output()
        .await;
    matches!(out, Ok(o) if o.status.success())
}

async fn git_stash_create(p: &Path) -> std::io::Result<String> {
    // We want a single commit that captures *both* tracked changes and
    // untracked files (e.g. a tool that ran `Write new_file.txt`).
    // `git stash create` alone only captures tracked changes; it has no
    // `--include-untracked` flag — that's a `stash push` thing.
    //
    // Strategy: snapshot the current index to a tree, then build a
    // single commit whose tree includes everything in the worktree.
    //
    // 1. Save the current index tree (so we can restore it after).
    // 2. `git add -A` to stage every worktree file.
    // 3. `git write-tree` produces the snapshot tree SHA.
    // 4. `git commit-tree` wraps the tree in an orphan commit (no
    //    parent) which is enough to be passed to `git restore --source`.
    // 5. Restore the original index via `read-tree`.
    //
    // The worktree is never touched. The committed objects are loose
    // (unreachable from any ref) and will be GC'd eventually.

    let saved_index = Command::new("git")
        .args(["write-tree"])
        .current_dir(p)
        .output()
        .await?;
    let saved_tree = if saved_index.status.success() {
        Some(
            String::from_utf8_lossy(&saved_index.stdout)
                .trim()
                .to_string(),
        )
    } else {
        None
    };

    // Stage every worktree file (additions, modifications, deletions).
    // Errors here are non-fatal; if `add` fails the subsequent
    // `write-tree` will simply reflect the prior index, which is still
    // a valid (though incomplete) snapshot.
    let _ = Command::new("git")
        .args(["add", "-A", "."])
        .current_dir(p)
        .output()
        .await;

    let tree_out = Command::new("git")
        .args(["write-tree"])
        .current_dir(p)
        .output()
        .await?;

    // Always restore the prior index, even on failure paths below.
    let restore_index = || async {
        if let Some(t) = saved_tree.as_ref() {
            let _ = Command::new("git")
                .args(["read-tree", t])
                .current_dir(p)
                .output()
                .await;
        }
    };

    if !tree_out.status.success() {
        restore_index().await;
        return Err(std::io::Error::other(format!(
            "git write-tree failed: {}",
            String::from_utf8_lossy(&tree_out.stderr)
        )));
    }
    let tree_sha = String::from_utf8_lossy(&tree_out.stdout).trim().to_string();

    // Wrap the tree in a commit object so callers can pass it to
    // `git restore --source <sha>`.
    let commit_out = Command::new("git")
        .args(["commit-tree", &tree_sha, "-m", "crucible-snapshot"])
        .current_dir(p)
        .output()
        .await?;

    restore_index().await;

    if !commit_out.status.success() {
        return Err(std::io::Error::other(format!(
            "git commit-tree failed: {}",
            String::from_utf8_lossy(&commit_out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&commit_out.stdout)
        .trim()
        .to_string())
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
        commit_id: None,
        journal: Some(journal),
        skipped: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

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
        // Initial commit so HEAD exists; without a HEAD commit
        // `git stash create` returns failure on some git versions.
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

    #[tokio::test]
    async fn snapshot_git_captures_uncommitted_change_and_restores() {
        let dir = tempdir().unwrap();
        git_init(dir.path()).await;
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let snap = WorkspaceSnapshot::create(dir.path()).await;
        assert!(snap.commit_id.is_some(), "expected git stash sha");
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
        let snap = WorkspaceSnapshot::create(dir.path()).await;
        assert!(snap.journal.is_some());
        assert!(snap.commit_id.is_none());
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
        let snap = WorkspaceSnapshot::create_with_cap(dir.path(), 5 * 1024 * 1024).await;
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
        let snap = WorkspaceSnapshot::create(dir.path()).await;
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

        let snap = WorkspaceSnapshot::create(dir.path()).await;
        let journal = snap.journal.expect("expected journal mode");
        assert!(journal.contains_key(&PathBuf::from("a.txt")));
        assert!(!journal
            .keys()
            .any(|k| k.starts_with(".git") || k.starts_with("target")));
    }

    #[tokio::test]
    async fn snapshot_default_restore_is_noop() {
        // Default snapshot (clean tree / empty workspace path) restores cleanly.
        let dir = tempdir().unwrap();
        let snap = WorkspaceSnapshot::default();
        snap.restore(dir.path()).await.unwrap();
    }
}
