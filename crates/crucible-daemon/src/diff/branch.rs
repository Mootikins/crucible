//! The git reads of a branch diffset.
//!
//! These functions call [`crate::scm::run_git`]. They do not use
//! `review/git.rs`: that module is private to the ledger, and it turns rename
//! detection off. A branch diff detects renames.
//!
//! Each git call uses `-z`. A NUL byte is the only separator that a path with
//! a quote, a tab or a newline does not break.

use std::collections::HashMap;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};
use crucible_core::diff::{DiffFileEntry, FileStatus};
use crucible_core::session::PhysicalRoot;
use crucible_core::types::acp::MAX_DIFF_BYTES;

use crate::scm::{run_git, GitOpts};

/// Git looks for a NUL byte in this many leading bytes to find a binary file.
/// This module uses the same test for the files that git does not diff.
const BINARY_PROBE_BYTES: usize = 8000;

/// The text of one file on one side of a branch diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileText {
    /// No file is at the path on this side.
    Absent,
    Text(String),
    /// The file has a NUL byte. It has no text.
    Binary,
    /// The file is larger than [`MAX_DIFF_BYTES`]. It has no text.
    TooLarge,
}

impl FileText {
    /// The text to show, or `None` when the side has no text to show.
    pub fn into_shown(self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text),
            Self::Absent | Self::Binary | Self::TooLarge => None,
        }
    }
}

async fn git(root: &Path, args: &[&str]) -> Result<String> {
    Ok(run_git(root, args, GitOpts::default()).await?)
}

/// Refuse a revision that git can read as an option.
fn check_rev(rev: &str) -> Result<()> {
    if rev.is_empty() || rev.starts_with('-') {
        bail!("not a git revision: {rev:?}");
    }
    Ok(())
}

/// Refuse a path that is not a plain path below the root.
fn check_relative(path: &str) -> Result<()> {
    let plain = !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
    if !plain {
        bail!("not a path relative to the root: {path:?}");
    }
    Ok(())
}

/// The branch that a branch diff compares against when the user names none.
///
/// The order is `origin/HEAD`, then a local `main`, then a local `master`.
pub async fn default_branch(root: &Path) -> Result<String> {
    if let Ok(target) = git(
        root,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    )
    .await
    {
        let target = target.trim();
        // `origin/HEAD` can point at a remote branch that a prune removed.
        if let Some(name) = target.strip_prefix("refs/remotes/") {
            if git(root, &["rev-parse", "--verify", "--quiet", target])
                .await
                .is_ok()
            {
                return Ok(name.to_string());
            }
        }
    }
    for name in ["main", "master"] {
        let full = format!("refs/heads/{name}");
        if git(root, &["rev-parse", "--verify", "--quiet", &full])
            .await
            .is_ok()
        {
            return Ok(name.to_string());
        }
    }
    bail!(
        "{}: found no origin/HEAD, main or master branch",
        root.display()
    )
}

/// The merge base of `head` with `base`. `None` means the checked-out `HEAD`.
pub async fn merge_base(root: &Path, base: &str, head: Option<&str>) -> Result<String> {
    let head = head.unwrap_or("HEAD");
    check_rev(base)?;
    check_rev(head)?;
    let out = git(root, &["merge-base", base, head]).await?;
    Ok(out.trim().to_string())
}

/// The files that changed from the commit `from` to the commit `to`.
///
/// When `to` is `None`, the other side is the working tree. The list then
/// includes the untracked files that `.gitignore` does not exclude, as added
/// files. The list is sorted by path.
pub async fn changes(
    root: &PhysicalRoot,
    from: &str,
    to: Option<&str>,
) -> Result<Vec<DiffFileEntry>> {
    check_rev(from)?;
    let mut revs = vec![from];
    if let Some(to) = to {
        check_rev(to)?;
        revs.push(to);
    }
    revs.push("--");
    let diff = |format: &'static str| {
        let args = [
            &["diff", "--no-ext-diff", "--no-textconv", "-z", "-M", format][..],
            &revs[..],
        ]
        .concat();
        async move { git(root, &args).await }
    };
    let name_status = diff("--name-status").await?;
    let numstat = diff("--numstat").await?;

    let statuses = parse_name_status(&name_status);
    let counts = parse_numstat(&numstat);

    let base_paths: Vec<&str> = statuses
        .iter()
        .filter_map(|(path, status)| match status {
            FileStatus::Added => None,
            FileStatus::Modified | FileStatus::Deleted => Some(path.as_str()),
            FileStatus::Renamed { from } => Some(from.as_str()),
        })
        .collect();
    let head_paths: Vec<&str> = statuses
        .iter()
        .filter(|(_, status)| !matches!(status, FileStatus::Deleted))
        .map(|(path, _)| path.as_str())
        .collect();
    let base_sizes = blob_sizes(root, from, &base_paths).await?;
    let head_sizes = match to {
        Some(to) => blob_sizes(root, to, &head_paths).await?,
        None => disk_sizes(root, &head_paths).await,
    };

    let mut entries = Vec::with_capacity(statuses.len());
    for (path, status) in statuses {
        let base_path = match &status {
            FileStatus::Renamed { from } => from.as_str(),
            _ => path.as_str(),
        };
        let over = |sizes: &HashMap<String, u64>, p: &str| {
            sizes.get(p).is_some_and(|&n| n > MAX_DIFF_BYTES as u64)
        };
        let too_large = over(&base_sizes, base_path) || over(&head_sizes, &path);
        let (added, removed, binary) = match counts.get(&path) {
            Some(Some((added, removed))) => (*added, *removed, false),
            Some(None) => (0, 0, true),
            // git lists each path in both outputs. A missing count is a
            // change with no lines, such as a mode change.
            None => (0, 0, false),
        };
        entries.push(DiffFileEntry {
            root: root.clone(),
            path,
            status,
            added,
            removed,
            binary,
            too_large,
        });
    }

    if to.is_none() {
        let untracked = git(root, &["ls-files", "--others", "--exclude-standard", "-z"]).await?;
        for path in untracked.split('\0').filter(|p| !p.is_empty()) {
            entries.push(untracked_entry(root, path).await?);
        }
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

/// The text of `path` at the commit `rev`, or in the working tree when `rev`
/// is `None`.
pub async fn file_text(root: &Path, rev: Option<&str>, path: &str) -> Result<FileText> {
    check_relative(path)?;
    let Some(rev) = rev else {
        return disk_text(&root.join(path)).await;
    };
    check_rev(rev)?;
    let listing = git(
        root,
        &[
            "--literal-pathspecs",
            "ls-tree",
            "-l",
            "-z",
            rev,
            "--",
            path,
        ],
    )
    .await?;
    let Some(entry) = parse_ls_tree(&listing).into_iter().find(|e| e.path == path) else {
        return Ok(FileText::Absent);
    };
    // A directory or a submodule at the path is not a file of the diff.
    if entry.kind != "blob" {
        return Ok(FileText::Absent);
    }
    if entry.size.is_some_and(|n| n > MAX_DIFF_BYTES as u64) {
        return Ok(FileText::TooLarge);
    }
    let text = git(root, &["cat-file", "blob", entry.object]).await?;
    Ok(classify_text(text))
}

/// Parse `git diff -z --name-status -M` into paths and their status.
fn parse_name_status(out: &str) -> Vec<(String, FileStatus)> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut statuses = Vec::new();
    while let Some(code) = fields.next() {
        // A rename or a copy has two paths: the old path, then the new path.
        let status = match code.chars().next() {
            Some('R') => {
                let (Some(from), Some(to)) = (fields.next(), fields.next()) else {
                    break;
                };
                statuses.push((
                    to.to_string(),
                    FileStatus::Renamed {
                        from: from.to_string(),
                    },
                ));
                continue;
            }
            Some('C') => {
                let (Some(_), Some(to)) = (fields.next(), fields.next()) else {
                    break;
                };
                statuses.push((to.to_string(), FileStatus::Added));
                continue;
            }
            Some('A') => FileStatus::Added,
            Some('D') => FileStatus::Deleted,
            // `U` is a path with a merge conflict in the working tree.
            Some('M' | 'T' | 'U') => FileStatus::Modified,
            _ => {
                fields.next();
                continue;
            }
        };
        let Some(path) = fields.next() else {
            break;
        };
        statuses.push((path.to_string(), status));
    }
    statuses
}

/// Parse `git diff -z --numstat -M` into the counts of each new path.
/// `None` marks a binary file: git writes `-` for both of its counts.
fn parse_numstat(out: &str) -> HashMap<String, Option<(u32, u32)>> {
    let mut fields = out.split('\0');
    let mut counts = HashMap::new();
    while let Some(record) = fields.next() {
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        // A rename has an empty path here. The old path and the new path
        // follow as two more fields.
        let path = if path.is_empty() {
            let (Some(_from), Some(to)) = (fields.next(), fields.next()) else {
                break;
            };
            to
        } else {
            path
        };
        let pair = added.parse().ok().zip(removed.parse().ok());
        counts.insert(path.to_string(), pair);
    }
    counts
}

struct TreeEntry<'a> {
    kind: &'a str,
    object: &'a str,
    size: Option<u64>,
    path: &'a str,
}

/// Parse `git ls-tree -l -z`. A tree or a submodule has no size.
fn parse_ls_tree(out: &str) -> Vec<TreeEntry<'_>> {
    out.split('\0')
        .filter_map(|record| {
            let (meta, path) = record.split_once('\t')?;
            let mut meta = meta.split_whitespace();
            let (_mode, kind, object, size) =
                (meta.next()?, meta.next()?, meta.next()?, meta.next()?);
            Some(TreeEntry {
                kind,
                object,
                size: size.parse().ok(),
                path,
            })
        })
        .collect()
}

/// The size of each of `paths` at the commit `rev`.
async fn blob_sizes(root: &Path, rev: &str, paths: &[&str]) -> Result<HashMap<String, u64>> {
    if paths.is_empty() {
        return Ok(HashMap::new());
    }
    let mut args = vec!["--literal-pathspecs", "ls-tree", "-l", "-z", rev, "--"];
    args.extend_from_slice(paths);
    let out = git(root, &args).await?;
    Ok(parse_ls_tree(&out)
        .into_iter()
        .filter_map(|e| Some((e.path.to_string(), e.size?)))
        .collect())
}

/// The size of each of `paths` in the working tree. A path that is not on
/// disk has no size.
async fn disk_sizes(root: &Path, paths: &[&str]) -> HashMap<String, u64> {
    let mut sizes = HashMap::new();
    for path in paths {
        if let Ok(meta) = tokio::fs::symlink_metadata(root.join(path)).await {
            sizes.insert((*path).to_string(), meta.len());
        }
    }
    sizes
}

/// The entry of a file that git does not track. git gives no counts for it,
/// so this function reads the file.
async fn untracked_entry(root: &PhysicalRoot, path: &str) -> Result<DiffFileEntry> {
    let mut entry = DiffFileEntry {
        root: root.clone(),
        path: path.to_string(),
        status: FileStatus::Added,
        added: 0,
        removed: 0,
        binary: false,
        too_large: false,
    };
    match disk_text(&root.join(path)).await? {
        FileText::Text(text) => {
            entry.added = u32::try_from(text.lines().count()).unwrap_or(u32::MAX);
        }
        FileText::Binary => entry.binary = true,
        FileText::TooLarge => entry.too_large = true,
        // The file went away after git listed it.
        FileText::Absent => {}
    }
    Ok(entry)
}

/// The text of a file in the working tree. A symbolic link gives its target,
/// as git stores it.
pub(crate) async fn disk_text(path: &Path) -> Result<FileText> {
    let meta = match tokio::fs::symlink_metadata(path).await {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(FileText::Absent),
        Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
    };
    if meta.is_symlink() {
        let target = tokio::fs::read_link(path)
            .await
            .with_context(|| format!("read link {}", path.display()))?;
        return Ok(FileText::Text(target.to_string_lossy().into_owned()));
    }
    if !meta.is_file() {
        return Ok(FileText::Absent);
    }
    if meta.len() > MAX_DIFF_BYTES as u64 {
        return Ok(FileText::TooLarge);
    }
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("read {}", path.display()))?;
    Ok(classify_text(String::from_utf8_lossy(&bytes).into_owned()))
}

fn classify_text(text: String) -> FileText {
    let probe = &text.as_bytes()[..text.len().min(BINARY_PROBE_BYTES)];
    if probe.contains(&0) {
        FileText::Binary
    } else {
        FileText::Text(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{git, init_repo};
    use tempfile::TempDir;

    /// A repository on the branch `main` with `files` in one commit.
    async fn repo(files: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        init_repo(tmp.path(), files).await;
        git(tmp.path(), &["branch", "-M", "main"]).await;
        tmp
    }

    async fn commit_all(dir: &Path, message: &str) {
        git(dir, &["add", "-A"]).await;
        git(dir, &["commit", "-q", "-m", message]).await;
    }

    async fn head(dir: &Path) -> String {
        git(dir, &["rev-parse", "HEAD"]).await.trim().to_string()
    }

    fn physical(tmp: &TempDir) -> PhysicalRoot {
        PhysicalRoot::from_top_level(tmp.path())
    }

    #[tokio::test]
    async fn default_branch_reads_origin_head_then_falls_back() {
        let tmp = repo(&[("a.txt", "a\n")]).await;
        let dir = tmp.path();

        git(dir, &["branch", "-M", "dev"]).await;
        assert!(default_branch(dir).await.is_err());

        git(dir, &["branch", "-M", "master"]).await;
        assert_eq!(default_branch(dir).await.unwrap(), "master");

        git(dir, &["branch", "main"]).await;
        assert_eq!(default_branch(dir).await.unwrap(), "main");

        git(dir, &["update-ref", "refs/remotes/origin/trunk", "HEAD"]).await;
        git(
            dir,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ],
        )
        .await;
        assert_eq!(default_branch(dir).await.unwrap(), "origin/trunk");

        // A dangling `origin/HEAD` falls back to the local branch.
        git(dir, &["update-ref", "-d", "refs/remotes/origin/trunk"]).await;
        assert_eq!(default_branch(dir).await.unwrap(), "main");

        let plain = TempDir::new().unwrap();
        assert!(default_branch(plain.path()).await.is_err());
    }

    #[tokio::test]
    async fn merge_base_of_a_feature_branch() {
        let tmp = repo(&[("a.txt", "a\n")]).await;
        let dir = tmp.path();
        let fork = head(dir).await;

        git(dir, &["checkout", "-q", "-b", "feature"]).await;
        std::fs::write(dir.join("b.txt"), "b\n").unwrap();
        commit_all(dir, "feature").await;

        git(dir, &["checkout", "-q", "main"]).await;
        std::fs::write(dir.join("c.txt"), "c\n").unwrap();
        commit_all(dir, "main moves").await;
        let main_tip = head(dir).await;

        assert_eq!(
            merge_base(dir, "main", Some("feature")).await.unwrap(),
            fork
        );
        assert_eq!(merge_base(dir, "main", None).await.unwrap(), main_tip);
        git(dir, &["checkout", "-q", "feature"]).await;
        assert_eq!(merge_base(dir, "main", None).await.unwrap(), fork);

        assert!(merge_base(dir, "--all", None).await.is_err());
        assert!(merge_base(dir, "no-such-branch", None).await.is_err());
    }

    #[tokio::test]
    async fn changes_detect_a_rename() {
        let body = "one\ntwo\nthree\nfour\nfive\n";
        let tmp = repo(&[("old.md", body), ("keep.md", "a\nb\n"), ("gone.md", "x\n")]).await;
        let dir = tmp.path();
        let base = head(dir).await;

        git(dir, &["checkout", "-q", "-b", "feature"]).await;
        git(dir, &["mv", "old.md", "new.md"]).await;
        std::fs::write(dir.join("keep.md"), "a\nB\nc\n").unwrap();
        std::fs::remove_file(dir.join("gone.md")).unwrap();
        std::fs::write(dir.join("fresh.md"), "f\n").unwrap();
        commit_all(dir, "feature").await;

        let status_of = |entries: &[DiffFileEntry]| {
            entries
                .iter()
                .map(|e| (e.path.clone(), e.status.clone(), e.added, e.removed))
                .collect::<Vec<_>>()
        };
        let expected = vec![
            ("fresh.md".to_string(), FileStatus::Added, 1, 0),
            ("gone.md".to_string(), FileStatus::Deleted, 0, 1),
            ("keep.md".to_string(), FileStatus::Modified, 2, 1),
            (
                "new.md".to_string(),
                FileStatus::Renamed {
                    from: "old.md".into(),
                },
                0,
                0,
            ),
        ];

        let committed = changes(&physical(&tmp), &base, Some("feature"))
            .await
            .unwrap();
        assert_eq!(status_of(&committed), expected);
        assert!(committed.iter().all(|e| !e.binary && !e.too_large));
        assert!(committed.iter().all(|e| e.root == physical(&tmp)));

        // The working tree gives the same list when it is clean.
        let working = changes(&physical(&tmp), &base, None).await.unwrap();
        assert_eq!(status_of(&working), expected);

        assert_eq!(
            file_text(dir, Some(&base), "old.md").await.unwrap(),
            FileText::Text(body.into())
        );
        assert_eq!(
            file_text(dir, Some("feature"), "old.md").await.unwrap(),
            FileText::Absent
        );
        assert_eq!(
            file_text(dir, None, "new.md").await.unwrap(),
            FileText::Text(body.into())
        );
        assert_eq!(
            file_text(dir, None, "gone.md").await.unwrap(),
            FileText::Absent
        );
        assert!(file_text(dir, None, "../outside").await.is_err());
        assert!(file_text(dir, None, "/etc/passwd").await.is_err());
    }

    #[tokio::test]
    async fn changes_mark_a_binary_file() {
        let tmp = repo(&[("a.txt", "a\n")]).await;
        let dir = tmp.path();
        let base = head(dir).await;
        std::fs::write(dir.join("image.bin"), b"\x89PNG\0\0\x01\x02").unwrap();
        commit_all(dir, "binary").await;

        let entries = changes(&physical(&tmp), &base, Some("HEAD")).await.unwrap();
        let [entry] = entries.as_slice() else {
            panic!("{entries:?}");
        };
        assert_eq!(entry.path, "image.bin");
        assert!(entry.binary);
        assert_eq!((entry.added, entry.removed), (0, 0));

        assert_eq!(
            file_text(dir, Some("HEAD"), "image.bin").await.unwrap(),
            FileText::Binary
        );
        assert_eq!(
            file_text(dir, None, "image.bin").await.unwrap(),
            FileText::Binary
        );
    }

    #[tokio::test]
    async fn an_untracked_file_is_added() {
        let tmp = repo(&[("a.txt", "a\n"), (".gitignore", "ignored.txt\n")]).await;
        let dir = tmp.path();
        let base = head(dir).await;
        std::fs::create_dir(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/new.txt"), "one\ntwo").unwrap();
        std::fs::write(dir.join("ignored.txt"), "no\n").unwrap();
        std::fs::write(dir.join("blob.bin"), b"a\0b").unwrap();

        let entries = changes(&physical(&tmp), &base, None).await.unwrap();
        let listed: Vec<_> = entries
            .iter()
            .map(|e| (e.path.as_str(), &e.status, e.added, e.binary))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("blob.bin", &FileStatus::Added, 0, true),
                ("sub/new.txt", &FileStatus::Added, 2, false),
            ]
        );

        // A diff between two commits has no untracked files.
        assert!(changes(&physical(&tmp), &base, Some("HEAD"))
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_file_over_the_limit_is_too_large() {
        let tmp = repo(&[("a.txt", "a\n")]).await;
        let dir = tmp.path();
        let base = head(dir).await;
        let big = "x\n".repeat(MAX_DIFF_BYTES / 2 + 1);
        std::fs::write(dir.join("big.txt"), &big).unwrap();
        commit_all(dir, "big").await;
        std::fs::write(dir.join("loose.txt"), &big).unwrap();

        let entries = changes(&physical(&tmp), &base, None).await.unwrap();
        let flags: Vec<_> = entries
            .iter()
            .map(|e| (e.path.as_str(), e.too_large))
            .collect();
        assert_eq!(flags, vec![("big.txt", true), ("loose.txt", true)]);

        let committed = changes(&physical(&tmp), &base, Some("HEAD")).await.unwrap();
        assert!(committed[0].too_large);

        // The old side alone can be too large.
        std::fs::write(dir.join("big.txt"), "small\n").unwrap();
        let shrunk = changes(&physical(&tmp), &head(dir).await, None)
            .await
            .unwrap();
        let big_entry = shrunk.iter().find(|e| e.path == "big.txt").unwrap();
        assert!(big_entry.too_large);
        assert_eq!(big_entry.status, FileStatus::Modified);

        assert_eq!(
            file_text(dir, Some("HEAD"), "big.txt").await.unwrap(),
            FileText::TooLarge
        );
        assert_eq!(
            file_text(dir, None, "loose.txt").await.unwrap(),
            FileText::TooLarge
        );
        assert_eq!(
            file_text(dir, None, "big.txt").await.unwrap(),
            FileText::Text("small\n".into())
        );
    }
}
