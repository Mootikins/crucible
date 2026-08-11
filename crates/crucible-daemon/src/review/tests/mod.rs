//! Ledger tests against real git fixtures.
//!
//! These exercise the two properties the design hangs on: a hunk's identity
//! survives unrelated edits, and attribution never claims a tool call made a
//! change it cannot account for.
//!
//! Grouped by what each file pins rather than by call order; the fixtures below
//! are shared, everything else lives in one of the submodules.

mod attribution;
mod delegation;
mod gate;
mod identity;
mod persistence;
mod retention;

use std::path::{Path, PathBuf};

use crucible_core::session::{
    CommentAuthor, ComposedHunk, HunkId, LineRange, ReviewState, SkipKind, Verdict,
};
use tempfile::TempDir;
use tokio::process::Command;

use super::*;
use crate::test_support::{git, init_repo as repo};

/// A git repo with one committed file, plus the ledger tracking it.
/// Session ids holding a keep ref in `root`, in whichever namespace.
///
/// The sweeper works in `(id, refname)` pairs because it deletes by name;
/// tests only ever care which sessions are still pinned.
async fn keep_ref_ids(root: &Path) -> ReviewResult<Vec<String>> {
    Ok(super::git::keep_refs(root)
        .await?
        .into_iter()
        .map(|(id, _)| id)
        .collect())
}

struct Fixture {
    dir: TempDir,
    ledgers: Arc<ReviewLedgers>,
    session: String,
}

impl Fixture {
    async fn new(initial: &str) -> Self {
        let dir = TempDir::new().unwrap();
        git(dir.path(), &["init", "-q"]).await;
        git(dir.path(), &["config", "user.email", "t@t"]).await;
        git(dir.path(), &["config", "user.name", "t"]).await;
        std::fs::write(dir.path().join("a.txt"), initial).unwrap();
        git(dir.path(), &["add", "."]).await;
        git(dir.path(), &["commit", "-q", "-m", "init"]).await;

        let ledgers = Arc::new(ReviewLedgers::default());
        ledgers
            .open("sess", &[dir.path().to_path_buf()])
            .await
            .unwrap();
        Self {
            dir,
            ledgers,
            session: "sess".to_string(),
        }
    }

    fn write(&self, contents: &str) {
        std::fs::write(self.dir.path().join("a.txt"), contents).unwrap();
    }

    fn read(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("a.txt")).unwrap()
    }

    /// Run one bracketed "tool call" that rewrites the file.
    async fn call(&self, tool_call_id: &str, node_id: u32, contents: &str) -> bool {
        let handle = self.ledgers.open_bracket(&self.session).await.unwrap();
        self.write(contents);
        self.ledgers
            .close(&self.session, handle, tool_call_id, node_id)
            .await
            .unwrap()
    }

    async fn hunks(&self) -> Vec<ComposedHunk> {
        self.ledgers.list_hunks(&self.session).await.unwrap()
    }
}

/// One spelling of one file, which is what a test gate query is. The gate
/// itself passes several, because a tool's relative path argument has no
/// single base — see `has_unreviewed_in_file`.
fn only(path: PathBuf) -> [PathBuf; 1] {
    [path]
}

fn find<'a>(hunks: &'a [ComposedHunk], after: &str) -> &'a ComposedHunk {
    hunks
        .iter()
        .find(|h| h.after_content == after)
        .unwrap_or_else(|| {
            panic!(
                "no hunk producing {after:?}; got {:?}",
                hunks.iter().map(|h| &h.after_content).collect::<Vec<_>>()
            )
        })
}

/// A repo with a journal beside it, so "restart" is a second `ReviewLedgers`
/// over the same session directory.
struct Persisted {
    repo_dir: TempDir,
    session_dir: TempDir,
    ledgers: Arc<ReviewLedgers>,
    session: String,
}

impl Persisted {
    async fn new(initial: &str) -> Self {
        Self::build(initial, false).await
    }

    /// As [`Persisted::new`], but with uncommitted work already in the
    /// worktree when the ledger opens.
    ///
    /// The distinction matters only for retention: on a clean checkout
    /// `write-tree` returns `HEAD`'s own tree, which is reachable and which gc
    /// would never prune — so a test that needs a prunable base has to make
    /// one.
    async fn new_with_uncommitted(initial: &str) -> Self {
        Self::build(initial, true).await
    }

    async fn build(initial: &str, uncommitted: bool) -> Self {
        let repo_dir = TempDir::new().unwrap();
        repo(repo_dir.path(), &[("a.txt", initial)]).await;
        if uncommitted {
            std::fs::write(repo_dir.path().join("uncommitted.txt"), "scratch\n").unwrap();
        }
        let session_dir = TempDir::new().unwrap();

        let ledgers = Arc::new(ReviewLedgers::default());
        ledgers
            .open_or_restore("sess", session_dir.path(), &[repo_dir.path().to_path_buf()])
            .await
            .unwrap();
        Self {
            repo_dir,
            session_dir,
            ledgers,
            session: "sess".to_string(),
        }
    }

    fn journal(&self) -> PathBuf {
        self.session_dir.path().join("review.jsonl")
    }

    fn write(&self, contents: &str) {
        std::fs::write(self.repo_dir.path().join("a.txt"), contents).unwrap();
    }

    async fn call(&self, tool_call_id: &str, contents: &str) {
        let handle = self.ledgers.open_bracket(&self.session).await.unwrap();
        self.write(contents);
        self.ledgers
            .close(&self.session, handle, tool_call_id, 1)
            .await
            .unwrap();
    }

    /// What a daemon restart sees: a brand new manager, same journal.
    async fn restart(&self) -> Arc<ReviewLedgers> {
        let ledgers = Arc::new(ReviewLedgers::default());
        ledgers
            .open_or_restore(
                &self.session,
                self.session_dir.path(),
                &[self.repo_dir.path().to_path_buf()],
            )
            .await
            .unwrap();
        ledgers
    }
}
