//! Source-control (git) helpers for the `scm.*` RPCs.
//!
//! Everything here shells out to the system `git` binary via
//! [`tokio::process::Command`] using **argument vectors only** — no command
//! string is ever handed to a shell, so branch names and paths cannot inject
//! flags or shell metacharacters. The pure parsing/validation/resolution
//! helpers are split out so they can be unit-tested without a repository.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// One branch in a [`ScmBranchesResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScmBranch {
    /// Short branch name (e.g. `master`, `feat/x`).
    pub name: String,
    /// Absolute path of the worktree that has this branch checked out, or
    /// `null` when the branch is not checked out anywhere.
    pub worktree_path: Option<String>,
    /// True for the branch checked out at the checkout the request `path`
    /// belongs to.
    pub is_current: bool,
    /// True for branches that exist only on a remote (no local ref yet).
    pub remote_only: bool,
}

/// Response for the `scm.branches` RPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScmBranchesResponse {
    /// Absolute path of the main checkout (first entry of `worktree list`).
    pub repo_root: String,
    /// Branch checked out at the checkout containing the request `path`;
    /// `null` when that checkout is in detached-HEAD state.
    pub current_branch: Option<String>,
    pub branches: Vec<ScmBranch>,
}

/// Response for the `scm.worktree_add` RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScmWorktreeAddResponse {
    /// Absolute path of the newly created worktree.
    pub path: String,
    /// The `Project` registered for the new worktree.
    pub project: crucible_core::Project,
    /// Set when the new worktree lives inside the repo but is not gitignored,
    /// so it would otherwise show up as untracked in the parent checkout.
    pub warning: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ScmError {
    #[error("not a git repository: {0}")]
    NotARepo(String),

    #[error("invalid branch name: {0}")]
    InvalidBranch(String),

    #[error("destination already exists: {0}")]
    DestExists(String),

    #[error("git {0} failed: {1}")]
    Git(String, String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Resolve the workdir of the checkout that `path` belongs to.
///
/// Mirrors `ProjectManager::detect_repository`: `gix::discover` walks up from
/// `path` and, inside a linked worktree, resolves to that worktree's workdir
/// (not the main checkout) — which is what `current_branch`/`is_current` need.
pub fn discover_workdir(path: &Path) -> Result<PathBuf, ScmError> {
    let repo = gix::discover(path).map_err(|_| ScmError::NotARepo(path.display().to_string()))?;
    repo.workdir()
        .map(Path::to_path_buf)
        .ok_or_else(|| ScmError::NotARepo(format!("{} (bare repository)", path.display())))
}

/// Parse `git worktree list --porcelain` into `(branch short name, path)`
/// pairs. Records are blank-line separated; a record has a `worktree <path>`
/// line and either a `branch refs/heads/<name>` line or a `detached` line.
/// Detached / branchless worktrees are skipped (they map no branch).
pub fn parse_worktree_porcelain(porcelain: &str) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut cur_path: Option<PathBuf> = None;

    for line in porcelain.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            cur_path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            let short = rest.strip_prefix("refs/heads/").unwrap_or(rest);
            if let Some(path) = cur_path.clone() {
                out.push((short.to_string(), path));
            }
        } else if line.is_empty() {
            cur_path = None;
        }
    }
    out
}

/// The main checkout path: the first `worktree ` entry of the porcelain
/// listing (git always lists the main worktree first).
pub fn main_worktree_path(porcelain: &str) -> Option<PathBuf> {
    porcelain
        .lines()
        .find_map(|l| l.strip_prefix("worktree ").map(PathBuf::from))
}

/// Turn a `refs/remotes` short name (e.g. `origin/feat/x`) into the branch
/// name (`feat/x`) by stripping the leading `<remote>/` segment. Returns
/// `None` for the symbolic `<remote>/HEAD` entries, which are not branches.
pub fn strip_remote_prefix(short: &str) -> Option<String> {
    let (_remote, branch) = short.split_once('/')?;
    if branch == "HEAD" || branch.is_empty() {
        return None;
    }
    Some(branch.to_string())
}

/// Extra branch-name rejections layered on top of `git check-ref-format`.
///
/// These are checked *before* the name is passed to git so a name like
/// `-foo` can never be interpreted as a flag by the `check-ref-format` call
/// itself, and so obviously-hostile path components (`..`, `\`, leading `/`)
/// are refused regardless of git's own rules.
pub fn validate_branch_name_extra(name: &str) -> Result<(), ScmError> {
    let reject = |why: &str| Err(ScmError::InvalidBranch(format!("{name}: {why}")));
    if name.is_empty() {
        return reject("empty");
    }
    if name.contains("..") {
        return reject("contains '..'");
    }
    if name.starts_with('-') {
        return reject("starts with '-'");
    }
    if name.starts_with('/') {
        return reject("starts with '/'");
    }
    if name.contains('\\') {
        return reject("contains '\\'");
    }
    Ok(())
}

/// Resolve where a new worktree should be created from the configured
/// template (default `{repo}/tree/{branch}`), substituting `{repo}` with the
/// repo root and `{branch}` with the branch name.
pub fn resolve_worktree_dest(template: Option<&str>, repo: &Path, branch: &str) -> PathBuf {
    let template = template.unwrap_or("{repo}/tree/{branch}");
    let resolved = template
        .replace("{repo}", &repo.to_string_lossy())
        .replace("{branch}", branch);
    PathBuf::from(resolved)
}

/// Order branches: current first, then branches with a worktree, then the
/// rest — alphabetical within each group.
pub fn sort_branches(branches: &mut [ScmBranch]) {
    branches.sort_by(|a, b| {
        let rank = |x: &ScmBranch| {
            if x.is_current {
                0
            } else if x.worktree_path.is_some() {
                1
            } else {
                2
            }
        };
        rank(a).cmp(&rank(b)).then_with(|| a.name.cmp(&b.name))
    });
}

/// Run `git <args>` and return trimmed stdout, mapping a nonzero exit to a
/// [`ScmError::Git`] carrying stderr. Arg vector only — never a shell string.
async fn git_stdout(args: &[&str]) -> Result<String, ScmError> {
    let output = Command::new("git").args(args).output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(ScmError::Git(args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Build the branch list for the checkout containing `path`.
pub async fn collect_branches(path: &Path) -> Result<ScmBranchesResponse, ScmError> {
    let workdir = discover_workdir(path)?;
    let workdir_str = workdir.to_string_lossy().to_string();

    let porcelain = git_stdout(&["-C", &workdir_str, "worktree", "list", "--porcelain"]).await?;
    let repo_root = main_worktree_path(&porcelain)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| workdir_str.clone());

    // branch short name -> worktree path
    let worktree_map: std::collections::HashMap<String, String> =
        parse_worktree_porcelain(&porcelain)
            .into_iter()
            .map(|(name, path)| (name, path.to_string_lossy().to_string()))
            .collect();

    // Current branch of the checkout containing `path` ("HEAD" = detached).
    let head = git_stdout(&["-C", &workdir_str, "rev-parse", "--abbrev-ref", "HEAD"]).await?;
    let current_branch = if head == "HEAD" || head.is_empty() {
        None
    } else {
        Some(head)
    };

    let locals = git_stdout(&[
        "-C",
        &workdir_str,
        "for-each-ref",
        "refs/heads",
        "--format=%(refname:short)",
    ])
    .await?;
    let local_set: std::collections::BTreeSet<String> = locals
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();

    let remotes = git_stdout(&[
        "-C",
        &workdir_str,
        "for-each-ref",
        "refs/remotes",
        "--format=%(refname:short)",
    ])
    .await?;
    let remote_only: std::collections::BTreeSet<String> = remotes
        .lines()
        .filter_map(strip_remote_prefix)
        .filter(|name| !local_set.contains(name))
        .collect();

    let mut branches: Vec<ScmBranch> = Vec::new();
    for name in &local_set {
        branches.push(ScmBranch {
            worktree_path: worktree_map.get(name).cloned(),
            is_current: current_branch.as_deref() == Some(name.as_str()),
            remote_only: false,
            name: name.clone(),
        });
    }
    for name in &remote_only {
        branches.push(ScmBranch {
            name: name.clone(),
            worktree_path: None,
            is_current: false,
            remote_only: true,
        });
    }

    sort_branches(&mut branches);

    Ok(ScmBranchesResponse {
        repo_root,
        current_branch,
        branches,
    })
}

/// Validate a branch name with both the extra rules and `git check-ref-format`.
pub async fn validate_branch_name(name: &str) -> Result<(), ScmError> {
    validate_branch_name_extra(name)?;
    // `--branch` validates a branch (not a full ref) name. Arg vector only,
    // and the leading-'-' case is already rejected above so `name` can't be
    // read as an option here.
    git_stdout(&["check-ref-format", "--branch", name])
        .await
        .map_err(|_| {
            ScmError::InvalidBranch(format!("{name}: rejected by git check-ref-format"))
        })?;
    Ok(())
}

/// Result of creating a worktree, before it is registered as a project.
pub struct WorktreeAdd {
    pub dest: PathBuf,
    /// True when `dest` is inside `repo_root` but not gitignored.
    pub not_ignored_warning: bool,
}

/// Create a worktree for `branch` under the resolved destination.
///
/// Never stashes and never passes `-f`. When `create_branch` is true a new
/// local branch is created (`worktree add -b <branch> <dest>`); otherwise the
/// branch name is passed positionally and git's DWIM creates a local tracking
/// branch for a remote-only name.
pub async fn add_worktree(
    repo_root: &Path,
    branch: &str,
    create_branch: bool,
    template: Option<&str>,
) -> Result<WorktreeAdd, ScmError> {
    validate_branch_name(branch).await?;

    let dest = resolve_worktree_dest(template, repo_root, branch);
    if dest.exists() {
        return Err(ScmError::DestExists(dest.to_string_lossy().to_string()));
    }

    let repo_str = repo_root.to_string_lossy().to_string();
    let dest_str = dest.to_string_lossy().to_string();

    let mut args: Vec<&str> = vec!["-C", &repo_str, "worktree", "add"];
    if create_branch {
        args.push("-b");
        args.push(branch);
        args.push(&dest_str);
    } else {
        args.push(&dest_str);
        args.push(branch);
    }
    git_stdout(&args).await?;

    // Warn when the new worktree sits inside the repo but is not ignored: it
    // would otherwise appear as untracked. `check-ignore -q` exits 0 when the
    // path is ignored, 1 when it is not — we only warn on "not ignored".
    let not_ignored_warning = if dest.starts_with(repo_root) {
        let ignored = Command::new("git")
            .args(["-C", &repo_str, "check-ignore", "-q", &dest_str])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        !ignored
    } else {
        false
    };

    Ok(WorktreeAdd {
        dest,
        not_ignored_warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worktree_porcelain_with_branches_and_detached() {
        let porcelain = "\
worktree /repo/main
HEAD 1111111111111111111111111111111111111111
branch refs/heads/master

worktree /repo/tree/feat/x
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feat/x

worktree /repo/tree/detached
HEAD 3333333333333333333333333333333333333333
detached
";
        let map = parse_worktree_porcelain(porcelain);
        assert_eq!(
            map,
            vec![
                ("master".to_string(), PathBuf::from("/repo/main")),
                ("feat/x".to_string(), PathBuf::from("/repo/tree/feat/x")),
            ]
        );
    }

    #[test]
    fn main_worktree_is_first_entry() {
        let porcelain = "worktree /repo/main\nbranch refs/heads/master\n\nworktree /repo/tree/x\n";
        assert_eq!(
            main_worktree_path(porcelain),
            Some(PathBuf::from("/repo/main"))
        );
    }

    #[test]
    fn strips_remote_prefix_and_drops_head() {
        assert_eq!(
            strip_remote_prefix("origin/master"),
            Some("master".to_string())
        );
        assert_eq!(
            strip_remote_prefix("origin/feat/x"),
            Some("feat/x".to_string())
        );
        assert_eq!(strip_remote_prefix("origin/HEAD"), None);
        assert_eq!(strip_remote_prefix("noremote"), None);
    }

    #[test]
    fn rejects_hostile_branch_names() {
        assert!(validate_branch_name_extra("../evil").is_err());
        assert!(validate_branch_name_extra("a..b").is_err());
        assert!(validate_branch_name_extra("-rf").is_err());
        assert!(validate_branch_name_extra("/abs").is_err());
        assert!(validate_branch_name_extra("back\\slash").is_err());
        assert!(validate_branch_name_extra("").is_err());
    }

    #[test]
    fn accepts_normal_branch_names() {
        assert!(validate_branch_name_extra("master").is_ok());
        assert!(validate_branch_name_extra("feat/x").is_ok());
        assert!(validate_branch_name_extra("refactor/subagents").is_ok());
    }

    #[test]
    fn resolves_default_and_custom_templates() {
        let repo = Path::new("/home/u/repo");
        assert_eq!(
            resolve_worktree_dest(None, repo, "feat/x"),
            PathBuf::from("/home/u/repo/tree/feat/x")
        );
        assert_eq!(
            resolve_worktree_dest(Some("{repo}/../wt/{branch}"), repo, "bug/y"),
            PathBuf::from("/home/u/repo/../wt/bug/y")
        );
        assert_eq!(
            resolve_worktree_dest(Some("/wt/{branch}"), repo, "z"),
            PathBuf::from("/wt/z")
        );
    }

    fn git(args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    /// End-to-end against a real repo: `collect_branches` reports the current
    /// branch, worktree paths, and remote-only branches; `add_worktree`
    /// creates a linked worktree on disk.
    #[tokio::test]
    async fn collect_branches_and_add_worktree_against_real_git() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        let repo_s = repo.to_string_lossy().to_string();

        git(&["-C", &repo_s, "init", "-q", "-b", "master"]);
        std::fs::write(repo.join("README.md"), "hi").unwrap();
        git(&["-C", &repo_s, "add", "."]);
        git(&[
            "-C",
            &repo_s,
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "init",
        ]);
        git(&["-C", &repo_s, "branch", "feat/x"]);

        // Simulate a remote-only branch: a bare "remote" with a branch the
        // local repo lacks, fetched into refs/remotes.
        let remote = tmp.path().join("remote.git");
        let remote_s = remote.to_string_lossy().to_string();
        git(&["init", "-q", "--bare", &remote_s]);
        git(&["-C", &repo_s, "remote", "add", "origin", &remote_s]);
        git(&["-C", &repo_s, "push", "-q", "origin", "master"]);
        git(&["-C", &repo_s, "push", "-q", "origin", "feat/x:only-remote"]);
        git(&["-C", &repo_s, "fetch", "-q", "origin"]);

        let resp = collect_branches(&repo).await.unwrap();
        assert_eq!(
            resp.repo_root,
            repo.canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(resp.current_branch.as_deref(), Some("master"));

        let master = resp.branches.iter().find(|b| b.name == "master").unwrap();
        assert!(master.is_current);
        assert!(!master.remote_only);
        assert!(master.worktree_path.is_some());

        let only_remote = resp.branches.iter().find(|b| b.name == "only-remote");
        assert!(only_remote.is_some_and(|b| b.remote_only && b.worktree_path.is_none()));

        // Current branch sorts first.
        assert_eq!(resp.branches[0].name, "master");

        // add_worktree for the existing local branch creates the tree.
        let dest_template = format!("{}/wt/{{branch}}", tmp.path().to_string_lossy());
        let added = add_worktree(&repo, "feat/x", false, Some(&dest_template))
            .await
            .unwrap();
        assert!(added.dest.join("README.md").is_file());

        // The worktree is outside repo_root, so no "not ignored" warning.
        assert!(!added.not_ignored_warning);

        // Re-listing now shows feat/x with a worktree path.
        let resp2 = collect_branches(&repo).await.unwrap();
        let feat = resp2.branches.iter().find(|b| b.name == "feat/x").unwrap();
        assert!(feat.worktree_path.is_some());
    }

    #[tokio::test]
    async fn validate_branch_name_rejects_bad_and_accepts_good() {
        assert!(validate_branch_name("../evil").await.is_err());
        assert!(validate_branch_name("has space").await.is_err());
        assert!(validate_branch_name("feat/ok").await.is_ok());
    }

    #[test]
    fn sorts_current_then_worktrees_then_alpha() {
        let mut branches = vec![
            ScmBranch {
                name: "zeta".into(),
                worktree_path: None,
                is_current: false,
                remote_only: false,
            },
            ScmBranch {
                name: "alpha".into(),
                worktree_path: None,
                is_current: false,
                remote_only: true,
            },
            ScmBranch {
                name: "has-wt".into(),
                worktree_path: Some("/wt/has".into()),
                is_current: false,
                remote_only: false,
            },
            ScmBranch {
                name: "current".into(),
                worktree_path: Some("/repo".into()),
                is_current: true,
                remote_only: false,
            },
        ];
        sort_branches(&mut branches);
        let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["current", "has-wt", "alpha", "zeta"]);
    }
}
