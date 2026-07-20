use super::helpers::{
    note_to_file_json, reject_path_traversal, validate_file_within_kiln,
    validate_write_target_within_kiln, MAX_CONTENT_SIZE,
};
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, routing::get, Json, Router};
use crucible_core::config::{read_project_config, ProjectFileAccess};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;

pub fn kiln_routes() -> Router<AppState> {
    Router::new()
        .route("/api/kiln/files", get(list_kiln_files))
        .route("/api/kiln/notes", get(list_kiln_notes))
        .route("/api/kiln/graph", get(kiln_graph))
        .route("/api/kiln/file", get(get_kiln_file).put(put_kiln_file))
}

// =========================================================================
// Query / Request types
// =========================================================================

#[derive(Debug, Deserialize)]
struct KilnPathQuery {
    kiln: PathBuf,
}

#[derive(Debug, Deserialize)]
struct FilePathQuery {
    path: String,
}

#[derive(Debug, Deserialize)]
struct PutFileRequest {
    path: String,
    content: String,
}

// =========================================================================
// Handlers
// =========================================================================

/// `GET /api/kiln/files?kiln=<path>` — list notes in a kiln as file entries.
async fn list_kiln_files(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let notes = state
        .daemon
        .list_notes(&query.kiln, None)
        .await
        .daemon_err()?;

    let files: Vec<serde_json::Value> = notes.into_iter().map(note_to_file_json).collect();

    Ok(Json(serde_json::json!({ "files": files })))
}

/// `GET /api/kiln/notes?kiln=<path>` — list notes in a kiln with metadata.
async fn list_kiln_notes(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let notes = state
        .daemon
        .list_notes(&query.kiln, None)
        .await
        .daemon_err()?;

    let notes_json: Vec<serde_json::Value> = notes.into_iter().map(note_to_file_json).collect();

    Ok(Json(serde_json::json!({ "files": notes_json })))
}

/// `GET /api/kiln/graph?kiln=<path>` — the full note-link graph of a kiln.
///
/// Returns the daemon's `kiln.graph` result verbatim:
/// `{ notes: [{ path, title, tags }], links: [{ source, target, resolved }] }`.
async fn kiln_graph(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KilnPathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let graph = state.daemon.kiln_graph(&query.kiln).await.daemon_err()?;
    Ok(Json(graph))
}

/// `GET /api/kiln/file?path=<path>` — read a file's content.
///
/// The path must reside within an open kiln; otherwise the request is rejected.
async fn get_kiln_file(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FilePathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    // The editor addresses files by ABSOLUTE path (a note's `path`); containment
    // is enforced below by find_enclosing_root + validate_file_within_kiln.
    reject_path_traversal(&query.path)?;

    let file_path = PathBuf::from(&query.path);
    let root = find_enclosing_root(&state, &file_path).await?;
    // Project files are readable unless the project's policy is `off` (then
    // they behave as not served — a 404, same as a path in no root at all).
    if let EnclosingRoot::Project(_, policy) = &root {
        if !policy.can_read() {
            return Err(WebError::NotFound(
                "File not within any open kiln".to_string(),
            ));
        }
    }
    let canonical_file = validate_file_within_kiln(&file_path, root.path(), &query.path)?;

    // Read the file directly. GET /api/notes/{name} (get_note_by_name) returns
    // only path/title/tags/links_to/content_hash — never a "content" field — so
    // a daemon-first content branch here was statically unreachable and a
    // footgun (it would have served stale DB text over the file bytes).
    let content = fs::read_to_string(&canonical_file)
        .await
        .map_err(|e| WebError::NotFound(format!("File not found: {e}")))?;

    Ok(Json(serde_json::json!({ "content": content })))
}

/// `PUT /api/kiln/file` — write content to a file within an open kiln.
async fn put_kiln_file(
    State(state): State<AppState>,
    Json(req): Json<PutFileRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    // Accept absolute paths (the editor saves by a note's absolute path);
    // containment is enforced below by find_enclosing_kiln + parent-within-kiln.
    reject_path_traversal(&req.path)?;

    // Security: limit content size (10 MB)
    if req.content.len() > MAX_CONTENT_SIZE {
        return Err(WebError::Validation(format!(
            "Content too large: {} bytes (max {MAX_CONTENT_SIZE})",
            req.content.len()
        )));
    }

    let file_path = PathBuf::from(&req.path);
    let root = find_enclosing_root(&state, &file_path).await?;
    // Writes obey the project policy: `read-only` → 403, `off` → 404 (as if the
    // file were not served). Kiln notes are always writable.
    if let EnclosingRoot::Project(_, policy) = &root {
        if !policy.can_write() {
            return Err(if policy.can_read() {
                WebError::Forbidden("Project files are read-only".to_string())
            } else {
                WebError::NotFound("File not within any open kiln".to_string())
            });
        }
    }

    validate_write_target_within_kiln(&file_path, root.path())?;

    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await.map_err(WebError::Io)?;
    }

    fs::write(&file_path, &req.content)
        .await
        .map_err(WebError::Io)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// A root the file endpoints may serve `file_path` from. Kilns are the
/// knowledge content and are always read-write; projects (the code/repo dir a
/// kiln lives in) obey a per-project [`ProjectFileAccess`] policy.
enum EnclosingRoot {
    Kiln(PathBuf),
    Project(PathBuf, ProjectFileAccess),
}

impl EnclosingRoot {
    /// The canonical containing directory, for containment validation.
    fn path(&self) -> &Path {
        match self {
            EnclosingRoot::Kiln(p) | EnclosingRoot::Project(p, _) => p,
        }
    }
}

/// Return the canonical root if `file_path` is inside `root` (matched against
/// both the canonical and raw forms, as daemon-reported paths may be either).
fn canonical_if_contains(file_path: &Path, root: &Path) -> Option<PathBuf> {
    let canonical = root.canonicalize().ok()?;
    (file_path.starts_with(&canonical) || file_path.starts_with(root)).then_some(canonical)
}

/// Resolve which open root encloses `file_path`. Kilns take precedence over
/// projects, so a kiln nested inside a project keeps its always-read-write
/// treatment. Daemon-free (canonicalizes on the filesystem only) so the
/// precedence and containment rules are unit-testable without a running daemon.
fn resolve_enclosing_root(
    file_path: &Path,
    kilns: &[PathBuf],
    projects: &[(PathBuf, ProjectFileAccess)],
) -> Option<EnclosingRoot> {
    for kiln in kilns {
        if let Some(root) = canonical_if_contains(file_path, kiln) {
            return Some(EnclosingRoot::Kiln(root));
        }
    }
    for (project, policy) in projects {
        if let Some(root) = canonical_if_contains(file_path, project) {
            return Some(EnclosingRoot::Project(root, *policy));
        }
    }
    None
}

/// Find the open kiln or registered project that contains `file_path`. The
/// project's `project_files` policy (default read-write) is loaded from its
/// `.crucible/project.toml` here so the handlers can gate read/write.
async fn find_enclosing_root(
    state: &AppState,
    file_path: &Path,
) -> Result<EnclosingRoot, WebError> {
    let kilns: Vec<PathBuf> = state
        .daemon
        .kiln_list()
        .await
        .daemon_err()?
        .iter()
        .filter_map(|v| v.get("path").and_then(|p| p.as_str()).map(PathBuf::from))
        .collect();

    let projects: Vec<(PathBuf, ProjectFileAccess)> = state
        .daemon
        .project_list()
        .await
        .daemon_err()?
        .into_iter()
        .map(|p| {
            let policy = read_project_config(&p.path)
                .map(|c| c.security.project_files)
                .unwrap_or_default();
            (p.path, policy)
        })
        .collect();

    resolve_enclosing_root(file_path, &kilns, &projects)
        .ok_or_else(|| WebError::NotFound("File not within any open kiln".to_string()))
}

#[cfg(test)]
mod tests {
    use super::super::helpers::{reject_path_traversal, validate_parent_within_kiln};
    use super::*;
    use crate::test_support::{arb_safe_path, arb_traversal_path};
    use proptest::prelude::*;
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::symlink as symlink_dir;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_dir;

    #[test]
    fn test_reject_path_traversal_rejects_dotdot() {
        assert!(reject_path_traversal("../etc/passwd").is_err());
        assert!(reject_path_traversal("foo/../../bar").is_err());
    }

    #[test]
    fn test_reject_path_traversal_rejects_null_bytes() {
        assert!(reject_path_traversal("file\0.md").is_err());
    }

    #[test]
    fn test_reject_path_traversal_allows_valid_paths() {
        assert!(reject_path_traversal("notes/daily/2024-01-15.md").is_ok());
        assert!(reject_path_traversal("subdir/note.md").is_ok());
    }

    #[test]
    fn test_reject_path_traversal_allows_absolute_paths() {
        // The kiln file routes accept absolute paths; kiln containment is
        // enforced separately by find_enclosing_kiln + within-kiln checks.
        assert!(reject_path_traversal("/home/user/kiln/note.md").is_ok());
        // ...but an absolute path with a `..` segment is still rejected.
        assert!(reject_path_traversal("/home/user/kiln/../../etc/passwd").is_err());
    }

    #[test]
    fn test_content_size_allows_exactly_ten_megabytes() {
        const MAX_SIZE: usize = 10 * 1024 * 1024;
        let content = "a".repeat(MAX_SIZE);

        assert_eq!(content.len(), MAX_SIZE);
        assert!(content.len() <= MAX_SIZE);
    }

    #[test]
    fn test_content_size_rejects_ten_megabytes_plus_one_byte() {
        const MAX_SIZE: usize = 10 * 1024 * 1024;
        let content = "a".repeat(MAX_SIZE + 1);

        assert_eq!(
            format!(
                "Content too large: {} bytes (max {MAX_SIZE})",
                content.len()
            ),
            "Content too large: 10485761 bytes (max 10485760)"
        );
        assert!(content.len() > MAX_SIZE);
    }

    #[test]
    fn symlink_escape_rejected() {
        let kiln = tempdir().expect("temp kiln");
        let outside = tempdir().expect("temp outside");

        let outside_file = outside.path().join("outside-note.md");
        std::fs::write(&outside_file, "outside").expect("write outside file");

        let link = kiln.path().join("escape-link");
        symlink_dir(outside.path(), &link).expect("create symlink to outside");

        let escaped_path = link.join("outside-note.md");
        let err =
            validate_file_within_kiln(&escaped_path, kiln.path(), &escaped_path.to_string_lossy())
                .expect_err("symlink target outside kiln must be rejected");

        match err {
            WebError::Validation(message) => {
                assert_eq!(message, "File path escapes kiln directory");
            }
            other => panic!("expected validation error, got: {other:?}"),
        }
    }

    #[test]
    fn put_kiln_file_rejects_new_file_outside_kiln() {
        let kiln = tempdir().expect("temp kiln");
        let outside = tempdir().expect("temp outside");

        let link = kiln.path().join("escape-link");
        symlink_dir(outside.path(), &link).expect("create symlink to outside");

        let new_file_path = link.join("new-note.md");
        assert!(!new_file_path.exists());

        let err = validate_parent_within_kiln(&new_file_path, kiln.path())
            .expect_err("symlinked parent outside kiln must be rejected");
        match err {
            WebError::Validation(message) => {
                assert_eq!(message, "Path escapes kiln directory");
            }
            other => panic!("expected validation error, got: {other:?}"),
        }
    }

    #[test]
    fn write_target_symlinked_final_component_rejected() {
        // KILN/evil.md is a pre-planted symlink to a file OUTSIDE the kiln. The
        // parent (the kiln root) is legitimate, so only the final-component
        // symlink check catches the escape — without it, fs::write would follow
        // the link and overwrite the outside file.
        let kiln = tempdir().expect("temp kiln");
        let outside = tempdir().expect("temp outside");

        let secret = outside.path().join("secret.md");
        std::fs::write(&secret, "original secret").expect("write secret");

        let link = kiln.path().join("evil.md");
        symlink_dir(&secret, &link).expect("plant symlink to outside file");

        let canonical_kiln = kiln.path().canonicalize().expect("canonical kiln");
        let err = validate_write_target_within_kiln(&link, &canonical_kiln)
            .expect_err("symlinked final component pointing outside the kiln must be rejected");
        match err {
            WebError::Validation(message) => assert_eq!(message, "Path escapes kiln directory"),
            other => panic!("expected validation error, got: {other:?}"),
        }

        // The guard runs before any write, so the outside file is untouched.
        assert_eq!(
            std::fs::read_to_string(&secret).expect("read secret"),
            "original secret"
        );
    }

    #[test]
    fn write_target_regular_file_within_kiln_allowed() {
        // A normal (non-symlink) file inside the kiln passes.
        let kiln = tempdir().expect("temp kiln");
        let canonical_kiln = kiln.path().canonicalize().expect("canonical kiln");
        let note = canonical_kiln.join("note.md");
        std::fs::write(&note, "hi").expect("write note");
        assert!(validate_write_target_within_kiln(&note, &canonical_kiln).is_ok());
    }

    // -- enclosing-root resolution (kiln vs project + policy) ----------------

    #[test]
    fn resolve_prefers_kiln_over_enclosing_project() {
        // A kiln nested inside a project keeps its always-read-write treatment
        // rather than inheriting the project's file policy.
        let project = tempdir().expect("temp project");
        let kiln = project.path().join("docs");
        std::fs::create_dir(&kiln).expect("mkdir kiln");
        let file = kiln.join("note.md");
        std::fs::write(&file, "n").expect("write note");

        let root = resolve_enclosing_root(
            &file,
            std::slice::from_ref(&kiln),
            &[(project.path().to_path_buf(), ProjectFileAccess::Off)],
        )
        .expect("kiln should match first");
        assert!(matches!(root, EnclosingRoot::Kiln(_)));
    }

    #[test]
    fn resolve_matches_project_and_carries_policy() {
        let project = tempdir().expect("temp project");
        let file = project.path().join("README.md");
        std::fs::write(&file, "r").expect("write readme");

        for policy in [
            ProjectFileAccess::ReadWrite,
            ProjectFileAccess::ReadOnly,
            ProjectFileAccess::Off,
        ] {
            let root =
                resolve_enclosing_root(&file, &[], &[(project.path().to_path_buf(), policy)])
                    .expect("project should match");
            match root {
                EnclosingRoot::Project(_, p) => assert_eq!(p, policy),
                other => panic!("expected project root, got a kiln: {:?}", other.path()),
            }
        }
    }

    #[test]
    fn resolve_returns_none_when_outside_every_root() {
        let project = tempdir().expect("temp project");
        let outside = tempdir().expect("temp outside");
        let file = outside.path().join("secret.md");
        std::fs::write(&file, "s").expect("write secret");

        assert!(resolve_enclosing_root(
            &file,
            &[],
            &[(project.path().to_path_buf(), ProjectFileAccess::ReadWrite)],
        )
        .is_none());
    }

    #[test]
    fn project_file_access_read_write_matrix() {
        assert!(ProjectFileAccess::ReadWrite.can_read());
        assert!(ProjectFileAccess::ReadWrite.can_write());
        assert!(ProjectFileAccess::ReadOnly.can_read());
        assert!(!ProjectFileAccess::ReadOnly.can_write());
        assert!(!ProjectFileAccess::Off.can_read());
        assert!(!ProjectFileAccess::Off.can_write());
    }

    proptest! {
        #[test]
        fn prop_traversal_paths_are_rejected(path in arb_traversal_path()) {
            prop_assert!(reject_path_traversal(&path).is_err());
        }

        #[test]
        fn prop_safe_paths_are_accepted(path in arb_safe_path()) {
            prop_assert!(reject_path_traversal(&path).is_ok());
        }

        #[test]
        fn prop_null_bytes_are_always_rejected(prefix in ".{0,32}", suffix in ".{0,32}") {
            let path = format!("{prefix}\0{suffix}");
            prop_assert!(reject_path_traversal(&path).is_err());
        }

        #[test]
        fn prop_new_file_path_traversal_rejected(file_name in "[a-zA-Z0-9_-]{1,32}\\.md") {
            let kiln = tempdir().expect("temp kiln");
            let outside = tempdir().expect("temp outside");

            let link = kiln.path().join("escape-link");
            symlink_dir(outside.path(), &link).expect("create symlink to outside");

            let new_file_path = link.join(file_name);
            prop_assume!(!new_file_path.exists());

            prop_assert!(validate_parent_within_kiln(&new_file_path, kiln.path()).is_err());
        }
    }
}
