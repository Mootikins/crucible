//! Read-only, one-level filesystem listing for the web file-tree explorer.
//!
//! Backs the `fs.list_dir` RPC. Lazily enumerates a single directory level
//! inside a **registered project**, returning metadata only (never file bytes)
//! and never mutating the filesystem.
//!
//! # Threat model (see plan §3e)
//!
//! The web server binds loopback with cookie-session auth, but this repo has a
//! prior loopback-auth-bypass → web-terminal-RCE history, so every check here is
//! daemon-side and load-bearing — never trust the thin web layer. Controls:
//!
//! 1. **Registry allowlist** — the only listable roots are directories the user
//!    registered as projects (`ProjectManager::get`, fail-closed). An unknown
//!    root is rejected before any disk access.
//! 2. **`rel_path` component whitelist** — `..`, absolute paths, Windows
//!    prefixes, and NUL are rejected *before* touching the disk (`resolve_within`).
//! 3. **Canonicalize-and-contain** on the resolved target dir — blocks
//!    directory-symlink escapes and TOCTOU (the resolved path must
//!    `starts_with` the canonical project root).
//! 4. **Per-entry symlink containment** — an entry whose symlink resolves
//!    outside the project root is silently dropped, never listed or followed
//!    (`follow_links(false)` + canonicalize check in `walk_one_level`).
//! 5. **Read-only, metadata-only** — no file contents, no mutation.
//!
//! # Dotfile policy
//!
//! With `show_ignored == false` (the default) ALL dotfiles and dot-dirs are
//! HIDDEN — not just gitignored ones — so a non-gitignored secret file (`.env`,
//! `.netrc`, `.envrc`) is NOT enumerable by default. `show_ignored == true`
//! reveals dotfiles AND gitignored entries AND `.git` together. This
//! deliberately hides non-gitignored secrets by default given the web server's
//! loopback-auth history; the residual accepted risk is that an authenticated
//! same-origin caller can enumerate non-dotfile names/sizes within registered
//! projects (equivalent to the user's own shell access).
//!
//! Out of scope: a TOCTOU where the resolved in-root target dir is swapped for a
//! symlink to outside the root between the containment check and the walk. Winning
//! it requires local filesystem write access, which a remote (read-only) web
//! caller does not have — it is already inside the "shell access" accepted risk.

use crate::project_manager::ProjectManager;
use crate::protocol::{Request, Response, INTERNAL_ERROR, INVALID_PARAMS};
use crate::rpc_helpers::{optional_param, require_param};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// One directory entry in an `fs.list_dir` response.
///
/// Wire keys (`name`/`rel_path`/`is_dir`/`size`/`modified`/`status`) are
/// byte-identical to the TypeScript `FsEntry`. `status` is a Phase-1 decoration
/// seam and is always `None`.
#[derive(serde::Serialize)]
pub(crate) struct FsEntry {
    pub name: String,
    pub rel_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
    pub status: Option<serde_json::Value>,
}

#[derive(Debug, thiserror::Error)]
enum FsListError {
    #[error("root is not a registered project")]
    NotRegistered,
    #[error("path escapes project root")]
    Escape,
    #[error("not a directory")]
    NotADir,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Handle the `fs.list_dir` RPC. Read-only, metadata only.
pub(crate) async fn handle_fs_list_dir(req: Request, pm: &Arc<ProjectManager>) -> Response {
    let root = require_param!(req, "root", as_str);
    let rel_path = require_param!(req, "rel_path", as_str);
    let show_ignored = optional_param!(req, "show_ignored", as_bool).unwrap_or(false);

    match list_dir(pm, Path::new(root), rel_path, show_ignored) {
        Ok(entries) => match serde_json::to_value(entries) {
            Ok(v) => Response::success(req.id, v),
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        },
        Err(FsListError::NotRegistered) => {
            Response::error(req.id, INVALID_PARAMS, "root is not a registered project")
        }
        Err(FsListError::Escape) => {
            Response::error(req.id, INVALID_PARAMS, "path escapes project root")
        }
        Err(FsListError::NotADir) => Response::error(req.id, INVALID_PARAMS, "not a directory"),
        Err(FsListError::Io(e)) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
    }
}

fn list_dir(
    pm: &Arc<ProjectManager>,
    root: &Path,
    rel_path: &str,
    show_ignored: bool,
) -> Result<Vec<FsEntry>, FsListError> {
    // Fail-closed allowlist: only registered projects are listable.
    let project = pm.get(root).ok_or(FsListError::NotRegistered)?;
    let base = project.path.canonicalize()?;
    let target = resolve_within(&base, rel_path)?;
    if !target.is_dir() {
        return Err(FsListError::NotADir);
    }
    walk_one_level(&base, &target, show_ignored)
}

/// Resolve `rel_path` against `base` with a component whitelist and
/// canonicalize-and-contain. `rel_path == ""` resolves to `base`.
fn resolve_within(base: &Path, rel_path: &str) -> Result<PathBuf, FsListError> {
    let rel = Path::new(rel_path);
    if rel.is_absolute() || rel_path.contains('\0') {
        return Err(FsListError::Escape);
    }
    for c in rel.components() {
        // Only plain path segments: no `..`, `.`, root, or Windows prefix.
        if !matches!(c, Component::Normal(_)) {
            return Err(FsListError::Escape);
        }
    }
    let canon = base
        .join(rel)
        .canonicalize()
        .map_err(|_| FsListError::Escape)?;
    if !canon.starts_with(base) {
        return Err(FsListError::Escape);
    }
    Ok(canon)
}

/// Enumerate exactly one level of `dir` (which is already contained in `base`),
/// hiding dotfiles/gitignored entries by default and dropping any entry whose
/// symlink resolves outside `base`. Dirs-first, then case-insensitive name.
fn walk_one_level(
    base: &Path,
    dir: &Path,
    show_ignored: bool,
) -> Result<Vec<FsEntry>, FsListError> {
    let mut out = Vec::new();
    let walker = ignore::WalkBuilder::new(dir)
        .max_depth(Some(1))
        .parents(true)
        .git_ignore(!show_ignored)
        .git_exclude(!show_ignored)
        .git_global(!show_ignored)
        // Honor .gitignore even when the project root is not a git repo (a project
        // may be an invocation dir, not a repo root). Default require_git(true) would
        // silently skip gitignore rules absent a .git dir.
        .require_git(false)
        .hidden(!show_ignored)
        .follow_links(false)
        .build();

    for dent in walker {
        let Ok(dent) = dent else { continue };
        if dent.depth() == 0 {
            continue; // skip `dir` itself
        }
        let path = dent.path();
        let name = dent.file_name().to_string_lossy().to_string();

        // symlink_metadata does NOT follow the link: detect symlinks regardless
        // of destination, then drop any that resolve outside the project root.
        let lmeta = std::fs::symlink_metadata(path)?;
        if lmeta.file_type().is_symlink() {
            match path.canonicalize() {
                Ok(resolved) if resolved.starts_with(base) => {} // intra-project: allowed
                _ => continue,                                   // escaping/broken: never listed
            }
        }

        // Follows symlinks (already contained) to report the real target's kind.
        let meta = std::fs::metadata(path).ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = if is_dir {
            0
        } else {
            meta.as_ref().map(|m| m.len()).unwrap_or(0)
        };
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let rel = path
            .strip_prefix(base)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        out.push(FsEntry {
            name,
            rel_path: rel,
            is_dir,
            size,
            modified,
            status: None,
        });
    }

    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Build a hermetic `ProjectManager` (temp `projects.json`, never the
    /// developer's real `~/.crucible`) with `project_dir` registered. Returns
    /// the manager and the canonical registered root.
    fn registered_pm(storage_root: &Path, project_dir: &Path) -> (Arc<ProjectManager>, PathBuf) {
        let pm = Arc::new(ProjectManager::new(storage_root.join("projects.json")));
        let project = pm.register(project_dir).expect("register project");
        (pm, project.path)
    }

    #[test]
    fn lists_nested_dirs_and_files_dirs_first() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::create_dir(proj.join("src")).unwrap();
        fs::create_dir(proj.join("assets")).unwrap();
        fs::write(proj.join("README.md"), "hi").unwrap();
        fs::write(proj.join("Cargo.toml"), "[package]").unwrap();
        fs::write(proj.join("src").join("main.rs"), "fn main() {}").unwrap();

        let (pm, root) = registered_pm(store.path(), proj);

        // Top level.
        let entries = list_dir(&pm, &root, "", false).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // Dirs first (case-insensitive name), then files (case-insensitive).
        assert_eq!(names, vec!["assets", "src", "Cargo.toml", "README.md"]);
        assert!(entries.iter().find(|e| e.name == "src").unwrap().is_dir);
        assert!(
            !entries
                .iter()
                .find(|e| e.name == "README.md")
                .unwrap()
                .is_dir
        );
        // status is always null in Phase 1.
        assert!(entries.iter().all(|e| e.status.is_none()));

        // One level down via rel_path.
        let sub = list_dir(&pm, &root, "src", false).unwrap();
        assert_eq!(sub.len(), 1);
        assert_eq!(sub[0].name, "main.rs");
        assert_eq!(sub[0].rel_path, "src/main.rs");
    }

    #[test]
    fn gitignored_file_hidden_by_default_shown_with_show_ignored() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::write(proj.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(proj.join("ignored.txt"), "secret").unwrap();
        fs::write(proj.join("kept.txt"), "ok").unwrap();

        let (pm, root) = registered_pm(store.path(), proj);

        let hidden = list_dir(&pm, &root, "", false).unwrap();
        let hidden_names: Vec<&str> = hidden.iter().map(|e| e.name.as_str()).collect();
        assert!(hidden_names.contains(&"kept.txt"));
        assert!(!hidden_names.contains(&"ignored.txt"));
        // `.gitignore` is itself a dotfile → also hidden by default.
        assert!(!hidden_names.contains(&".gitignore"));

        let shown = list_dir(&pm, &root, "", true).unwrap();
        let shown_names: Vec<&str> = shown.iter().map(|e| e.name.as_str()).collect();
        assert!(shown_names.contains(&"ignored.txt"));
        assert!(shown_names.contains(&".gitignore"));
    }

    #[test]
    fn dotfile_hidden_by_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        // Not gitignored — hidden purely because it is a dotfile.
        fs::write(proj.join(".env"), "SECRET=1").unwrap();
        fs::write(proj.join("visible.txt"), "ok").unwrap();

        let (pm, root) = registered_pm(store.path(), proj);

        let hidden = list_dir(&pm, &root, "", false).unwrap();
        assert!(hidden.iter().all(|e| e.name != ".env"));
        assert!(hidden.iter().any(|e| e.name == "visible.txt"));

        let shown = list_dir(&pm, &root, "", true).unwrap();
        assert!(shown.iter().any(|e| e.name == ".env"));
    }

    #[test]
    fn symlink_escaping_root_is_excluded() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "top secret").unwrap();
        fs::write(proj.join("inside.txt"), "ok").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(&secret, proj.join("escape.txt")).unwrap();

        let (pm, root) = registered_pm(store.path(), proj);
        let entries = list_dir(&pm, &root, "", false).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"inside.txt"));
        // The escaping symlink must never surface.
        assert!(!names.contains(&"escape.txt"));
    }

    #[test]
    fn intra_project_symlink_is_listed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::write(proj.join("target.txt"), "data").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(proj.join("target.txt"), proj.join("link.txt")).unwrap();

        let (pm, root) = registered_pm(store.path(), proj);
        let entries = list_dir(&pm, &root, "", false).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        #[cfg(unix)]
        assert!(names.contains(&"link.txt"));
        assert!(names.contains(&"target.txt"));
    }

    #[test]
    fn rejects_parent_traversal() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::create_dir(proj.join("src")).unwrap();
        let (pm, root) = registered_pm(store.path(), proj);

        assert!(matches!(
            list_dir(&pm, &root, "../", false),
            Err(FsListError::Escape)
        ));
        assert!(matches!(
            list_dir(&pm, &root, "src/../..", false),
            Err(FsListError::Escape)
        ));
    }

    #[test]
    fn rejects_absolute_rel_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        let (pm, root) = registered_pm(store.path(), proj);

        assert!(matches!(
            list_dir(&pm, &root, "/etc", false),
            Err(FsListError::Escape)
        ));
    }

    #[test]
    fn unregistered_root_is_rejected() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        // A ProjectManager with nothing registered.
        let pm = Arc::new(ProjectManager::new(store.path().join("projects.json")));
        assert!(matches!(
            list_dir(&pm, tmp.path(), "", false),
            Err(FsListError::NotRegistered)
        ));
    }

    #[test]
    fn not_a_directory_is_rejected() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = tempfile::TempDir::new().unwrap();
        let proj = tmp.path();
        fs::write(proj.join("file.txt"), "x").unwrap();
        let (pm, root) = registered_pm(store.path(), proj);

        assert!(matches!(
            list_dir(&pm, &root, "file.txt", false),
            Err(FsListError::NotADir)
        ));
    }
}
