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

use crate::kiln_manager::KilnManager;
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

// ── fs.move ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub(crate) enum FsMoveError {
    #[error("path escapes root")]
    Escape,
    #[error("source does not exist")]
    SourceMissing,
    #[error("destination already exists")]
    DestinationExists,
    #[error("destination parent is not a directory inside the root")]
    BadDestination,
    #[error("cannot move a directory into itself")]
    IntoSelf,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Handle the `fs.move` RPC — rename/move a file or directory *within* one
/// root. The web file-tree's drag-and-drop backend.
///
/// Same threat model as `fs.list_dir` (all checks daemon-side, fail-closed):
/// the only movable roots are registered projects (`kind == "project"`) or
/// **already-open** kilns (`kind == "kiln"`). Open-kilns-only is deliberate:
/// `KilnManager::open` will initialize `.crucible/` in ANY directory, so
/// `get_or_open` here would let a caller mint move-capability over arbitrary
/// paths. Both `from_rel` and `to_rel` get the component whitelist +
/// canonicalize-and-contain treatment on their PARENT dirs (never the leaf,
/// so a symlink moves as a link, not its target). Overwrites are rejected.
///
/// Kiln index consistency: the open kiln's watch pipeline observes the rename
/// and re-indexes; this handler only touches the filesystem.
pub(crate) async fn handle_fs_move(
    req: Request,
    pm: &Arc<ProjectManager>,
    km: &Arc<KilnManager>,
) -> Response {
    let root = require_param!(req, "root", as_str);
    let kind = require_param!(req, "kind", as_str);
    let from_rel = require_param!(req, "from_rel", as_str);
    let to_rel = require_param!(req, "to_rel", as_str);

    let base = match resolve_root(pm, km, kind, root).await {
        Ok(base) => base,
        Err(msg) => return Response::error(req.id, INVALID_PARAMS, msg),
    };

    // Kiln markdown notes route through the wikilink-aware rename: the move
    // AND the inbound-link rewrite/reindex happen as one operation, so a DnD
    // move in the web file tree can never silently break links. Directories
    // and non-markdown files keep the plain rename (folder-level bulk rewrite
    // is Phase 3.1; bare-stem links to children keep resolving via key
    // re-resolution regardless).
    if kind == "kiln"
        && from_rel.ends_with(".md")
        && to_rel.ends_with(".md")
        && base.join(from_rel).is_file()
    {
        return match crate::server::note_refactor::rename_note(km, &base, from_rel, to_rel).await {
            Ok(outcome) => Response::success(
                req.id,
                serde_json::json!({
                    "moved": true,
                    "rewritten_sources": outcome.rewritten_sources,
                    "skipped": outcome.skipped,
                }),
            ),
            Err(crate::server::note_refactor::RenameError::Move(e)) => {
                Response::error(req.id, INVALID_PARAMS, e.to_string())
            }
            Err(e) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        };
    }

    match move_within(&base, from_rel, to_rel) {
        Ok(()) => Response::success(req.id, serde_json::json!({ "moved": true })),
        Err(FsMoveError::Io(e)) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

/// Resolve the mutation root for `kind`: a registered project or an
/// ALREADY-OPEN kiln (fail-closed — see `handle_fs_move` docs). Returns the
/// canonical base, or the INVALID_PARAMS message for the caller to wrap.
async fn resolve_root(
    pm: &Arc<ProjectManager>,
    km: &Arc<KilnManager>,
    kind: &str,
    root: &str,
) -> Result<PathBuf, &'static str> {
    let base = match kind {
        "project" => pm.get(Path::new(root)).map(|p| p.path),
        "kiln" => match Path::new(root).canonicalize() {
            Ok(canon) if km.get(&canon).await.is_some() => Some(canon),
            _ => None,
        },
        _ => return Err("kind must be 'project' or 'kiln'"),
    };
    base.ok_or("root is not a registered project or open kiln")
}

/// Resolve `rel` to `canonical(parent) + leaf name`, containing the PARENT in
/// `base`. The leaf is deliberately not canonicalized: renaming a symlink must
/// move the link itself, and a destination leaf does not exist yet.
/// `missing_parent` is the error when the parent dir doesn't exist — the one
/// case that means different things for source (missing) vs destination (bad
/// target); real escapes always surface as `Escape`.
fn split_contained(
    base: &Path,
    rel: &str,
    missing_parent: fn() -> FsMoveError,
) -> Result<PathBuf, FsMoveError> {
    let rel_p = Path::new(rel);
    if rel_p.is_absolute() || rel.contains('\0') {
        return Err(FsMoveError::Escape);
    }
    for c in rel_p.components() {
        if !matches!(c, Component::Normal(_)) {
            return Err(FsMoveError::Escape);
        }
    }
    // file_name is None only for empty/`..`-ish paths — the root itself is
    // never a valid move source or destination.
    let name = rel_p.file_name().ok_or(FsMoveError::Escape)?;
    let parent_rel = rel_p.parent().unwrap_or(Path::new(""));
    let parent = base
        .join(parent_rel)
        .canonicalize()
        .map_err(|_| missing_parent())?;
    if !parent.starts_with(base) {
        return Err(FsMoveError::Escape);
    }
    if !parent.is_dir() {
        return Err(missing_parent());
    }
    Ok(parent.join(name))
}

pub(crate) fn move_within(base: &Path, from_rel: &str, to_rel: &str) -> Result<(), FsMoveError> {
    let from = split_contained(base, from_rel, || FsMoveError::SourceMissing)?;
    if from.symlink_metadata().is_err() {
        return Err(FsMoveError::SourceMissing);
    }
    let dest = split_contained(base, to_rel, || FsMoveError::BadDestination)?;
    if dest.symlink_metadata().is_ok() {
        return Err(FsMoveError::DestinationExists);
    }
    if dest.starts_with(&from) {
        return Err(FsMoveError::IntoSelf);
    }
    std::fs::rename(&from, &dest)?;
    Ok(())
}

// ── fs.mkdir / fs.trash ────────────────────────────────────────────────────

/// Handle the `fs.mkdir` RPC — create a folder (and missing parents) inside a
/// root. Same fail-closed allowlist as `fs.move`; components are whitelisted
/// and the deepest EXISTING ancestor must canonicalize inside the root (so a
/// symlinked prefix can never escape).
pub(crate) async fn handle_fs_mkdir(
    req: Request,
    pm: &Arc<ProjectManager>,
    km: &Arc<KilnManager>,
) -> Response {
    let root = require_param!(req, "root", as_str);
    let kind = require_param!(req, "kind", as_str);
    let rel_path = require_param!(req, "rel_path", as_str);

    let base = match resolve_root(pm, km, kind, root).await {
        Ok(base) => base,
        Err(msg) => return Response::error(req.id, INVALID_PARAMS, msg),
    };

    match mkdir_within(&base, rel_path) {
        Ok(()) => Response::success(req.id, serde_json::json!({ "created": true })),
        Err(FsMoveError::Io(e)) => Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        Err(e) => Response::error(req.id, INVALID_PARAMS, e.to_string()),
    }
}

fn mkdir_within(base: &Path, rel_path: &str) -> Result<(), FsMoveError> {
    let rel = Path::new(rel_path);
    if rel.is_absolute() || rel_path.is_empty() || rel_path.contains('\0') {
        return Err(FsMoveError::Escape);
    }
    for c in rel.components() {
        if !matches!(c, Component::Normal(_)) {
            return Err(FsMoveError::Escape);
        }
    }
    let target = base.join(rel);
    if target.symlink_metadata().is_ok() {
        return Err(FsMoveError::DestinationExists);
    }
    // Contain the deepest existing ancestor (whitelisted components alone
    // don't stop an existing symlinked prefix from pointing outside).
    let mut ancestor = target.parent().unwrap_or(base).to_path_buf();
    while ancestor.symlink_metadata().is_err() {
        match ancestor.parent() {
            Some(p) => ancestor = p.to_path_buf(),
            None => return Err(FsMoveError::Escape),
        }
    }
    let canon = ancestor.canonicalize().map_err(|_| FsMoveError::Escape)?;
    if !canon.starts_with(base) {
        return Err(FsMoveError::Escape);
    }
    std::fs::create_dir_all(&target)?;
    Ok(())
}

/// Handle the `fs.trash` RPC — move a file or directory into the root's
/// `.crucible/trash/` (timestamped, never overwrites). `.crucible` is in
/// `EXCLUDED_DIRS`, so trashed notes leave the watcher/discovery universe;
/// kiln `.md` notes (including a trashed directory's children) are dropped
/// from the index inline so backlinks re-resolve immediately.
pub(crate) async fn handle_fs_trash(
    req: Request,
    pm: &Arc<ProjectManager>,
    km: &Arc<KilnManager>,
) -> Response {
    let root = require_param!(req, "root", as_str);
    let kind = require_param!(req, "kind", as_str);
    let rel_path = require_param!(req, "rel_path", as_str);

    let base = match resolve_root(pm, km, kind, root).await {
        Ok(base) => base,
        Err(msg) => return Response::error(req.id, INVALID_PARAMS, msg),
    };

    // Collect the kiln notes this trash will remove BEFORE moving anything.
    let source = match split_contained(&base, rel_path, || FsMoveError::SourceMissing) {
        Ok(p) => p,
        Err(e) => return Response::error(req.id, INVALID_PARAMS, e.to_string()),
    };
    let mut removed_notes: Vec<PathBuf> = Vec::new();
    if kind == "kiln" {
        collect_md_files(&source, &mut removed_notes);
    }

    let trash_rel = match trash_within(&base, rel_path) {
        Ok(rel) => rel,
        Err(FsMoveError::Io(e)) => return Response::error(req.id, INTERNAL_ERROR, e.to_string()),
        Err(e) => return Response::error(req.id, INVALID_PARAMS, e.to_string()),
    };

    for note in &removed_notes {
        if let Err(e) = km.handle_file_deleted(&base, note).await {
            tracing::warn!(path = %note.display(), error = %e, "trash: index cleanup failed");
        }
    }

    Response::success(
        req.id,
        serde_json::json!({ "trashed": true, "trash_path": trash_rel }),
    )
}

/// `.md` files at or under `path` (the pre-move index-cleanup set).
fn collect_md_files(path: &Path, out: &mut Vec<PathBuf>) {
    let meta = match path.symlink_metadata() {
        Ok(m) => m,
        Err(_) => return,
    };
    if meta.is_file() {
        if path.extension().is_some_and(|e| e == "md") {
            out.push(path.to_path_buf());
        }
        return;
    }
    if meta.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                collect_md_files(&entry.path(), out);
            }
        }
    }
}

/// Move `rel_path` to `.crucible/trash/<unix-secs>-<name>` inside `base`.
/// Returns the trash-relative destination.
fn trash_within(base: &Path, rel_path: &str) -> Result<String, FsMoveError> {
    let source = split_contained(base, rel_path, || FsMoveError::SourceMissing)?;
    if source.symlink_metadata().is_err() {
        return Err(FsMoveError::SourceMissing);
    }
    // Never trash the trash (or anything already under .crucible).
    if Path::new(rel_path)
        .components()
        .next()
        .is_some_and(|c| c.as_os_str() == ".crucible")
    {
        return Err(FsMoveError::Escape);
    }

    let trash_dir = base.join(".crucible").join("trash");
    std::fs::create_dir_all(&trash_dir)?;

    let name = Path::new(rel_path)
        .file_name()
        .ok_or(FsMoveError::Escape)?
        .to_string_lossy()
        .to_string();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut dest = trash_dir.join(format!("{stamp}-{name}"));
    let mut n = 1u32;
    while dest.symlink_metadata().is_ok() {
        dest = trash_dir.join(format!("{stamp}-{n}-{name}"));
        n += 1;
    }
    std::fs::rename(&source, &dest)?;
    Ok(format!(
        ".crucible/trash/{}",
        dest.file_name().unwrap_or_default().to_string_lossy()
    ))
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

    // ── move_within ────────────────────────────────────────────────────

    /// Canonicalized tempdir root (macOS /var → /private/var etc.); the
    /// handler always passes a canonical base, so tests must too.
    fn canon_root(tmp: &tempfile::TempDir) -> PathBuf {
        tmp.path().canonicalize().unwrap()
    }

    #[test]
    fn move_file_into_subdir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::create_dir(base.join("notes")).unwrap();
        fs::write(base.join("a.md"), "x").unwrap();

        move_within(&base, "a.md", "notes/a.md").unwrap();
        assert!(!base.join("a.md").exists());
        assert_eq!(fs::read_to_string(base.join("notes/a.md")).unwrap(), "x");
    }

    #[test]
    fn move_renames_a_directory_with_contents() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::create_dir_all(base.join("src/deep")).unwrap();
        fs::write(base.join("src/deep/f.rs"), "fn").unwrap();
        fs::create_dir(base.join("lib")).unwrap();

        move_within(&base, "src", "lib/src").unwrap();
        assert!(base.join("lib/src/deep/f.rs").exists());
        assert!(!base.join("src").exists());
    }

    #[test]
    fn move_rejects_escapes_in_either_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::write(base.join("a.md"), "x").unwrap();

        for (from, to) in [
            ("../a.md", "b.md"),
            ("a.md", "../b.md"),
            ("/etc/passwd", "b.md"),
            ("a.md", "/tmp/b.md"),
            ("", "b.md"),
            ("a.md", ""),
        ] {
            assert!(
                matches!(move_within(&base, from, to), Err(FsMoveError::Escape)),
                "expected Escape for ({from:?}, {to:?})"
            );
        }
        assert!(base.join("a.md").exists());
    }

    #[test]
    fn move_rejects_overwrite_and_self_move() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::write(base.join("a.md"), "a").unwrap();
        fs::write(base.join("b.md"), "b").unwrap();

        assert!(matches!(
            move_within(&base, "a.md", "b.md"),
            Err(FsMoveError::DestinationExists)
        ));
        // A no-op move (same path) is also an existing destination.
        assert!(matches!(
            move_within(&base, "a.md", "a.md"),
            Err(FsMoveError::DestinationExists)
        ));
        assert_eq!(fs::read_to_string(base.join("b.md")).unwrap(), "b");
    }

    #[test]
    fn move_rejects_dir_into_itself() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::create_dir_all(base.join("dir/sub")).unwrap();

        assert!(matches!(
            move_within(&base, "dir", "dir/sub/dir"),
            Err(FsMoveError::IntoSelf)
        ));
    }

    #[test]
    fn move_distinguishes_missing_source_and_bad_destination() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::write(base.join("a.md"), "x").unwrap();

        assert!(matches!(
            move_within(&base, "ghost.md", "a2.md"),
            Err(FsMoveError::SourceMissing)
        ));
        assert!(matches!(
            move_within(&base, "a.md", "no-such-dir/a.md"),
            Err(FsMoveError::BadDestination)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn move_rejects_symlink_parent_escape() {
        let tmp = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::write(base.join("a.md"), "x").unwrap();
        std::os::unix::fs::symlink(outside.path(), base.join("evil")).unwrap();

        // Destination parent canonicalizes outside the root → Escape, and the
        // outside dir stays untouched.
        assert!(matches!(
            move_within(&base, "a.md", "evil/a.md"),
            Err(FsMoveError::Escape)
        ));
        assert!(outside.path().read_dir().unwrap().next().is_none());
    }

    // ── mkdir_within / trash_within ────────────────────────────────────

    #[test]
    fn mkdir_creates_nested_and_rejects_escape_and_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);

        mkdir_within(&base, "a/b/c").unwrap();
        assert!(base.join("a/b/c").is_dir());

        assert!(matches!(
            mkdir_within(&base, "a/b/c"),
            Err(FsMoveError::DestinationExists)
        ));
        for bad in ["../x", "/abs", "", "a/../x"] {
            assert!(
                matches!(mkdir_within(&base, bad), Err(FsMoveError::Escape)),
                "expected Escape for {bad:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn mkdir_rejects_symlinked_prefix_escape() {
        let tmp = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        std::os::unix::fs::symlink(outside.path(), base.join("evil")).unwrap();

        assert!(matches!(
            mkdir_within(&base, "evil/new-dir"),
            Err(FsMoveError::Escape)
        ));
        assert!(outside.path().read_dir().unwrap().next().is_none());
    }

    #[test]
    fn trash_moves_into_crucible_trash_without_overwrite() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::write(base.join("a.md"), "one").unwrap();

        let rel1 = trash_within(&base, "a.md").unwrap();
        assert!(!base.join("a.md").exists());
        assert_eq!(fs::read_to_string(base.join(&rel1)).unwrap(), "one");

        // Same name trashed again in the same second must not collide.
        fs::write(base.join("a.md"), "two").unwrap();
        let rel2 = trash_within(&base, "a.md").unwrap();
        assert_ne!(rel1, rel2);
        assert_eq!(fs::read_to_string(base.join(&rel2)).unwrap(), "two");
    }

    #[test]
    fn trash_refuses_dot_crucible_and_escapes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::create_dir_all(base.join(".crucible/trash")).unwrap();
        fs::write(base.join(".crucible/trash/x.md"), "x").unwrap();

        assert!(matches!(
            trash_within(&base, ".crucible/trash/x.md"),
            Err(FsMoveError::Escape)
        ));
        assert!(matches!(
            trash_within(&base, "../escape.md"),
            Err(FsMoveError::Escape)
        ));
        assert!(matches!(
            trash_within(&base, "ghost.md"),
            Err(FsMoveError::SourceMissing)
        ));
    }

    #[test]
    fn trash_takes_whole_directories_and_collects_child_notes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        fs::create_dir_all(base.join("dir/sub")).unwrap();
        fs::write(base.join("dir/a.md"), "a").unwrap();
        fs::write(base.join("dir/sub/b.md"), "b").unwrap();
        fs::write(base.join("dir/other.txt"), "t").unwrap();

        let mut notes = Vec::new();
        collect_md_files(&base.join("dir"), &mut notes);
        assert_eq!(notes.len(), 2, "only .md files collected: {notes:?}");

        let rel = trash_within(&base, "dir").unwrap();
        assert!(!base.join("dir").exists());
        assert_eq!(
            fs::read_to_string(base.join(&rel).join("a.md")).unwrap(),
            "a"
        );
        assert_eq!(
            fs::read_to_string(base.join(&rel).join("sub/b.md")).unwrap(),
            "b"
        );
    }

    #[cfg(unix)]
    #[test]
    fn move_of_symlink_moves_the_link_not_its_target() {
        let tmp = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let base = canon_root(&tmp);
        let target = outside.path().join("real.txt");
        fs::write(&target, "real").unwrap();
        std::os::unix::fs::symlink(&target, base.join("link.txt")).unwrap();
        fs::create_dir(base.join("sub")).unwrap();

        move_within(&base, "link.txt", "sub/link.txt").unwrap();
        // The link moved; the outside target is untouched.
        assert!(base
            .join("sub/link.txt")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&target).unwrap(), "real");
    }
}
