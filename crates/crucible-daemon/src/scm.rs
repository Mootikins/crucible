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

/// Response for the `scm.clone` RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScmCloneResponse {
    /// Absolute path of the freshly cloned repository.
    pub path: String,
    /// The `Project` registered for the clone.
    pub project: crucible_core::Project,
}

#[derive(Debug, thiserror::Error)]
pub enum ScmError {
    #[error("not a git repository: {0}")]
    NotARepo(String),

    #[error("invalid branch name: {0}")]
    InvalidBranch(String),

    #[error("invalid repository url: {0}")]
    InvalidUrl(String),

    #[error("destination already exists: {0}")]
    DestExists(String),

    #[error("invalid destination: {0}")]
    InvalidDest(String),

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

// ── scm.clone helpers ────────────────────────────────────────────────────

/// Validate and normalize a remote repository URL *before* git ever sees it.
///
/// Accepted forms:
/// - `https://…` / `http://…` — returned as-is.
/// - scp-like ssh (`git@host:owner/repo(.git)`) — returned as-is.
/// - shorthand `owner/repo` (exactly one `/`, no scheme) — rewritten to
///   `https://github.com/owner/repo.git`.
///
/// Everything else is rejected: a leading `-` (could be read as a git flag),
/// any whitespace, `file://` and bare local paths (this endpoint is for
/// *remote* repos — local dirs go through `project.register`).
pub fn normalize_clone_url(url: &str) -> Result<String, ScmError> {
    let reject = |why: &str| Err(ScmError::InvalidUrl(format!("{url}: {why}")));

    if url.is_empty() {
        return reject("empty");
    }
    if url.chars().any(char::is_whitespace) {
        return reject("contains whitespace");
    }
    if url.starts_with('-') {
        return reject("starts with '-'");
    }
    if url.starts_with("https://") || url.starts_with("http://") {
        return Ok(url.to_string());
    }
    if url.starts_with("file://") {
        return reject("file:// urls are not remote repositories");
    }
    // scp-like ssh: `user@host:path`, with the `@host` part appearing before
    // the first `:` and no URL scheme (`://`) anywhere.
    if !url.contains("://") {
        if let Some((before, after)) = url.split_once(':') {
            if before.contains('@') && !before.contains('/') && !after.is_empty() {
                return Ok(url.to_string());
            }
        }
    }
    // Bare local paths and unknown schemes.
    if url.starts_with('/') || url.starts_with('.') || url.starts_with('~') {
        return reject("looks like a local path");
    }
    if url.contains("://") {
        return reject("unsupported url scheme");
    }
    // Shorthand `owner/repo` → GitHub https.
    if let Some((owner, repo)) = url.split_once('/') {
        if !owner.is_empty()
            && !repo.is_empty()
            && !repo.contains('/')
            && !owner.starts_with(['-', '.'])
            && !repo.starts_with(['-', '.'])
        {
            return Ok(format!("https://github.com/{owner}/{repo}.git"));
        }
    }
    reject("not a recognized remote repository url")
}

/// Reject a repository directory name that would escape the projects dir or
/// name a git flag. Applied to both the caller-supplied `name` and the name
/// derived from a URL.
pub fn sanitize_repo_name(name: &str) -> Result<String, ScmError> {
    let reject = |why: &str| Err(ScmError::InvalidUrl(format!("repo name {name:?}: {why}")));
    if name.is_empty() {
        return reject("empty");
    }
    if name == "." || name == ".." {
        return reject("'.' / '..' are not valid names");
    }
    if name.contains('/') {
        return reject("contains '/'");
    }
    if name.starts_with('-') {
        return reject("starts with '-'");
    }
    if name.starts_with('.') {
        return reject("starts with '.'");
    }
    Ok(name.to_string())
}

/// Derive the on-disk repo name from a (normalized) URL: the last path segment
/// with a trailing `.git` stripped, then run through [`sanitize_repo_name`].
pub fn derive_repo_name(url: &str) -> Result<String, ScmError> {
    let trimmed = url.trim_end_matches('/');
    let base = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    // scp-like urls separate the path with ':' as well as '/'.
    let last = base.rsplit(['/', ':']).next().unwrap_or("");
    sanitize_repo_name(last)
}

/// Resolve the configured `[workspace] root_dir` (default `~/Projects`),
/// expanding a leading `~/` using `home`. `home = None` leaves a `~/` prefix
/// unexpanded (only reachable when the OS reports no home dir).
pub fn resolve_workspace_root_dir(configured: Option<&str>, home: Option<&Path>) -> PathBuf {
    let raw = configured.unwrap_or("~/Projects");
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(rest);
        }
    } else if raw == "~" {
        if let Some(home) = home {
            return home.to_path_buf();
        }
    }
    PathBuf::from(raw)
}

/// Contain an EXPLICIT clone destination inside `base` (the resolved
/// workspace root dir). Every other daemon write endpoint enforces allowlist
/// containment; without this, an authenticated web client could point
/// `scm.clone` at any writable absolute path (`~/.config/autostart`,
/// systemd user units, …) and materialize an attacker-controlled tree
/// there. Rejects `..` components lexically, then canonicalizes the
/// deepest EXISTING ancestor so a symlink inside `base` can't hop out.
/// `base` must exist (create it first).
pub fn validate_clone_dest(dest: &Path, base: &Path) -> Result<(), ScmError> {
    use std::path::Component;
    if dest.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(ScmError::InvalidDest(
            "dest must not contain '..'".to_string(),
        ));
    }
    let canon_base = base
        .canonicalize()
        .map_err(|e| ScmError::InvalidDest(format!("workspace root dir unavailable: {e}")))?;

    let mut ancestor = dest.parent();
    while let Some(a) = ancestor {
        if a.exists() {
            let canon = a
                .canonicalize()
                .map_err(|e| ScmError::InvalidDest(format!("cannot resolve dest: {e}")))?;
            let rest = dest.strip_prefix(a).expect("ancestor is a prefix of dest");
            if canon.join(rest).starts_with(&canon_base) {
                return Ok(());
            }
            return Err(ScmError::InvalidDest(format!(
                "dest must be inside the workspace root dir {}",
                canon_base.display()
            )));
        }
        ancestor = a.parent();
    }
    Err(ScmError::InvalidDest(
        "dest has no existing ancestor".to_string(),
    ))
}

/// Resolve the base directory under which per-session SCRATCH workspaces
/// (`<base>/<session_id>`) are created for sessions started without an explicit
/// workspace. Reads `[workspace] session_scratch_dir`.
///
/// A `configured` value wins and has a leading `~/` expanded using `home`
/// (`home = None` leaves a `~/` prefix unexpanded, only reachable when the OS
/// reports no home dir). When unset, the default is `<default_base>/workspaces`
/// — seeded from the daemon's data root, which is `~/.crucible` in production
/// (so the default path is `~/.crucible/workspaces`) but an injected temp dir
/// under test.
pub fn resolve_session_scratch_dir(
    configured: Option<&str>,
    home: Option<&Path>,
    default_base: &Path,
) -> PathBuf {
    let Some(raw) = configured else {
        return default_base.join("workspaces");
    };
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = home {
            return home.join(rest);
        }
    } else if raw == "~" {
        if let Some(home) = home {
            return home.to_path_buf();
        }
    }
    PathBuf::from(raw)
}

/// Run `git clone -- <url> <dest>` with an argument vector (never a shell
/// string). The `--` separator guarantees `url` and `dest` can't be read as
/// flags. No timeout and no credential handling — cloning uses git's ambient
/// auth (ssh-agent / credential helper). On failure the error carries the last
/// ~10 lines of git's stderr.
pub async fn clone_repo(url: &str, dest: &Path) -> Result<(), ScmError> {
    if dest.exists() {
        return Err(ScmError::DestExists(dest.to_string_lossy().to_string()));
    }
    let dest_str = dest.to_string_lossy().to_string();
    let output = Command::new("git")
        .args(["clone", "--", url, &dest_str])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr
            .lines()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(ScmError::Git("clone".to_string(), tail.trim().to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Run git in a fixture repo, failing the test rather than the assertion
    /// if git itself errors.
    fn git(args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    }

    use super::*;

    #[test]
    fn normalizes_https_and_ssh_urls_verbatim() {
        assert_eq!(
            normalize_clone_url("https://github.com/o/r.git").unwrap(),
            "https://github.com/o/r.git"
        );
        assert_eq!(
            normalize_clone_url("http://example.com/o/r").unwrap(),
            "http://example.com/o/r"
        );
        assert_eq!(
            normalize_clone_url("git@github.com:o/r.git").unwrap(),
            "git@github.com:o/r.git"
        );
    }

    #[test]
    fn expands_shorthand_to_github_https() {
        assert_eq!(
            normalize_clone_url("owner/repo").unwrap(),
            "https://github.com/owner/repo.git"
        );
    }

    #[test]
    fn rejects_hostile_clone_urls() {
        assert!(normalize_clone_url("").is_err());
        assert!(normalize_clone_url("-oProxyCommand=evil").is_err());
        assert!(normalize_clone_url("has space/repo").is_err());
        assert!(normalize_clone_url("file:///etc/passwd").is_err());
        assert!(normalize_clone_url("/home/user/repo").is_err());
        assert!(normalize_clone_url("./local/repo").is_err());
        assert!(normalize_clone_url("~/repo").is_err());
        assert!(normalize_clone_url("owner/repo/extra").is_err());
        assert!(normalize_clone_url("ssh://host/repo").is_err());
    }

    #[test]
    fn derives_and_sanitizes_repo_name() {
        assert_eq!(
            derive_repo_name("https://github.com/owner/repo.git").unwrap(),
            "repo"
        );
        assert_eq!(
            derive_repo_name("https://github.com/owner/repo").unwrap(),
            "repo"
        );
        assert_eq!(
            derive_repo_name("git@github.com:owner/repo.git").unwrap(),
            "repo"
        );
        assert_eq!(derive_repo_name("https://host/o/repo/").unwrap(), "repo");

        assert!(sanitize_repo_name("").is_err());
        assert!(sanitize_repo_name(".").is_err());
        assert!(sanitize_repo_name("..").is_err());
        assert!(sanitize_repo_name("a/b").is_err());
        assert!(sanitize_repo_name("-rf").is_err());
        assert!(sanitize_repo_name(".hidden").is_err());
        assert_eq!(sanitize_repo_name("my-repo").unwrap(), "my-repo");
    }

    #[test]
    fn clone_dest_contained_to_workspace_root_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("Projects");
        std::fs::create_dir_all(&base).unwrap();

        // Inside base: ok (including a not-yet-existing nested path).
        assert!(validate_clone_dest(&base.join("repo"), &base).is_ok());
        assert!(validate_clone_dest(&base.join("org/repo"), &base).is_ok());

        // Outside base: rejected.
        assert!(matches!(
            validate_clone_dest(&tmp.path().join("elsewhere"), &base),
            Err(ScmError::InvalidDest(_))
        ));
        assert!(matches!(
            validate_clone_dest(Path::new("/etc/cron.d/evil"), &base),
            Err(ScmError::InvalidDest(_))
        ));

        // Lexical escape: rejected before any FS access.
        assert!(matches!(
            validate_clone_dest(&base.join("../escape"), &base),
            Err(ScmError::InvalidDest(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn clone_dest_rejects_symlink_hop_out_of_base() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("Projects");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("link")).unwrap();

        // base/link/repo lexically starts with base but resolves outside it.
        assert!(matches!(
            validate_clone_dest(&base.join("link/repo"), &base),
            Err(ScmError::InvalidDest(_))
        ));
    }

    #[test]
    fn resolves_workspace_root_dir_tilde_and_default() {
        let home = Path::new("/home/u");
        assert_eq!(
            resolve_workspace_root_dir(None, Some(home)),
            PathBuf::from("/home/u/Projects")
        );
        assert_eq!(
            resolve_workspace_root_dir(Some("~/code"), Some(home)),
            PathBuf::from("/home/u/code")
        );
        assert_eq!(
            resolve_workspace_root_dir(Some("/srv/repos"), Some(home)),
            PathBuf::from("/srv/repos")
        );
        assert_eq!(
            resolve_workspace_root_dir(Some("~"), Some(home)),
            PathBuf::from("/home/u")
        );
    }

    #[test]
    fn resolves_session_scratch_dir_tilde_and_default() {
        let home = Path::new("/home/u");
        let data_home = Path::new("/home/u/.crucible");
        // Unset → <data_home>/workspaces (== ~/.crucible/workspaces in prod).
        assert_eq!(
            resolve_session_scratch_dir(None, Some(home), data_home),
            PathBuf::from("/home/u/.crucible/workspaces")
        );
        // An injected data root (tests) keeps the default inside it.
        assert_eq!(
            resolve_session_scratch_dir(None, Some(home), Path::new("/tmp/xyz")),
            PathBuf::from("/tmp/xyz/workspaces")
        );
        assert_eq!(
            resolve_session_scratch_dir(Some("~/scratch"), Some(home), data_home),
            PathBuf::from("/home/u/scratch")
        );
        assert_eq!(
            resolve_session_scratch_dir(Some("/var/scratch"), Some(home), data_home),
            PathBuf::from("/var/scratch")
        );
        assert_eq!(
            resolve_session_scratch_dir(Some("~"), Some(home), data_home),
            PathBuf::from("/home/u")
        );
    }

    /// End-to-end clone against a real local fixture repo. `clone_repo` is
    /// exercised directly with a `file://`-free local path — the URL-validation
    /// layer (which rejects local paths) is bypassed on purpose, matching how
    /// the RPC handler splits validation from execution.
    #[tokio::test]
    async fn clone_repo_against_real_git_fixture() {
        let tmp = tempfile::TempDir::new().unwrap();

        // Build a source repo with one commit.
        let src = tmp.path().join("src");
        std::fs::create_dir(&src).unwrap();
        let src_s = src.to_string_lossy().to_string();
        git(&["-C", &src_s, "init", "-q", "-b", "master"]);
        std::fs::write(src.join("README.md"), "hello").unwrap();
        git(&["-C", &src_s, "add", "."]);
        git(&[
            "-C",
            &src_s,
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "init",
        ]);

        // Clone it to a fresh destination that does not yet exist.
        let dest = tmp.path().join("clone");
        clone_repo(&src_s, &dest).await.unwrap();
        assert!(dest.join("README.md").is_file());
        assert!(dest.join(".git").is_dir());

        // Cloning onto an existing path is refused.
        assert!(matches!(
            clone_repo(&src_s, &dest).await,
            Err(ScmError::DestExists(_))
        ));

        // A bogus source surfaces git's stderr.
        let missing = tmp.path().join("nope");
        let out = tmp.path().join("out");
        assert!(matches!(
            clone_repo(&missing.to_string_lossy(), &out).await,
            Err(ScmError::Git(_, _))
        ));
    }
}
