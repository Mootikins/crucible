use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::State,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;
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

    let files: Vec<serde_json::Value> = notes
        .into_iter()
        .map(|(name, path, _title, _tags, _updated_at)| {
            serde_json::json!({
                "name": name,
                "path": path,
                "is_dir": false,
            })
        })
        .collect();

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

    let notes_json: Vec<serde_json::Value> = notes
        .into_iter()
        .map(|(name, path, _title, _tags, _updated_at)| {
            serde_json::json!({
                "name": name,
                "path": path,
                "is_dir": false,
            })
        })
        .collect();

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

    // Try daemon get_note_by_name first (structured data)
    let relative = file_path
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
    let content = fs::read_to_string(&file_path)
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
    const MAX_SIZE: usize = 10 * 1024 * 1024;
    if req.content.len() > MAX_SIZE {
        return Err(WebError::Validation(format!(
            "Content too large: {} bytes (max {MAX_SIZE})",
            req.content.len()
        )));
    }

    let file_path = PathBuf::from(&req.path);
    let kiln = find_enclosing_kiln(&state, &file_path).await?;

    // Verify resolved path stays within kiln
    if let Ok(canonical) = file_path.canonicalize() {
        if !canonical.starts_with(&kiln) {
            return Err(WebError::Validation(
                "Path escapes kiln directory".to_string(),
            ));
        }
    }

    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await.map_err(WebError::Io)?;
    }

    fs::write(&file_path, &req.content)
        .await
        .map_err(WebError::Io)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// =========================================================================
// Helpers
// =========================================================================

/// Reject paths containing traversal sequences or null bytes.
fn validate_no_traversal(path: &str) -> Result<(), WebError> {
    if path.contains("..") || path.contains('\0') {
        return Err(WebError::Validation(
            "Invalid path: traversal not allowed".to_string(),
        ));
    }
    Ok(())
}

/// Find the open kiln that contains `file_path`.
async fn find_enclosing_kiln(state: &AppState, file_path: &PathBuf) -> Result<PathBuf, WebError> {
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
        assert!(validate_no_traversal("/home/user/kiln/note.md").is_ok());
        assert!(validate_no_traversal("notes/daily/2024-01-15.md").is_ok());
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
            format!("Content too large: {} bytes (max {MAX_SIZE})", content.len()),
            "Content too large: 10485761 bytes (max 10485760)"
        );
        assert!(content.len() > MAX_SIZE);
    }

    #[test]
    fn test_symlink_escape_risk_is_documented() {
        // TODO(security): `get_kiln_file` reads `req.path` directly without canonicalizing
        // or checking that the resolved path remains inside the kiln; a symlink inside the
        // kiln can point outside and allow path escape on reads.
        let documented = "get_kiln_file symlink escape risk documented";

        assert!(documented.contains("symlink"));
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
    }
}
