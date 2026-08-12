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
//! # Visibility policy (two independent axes)
//!
//! - `show_ignored` (default false): reveal gitignored entries. The web UI
//!   passes `true` — a file browser must show ALL files in a folder, not
//!   just the git-clean subset (`target/`, `node_modules/`, …).
//! - `show_hidden` (default false): reveal dotfiles/dot-dirs. Off by
//!   default so non-gitignored secret files (`.env`, `.netrc`, `.envrc`)
//!   are not enumerable unless explicitly requested (the tree's
//!   "Show hidden files" toggle).
//! - `.git` is NEVER listed, regardless of both flags.
//!
//! The residual accepted risk is that an authenticated same-origin caller
//! can enumerate names/sizes within registered projects (equivalent to the
//! user's own shell access).
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
    let show_hidden = optional_param!(req, "show_hidden", as_bool).unwrap_or(false);

    match list_dir(pm, Path::new(root), rel_path, show_ignored, show_hidden) {
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
    show_hidden: bool,
) -> Result<Vec<FsEntry>, FsListError> {
    // Fail-closed allowlist: only registered projects are listable.
    let project = pm.get(root).ok_or(FsListError::NotRegistered)?;
    let base = project.path.canonicalize()?;
    let target = resolve_within(&base, rel_path)?;
    if !target.is_dir() {
        return Err(FsListError::NotADir);
    }
    walk_one_level(&base, &target, show_ignored, show_hidden)
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
/// applying the two visibility axes (gitignored / hidden) and dropping any
/// entry whose symlink resolves outside `base`. `.git` never lists.
/// Dirs-first, then case-insensitive name.
fn walk_one_level(
    base: &Path,
    dir: &Path,
    show_ignored: bool,
    show_hidden: bool,
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
        .hidden(!show_hidden)
        .follow_links(false)
        .build();

    for dent in walker {
        let Ok(dent) = dent else { continue };
        if dent.depth() == 0 {
            continue; // skip `dir` itself
        }
        let path = dent.path();
        let name = dent.file_name().to_string_lossy().to_string();
        if name == ".git" {
            continue; // never enumerable, regardless of visibility flags
        }

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

    // Indexed kiln files route through the link-aware rename: the move AND the
    // inbound-link rewrite/reindex happen as one operation, so a DnD move in
    // the web file tree can never silently break links. This covers canvases
    // as well as notes — a canvas is a link source, and moving one without
    // repointing inbound references would break them exactly as it would for a
    // note. Directories and assets keep the plain rename (folder-level bulk
    // rewrite is Phase 3.1; bare-stem links to children keep resolving via key
    // re-resolution regardless).
    if kind == "kiln"
        && crucible_core::kiln::is_indexable_file(std::path::Path::new(from_rel))
        && crucible_core::kiln::is_indexable_file(std::path::Path::new(to_rel))
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
        collect_indexed_files(&source, &mut removed_notes);
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

/// Indexed files at or under `path` — the pre-move index-cleanup set.
///
/// Every kind with an index row, not just notes: a canvas and a `.txt` each have
/// one, so moving or trashing a directory has to drop theirs too.
fn collect_indexed_files(path: &Path, out: &mut Vec<PathBuf>) {
    let meta = match path.symlink_metadata() {
        Ok(m) => m,
        Err(_) => return,
    };
    if meta.is_file() {
        if crucible_core::kiln::is_indexable_file(path) {
            out.push(path.to_path_buf());
        }
        return;
    }
    if meta.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                collect_indexed_files(&entry.path(), out);
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
mod tests;
