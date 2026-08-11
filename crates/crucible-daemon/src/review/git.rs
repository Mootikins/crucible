//! Git plumbing for the ledger.
//!
//! Shelling out rather than using `gix` in-process: the capture side already
//! has to be `git add -A` (there is no equivalent one-liner for "stage the
//! worktree into a scratch index"), and splitting the two halves across two
//! git implementations is how subtle disagreements about `.gitignore` and
//! path normalisation get in. Argument vectors only — never a shell string.

use std::path::Path;

use crucible_core::session::{PhysicalRoot, TreeSha};
use tokio::process::Command;

use super::error::{ReviewError, ReviewResult};

/// How a path differs between two trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

impl ChangeKind {
    /// True when the path exists in the *from* tree.
    pub(super) fn has_before(self) -> bool {
        !matches!(self, ChangeKind::Added)
    }

    /// True when the path exists in the *to* tree.
    pub(super) fn has_after(self) -> bool {
        !matches!(self, ChangeKind::Deleted)
    }
}

/// The repository top level containing `path`.
///
/// Every root the ledger tracks is normalised to its top level, because
/// `git write-tree` emits repo-root-relative paths regardless of where it
/// runs. Capture, diff and restore then all speak the same coordinates —
/// the alternative (capture scoped to a subdirectory, restore scoped to
/// `:(top)`) reverts sibling directories that were never part of the review.
/// This is the one place a [`PhysicalRoot`] is minted: `--show-toplevel`
/// resolves symlinks and ignores `PWD`, so its output is the canonical
/// spelling every stored root and every hashed identity agrees on.
pub(super) async fn top_level(path: &Path) -> ReviewResult<PhysicalRoot> {
    let out = git(path, &["rev-parse", "--show-toplevel"]).await;
    match out {
        Ok(s) => Ok(PhysicalRoot::from_top_level(s.trim())),
        Err(_) => Err(ReviewError::NotAGitRepo {
            path: path.to_path_buf(),
        }),
    }
}

/// Paths differing between two trees, with the kind of change.
///
/// Renames are deliberately not detected: a rename becomes a delete plus an
/// add, which is what the composed diff should show anyway — the file at the
/// new path is entirely new content to review.
pub(super) async fn changed_paths(
    root: &Path,
    from: &TreeSha,
    to: &TreeSha,
) -> ReviewResult<Vec<(String, ChangeKind)>> {
    let out = git(
        root,
        &[
            "diff",
            "--name-status",
            "--no-renames",
            "-z",
            from.as_str(),
            to.as_str(),
        ],
    )
    .await?;

    // `-z` output is `status NUL path NUL` repeated; NUL-separation is the
    // only encoding that survives a filename containing a quote or newline.
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut changes = Vec::new();
    while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
        let kind = match status.chars().next() {
            Some('A') => ChangeKind::Added,
            Some('D') => ChangeKind::Deleted,
            Some('M' | 'T') => ChangeKind::Modified,
            // Unmerged, unknown, or a mode-only change: nothing line-oriented
            // to review.
            _ => continue,
        };
        changes.push((path.to_string(), kind));
    }
    Ok(changes)
}

/// File content at `tree:path`.
///
/// `Ok(None)` means the blob is not UTF-8 — a binary file, which has no line
/// hunks to review. Callers skip it rather than fabricating an empty diff.
pub(super) async fn blob(root: &Path, tree: &TreeSha, path: &str) -> ReviewResult<Option<String>> {
    let spec = format!("{}:{}", tree.as_str(), path);
    let out = Command::new("git")
        .args(["cat-file", "blob", &spec])
        .current_dir(root)
        .output()
        .await?;
    if !out.status.success() {
        return Err(ReviewError::Git(format!(
            "cat-file {spec}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8(out.stdout).ok())
}

/// One side of a change: the blob at `tree:path`, or empty text when
/// [`ChangeKind`] says the path does not exist on that side.
///
/// Asking git for a blob that is not in the tree is an error, not an empty
/// answer, so the `exists` short-circuit is what lets an add and a delete go
/// through the same code path as a modification. `Ok(None)` still means
/// binary — see [`blob`].
pub(super) async fn blob_or_empty(
    root: &Path,
    tree: &TreeSha,
    path: &str,
    exists: bool,
) -> ReviewResult<Option<String>> {
    if !exists {
        return Ok(Some(String::new()));
    }
    blob(root, tree, path).await
}

// ── Retention ───────────────────────────────────────────────────────────────
//
// Every tree the ledger records comes from `git write-tree`, which makes it an
// object no ref points at. `git gc --prune=now` in the user's own repo deletes
// it, and once the session base is gone the composed diff is not merely stale
// — it is uncomputable, and the review queue for that session is lost.
//
// One ref per (session, repository) fixes that. Verified against git 2.55: a
// single such ref protects every tree it names transitively through
// `gc --prune=now --aggressive`, `fsck` stays clean, and — because the ref is
// *tree*-valued rather than commit-valued — it is invisible to `git log --all`,
// so it does not clutter the user's history the way an orphan commit would.

/// The keep ref holding one session's trees inside one repository.
///
/// The session id alone is enough: roots are normalised to their repository
/// top level, so two roots that would collide on a ref name are the same root.
pub(super) fn keep_ref(session_id: &str) -> String {
    format!("refs/crucible/sessions/{session_id}")
}

/// Point the session's keep ref at a tree naming every SHA in `trees`.
///
/// Idempotent and total: the ref is rebuilt from the full list on every call
/// rather than appended to, so a ledger that lost an interval also loses its
/// claim on that interval's trees, and there is no state to get out of sync.
pub(super) async fn update_keep(
    root: &Path,
    session_id: &str,
    trees: &[TreeSha],
) -> ReviewResult<()> {
    // Entry name is the SHA itself: `mktree` rejects duplicate names, and the
    // SHA is the one thing guaranteed unique among the trees we are keeping.
    let mut input = String::new();
    for tree in trees {
        input.push_str(&format!(
            "040000 tree {}\t{}\n",
            tree.as_str(),
            tree.as_str()
        ));
    }
    let keep = git_stdin(root, &["mktree"], &input).await?;
    git(root, &["update-ref", &keep_ref(session_id), keep.trim()]).await?;
    Ok(())
}

/// Release a session's claim on its trees, leaving them to normal gc.
pub(super) async fn drop_keep(root: &Path, session_id: &str) -> ReviewResult<()> {
    // `update-ref -d` on a ref that is already gone succeeds, so deleting
    // twice — the delete path and the sweep — is not an error.
    git(root, &["update-ref", "-d", &keep_ref(session_id)]).await?;
    Ok(())
}

/// Every crucible keep ref in this repository, as `(session id, ref name)`.
///
/// Deliberately the whole `refs/crucible/` namespace, not just this module's
/// `sessions/` corner. Turn-undo grew its own keep refs under
/// `refs/crucible/snapshots/{session}/{node}` and no sweeper knew about them,
/// so a daemon that crashed — or a session directory removed out of band —
/// pinned those trees in the user's repository forever. A sweep that only
/// knows one of the two namespaces is a leak by construction.
pub(super) async fn keep_refs(root: &Path) -> ReviewResult<Vec<(String, String)>> {
    let out = git(
        root,
        &["for-each-ref", "--format=%(refname)", "refs/crucible/"],
    )
    .await?;
    Ok(out
        .lines()
        .filter_map(|line| {
            let name = line.trim();
            let rest = name.strip_prefix("refs/crucible/")?;
            // `sessions/{id}` (the ledger) or `snapshots/{id}/{node}` (undo).
            let session = match rest.split_once('/') {
                Some(("sessions", id)) => id,
                Some(("snapshots", tail)) => tail.split('/').next()?,
                _ => return None,
            };
            (!session.is_empty()).then(|| (session.to_string(), name.to_string()))
        })
        .collect())
}

/// Delete one ref by name, whichever namespace it is in.
pub(super) async fn drop_ref(root: &Path, name: &str) -> ReviewResult<()> {
    git(root, &["update-ref", "-d", name]).await?;
    Ok(())
}

/// Whether `tree` is still in this repository's object store.
///
/// The `^{tree}` peel is load-bearing: `cat-file -e` on a bare SHA answers for
/// any object type, and a tree SHA that has been gc'd can collide with nothing
/// — but a caller that passed a commit by mistake would get a false positive
/// and then diff the wrong pair of objects.
pub(super) async fn tree_exists(root: &Path, tree: &TreeSha) -> bool {
    git(
        root,
        &["cat-file", "-e", &format!("{}^{{tree}}", tree.as_str())],
    )
    .await
    .is_ok()
}

/// Run git in `root` and return stdout, mapping a non-zero exit to
/// [`ReviewError::Git`] with git's own stderr.
async fn git(root: &Path, args: &[&str]) -> ReviewResult<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .await?;
    if !out.status.success() {
        return Err(ReviewError::Git(format!(
            "{}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// As [`git`], feeding `input` on stdin. `mktree` is the only caller.
async fn git_stdin(root: &Path, args: &[&str], input: &str) -> ReviewResult<String> {
    use tokio::io::AsyncWriteExt;

    let mut child = Command::new("git")
        .args(args)
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    // Taken and dropped before `wait_with_output`, or git never sees EOF and
    // both sides wait forever.
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).await?;
        stdin.shutdown().await?;
    }
    let out = child.wait_with_output().await?;
    if !out.status.success() {
        return Err(ReviewError::Git(format!(
            "{}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
