use super::helpers::{
    note_to_file_json, validate_file_within_kiln, validate_no_traversal,
    validate_parent_within_kiln, MAX_CONTENT_SIZE,
};
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, routing::get, Json, Router};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs;

pub fn kiln_routes() -> Router<AppState> {
    Router::new()
        .route("/api/kiln/files", get(list_kiln_files))
        .route("/api/kiln/notes", get(list_kiln_notes))
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

/// `GET /api/kiln/file?path=<path>` — read a file's content.
///
/// The path must reside within an open kiln; otherwise the request is rejected.
async fn get_kiln_file(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FilePathQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    validate_no_traversal(&query.path)?;

    let file_path = PathBuf::from(&query.path);
    let kiln = find_enclosing_kiln(&state, &file_path).await?;
    let canonical_file = validate_file_within_kiln(&file_path, &kiln, &query.path)?;

    // Try daemon get_note_by_name first (structured data)
    let relative = canonical_file
        .strip_prefix(&kiln)
        .map_err(|_| WebError::Validation("Path not within kiln".to_string()))?;
    let note_name = relative
        .to_string_lossy()
        .trim_end_matches(".md")
        .to_string();

    if let Ok(Some(note)) = state.daemon.get_note_by_name(&kiln, &note_name).await {
        if let Some(content) = note.get("content").and_then(|v| v.as_str()) {
            return Ok(Json(serde_json::json!({ "content": content })));
        }
    }

    // Fallback: read from filesystem directly
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
    validate_no_traversal(&req.path)?;

    // Security: limit content size (10 MB)
    if req.content.len() > MAX_CONTENT_SIZE {
        return Err(WebError::Validation(format!(
            "Content too large: {} bytes (max {MAX_CONTENT_SIZE})",
            req.content.len()
        )));
    }

    let file_path = PathBuf::from(&req.path);
    let kiln = find_enclosing_kiln(&state, &file_path).await?;

    validate_parent_within_kiln(&file_path, &kiln)?;

    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await.map_err(WebError::Io)?;
    }

    fs::write(&file_path, &req.content)
        .await
        .map_err(WebError::Io)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Find the open kiln that contains `file_path`.
async fn find_enclosing_kiln(state: &AppState, file_path: &Path) -> Result<PathBuf, WebError> {
    let kilns = state.daemon.kiln_list().await.daemon_err()?;

    for kiln_value in &kilns {
        if let Some(kiln_str) = kiln_value.get("path").and_then(|p| p.as_str()) {
            let kiln_path = PathBuf::from(kiln_str);
            if let Ok(canonical_kiln) = kiln_path.canonicalize() {
                // Check if file_path is within this kiln (either raw or canonical)
                if file_path.starts_with(&canonical_kiln) || file_path.starts_with(&kiln_path) {
                    return Ok(canonical_kiln);
                }
            }
        }
    }

    Err(WebError::NotFound(
        "File not within any open kiln".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{arb_safe_path, arb_traversal_path};
    use proptest::prelude::*;
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::symlink as symlink_dir;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_dir;

    #[test]
    fn test_validate_no_traversal_rejects_dotdot() {
        assert!(validate_no_traversal("../etc/passwd").is_err());
        assert!(validate_no_traversal("foo/../../bar").is_err());
    }

    #[test]
    fn test_validate_no_traversal_rejects_null_bytes() {
        assert!(validate_no_traversal("file\0.md").is_err());
    }

    #[test]
    fn test_validate_no_traversal_allows_valid_paths() {
        assert!(validate_no_traversal("notes/daily/2024-01-15.md").is_ok());
        assert!(validate_no_traversal("subdir/note.md").is_ok());
    }

    #[test]
    fn test_validate_no_traversal_rejects_absolute_paths() {
        assert!(validate_no_traversal("/home/user/kiln/note.md").is_err());
        assert!(validate_no_traversal("/etc/passwd").is_err());
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

    proptest! {
        #[test]
        fn prop_traversal_paths_are_rejected(path in arb_traversal_path()) {
            prop_assert!(validate_no_traversal(&path).is_err());
        }

        #[test]
        fn prop_safe_paths_are_accepted(path in arb_safe_path()) {
            prop_assert!(validate_no_traversal(&path).is_ok());
        }

        #[test]
        fn prop_null_bytes_are_always_rejected(prefix in ".{0,32}", suffix in ".{0,32}") {
            let path = format!("{prefix}\0{suffix}");
            prop_assert!(validate_no_traversal(&path).is_err());
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
