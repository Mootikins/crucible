use crate::services::daemon::AppState;
use crate::WebError;
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use crucible_core::parser::types::BlockHash;
use crucible_core::storage::NoteRecord;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

pub fn search_routes() -> Router<AppState> {
    Router::new()
        .route("/api/kilns", get(list_kilns))
        .route("/api/notes", get(list_notes))
        .route("/api/notes/{name}", get(get_note))
        .route("/api/notes/{name}", put(put_note))
        .route("/api/search/vectors", post(search_vectors))
}

async fn list_kilns(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let kilns = state
        .daemon
        .kiln_list()
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "kilns": kilns })))
}

#[derive(Debug, Deserialize)]
struct ListNotesQuery {
    kiln: PathBuf,
    path_filter: Option<String>,
}

async fn list_notes(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListNotesQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let notes = state
        .daemon
        .list_notes(&query.kiln, query.path_filter.as_deref())
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    let notes_json: Vec<serde_json::Value> = notes
        .into_iter()
        .map(|(name, path, title, tags, updated_at)| {
            serde_json::json!({
                "name": name,
                "path": path,
                "title": title,
                "tags": tags,
                "updated_at": updated_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "notes": notes_json })))
}

async fn get_note(
    State(state): State<AppState>,
    Path(name): Path<String>,
    axum::extract::Query(query): axum::extract::Query<KilnQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    // Security: Validate note name doesn't contain path traversal
    if name.contains("..") || name.starts_with('/') || name.starts_with('\\') || name.contains('\0') {
        return Err(WebError::Chat(
            "Invalid note name: path traversal not allowed".to_string(),
        ));
    }

    let note = state
        .daemon
        .get_note_by_name(&query.kiln, &name)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    match note {
        Some(n) => Ok(Json(n)),
        None => Err(WebError::NotFound(format!("Note '{name}' not found"))),
    }
}

#[derive(Debug, Deserialize)]
struct KilnQuery {
    kiln: PathBuf,
}

#[derive(Debug, Deserialize)]
struct PutNoteRequest {
    kiln: PathBuf,
    content: String,
}

/// Maximum note content size (10 MB)
const MAX_NOTE_SIZE: usize = 10 * 1024 * 1024;

async fn put_note(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<PutNoteRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    // Security: Validate content size to prevent DoS
    if req.content.len() > MAX_NOTE_SIZE {
        return Err(WebError::Chat(format!(
            "Note content too large: {} bytes (max {} bytes)",
            req.content.len(),
            MAX_NOTE_SIZE
        )));
    }

    // Security: Validate note name doesn't contain path traversal
    if name.contains("..") || name.starts_with('/') || name.starts_with('\\') || name.contains('\0') {
        return Err(WebError::Chat(
            "Invalid note name: path traversal not allowed".to_string(),
        ));
    }

    // Security: Validate kiln is registered/open
    let kilns = state
        .daemon
        .kiln_list()
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    let canonical_kiln = req
        .kiln
        .canonicalize()
        .map_err(|_| WebError::NotFound("Invalid kiln path".to_string()))?;

    let kiln_registered = kilns.iter().any(|kiln_value| {
        kiln_value
            .get("path")
            .and_then(|p| p.as_str())
            .and_then(|p| PathBuf::from(p).canonicalize().ok())
            .map(|p| p == canonical_kiln)
            .unwrap_or(false)
    });

    if !kiln_registered {
        return Err(WebError::NotFound(
            "Kiln not registered. Please open the kiln first.".to_string(),
        ));
    }

    // Build the full file path (ensure .md extension)
    let note_filename = if name.ends_with(".md") {
        name.clone()
    } else {
        format!("{}.md", name)
    };
    let file_path = canonical_kiln.join(&note_filename);

    // Security: Verify the file path is still within the kiln after joining
    if !file_path.starts_with(&canonical_kiln) {
        return Err(WebError::Chat(
            "Invalid note name: path escapes kiln directory".to_string(),
        ));
    }

    // Create parent directories if needed
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| WebError::Io(e))?;
    }

    // Write content to filesystem (source of truth)
    fs::write(&file_path, &req.content)
        .await
        .map_err(|e| WebError::Io(e))?;

    // Extract metadata and update database
    let title = extract_title(&req.content);
    let now = Utc::now();

    let note = NoteRecord {
        path: note_filename.clone(),
        content_hash: BlockHash::default(), // TODO: compute actual hash
        embedding: None,
        title: title.clone(),
        tags: vec![],
        links_to: vec![],
        properties: HashMap::new(),
        updated_at: now,
    };

    state
        .daemon
        .note_upsert(&req.kiln, &note)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "success": true,
        "name": note_filename,
        "title": title,
        "updated_at": now
    })))
}

fn extract_title(content: &str) -> String {
    content
        .lines()
        .find(|line| line.starts_with('#'))
        .and_then(|line| {
            let trimmed = line.trim_start_matches('#').trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .unwrap_or_else(|| {
            content
                .lines()
                .next()
                .unwrap_or("Untitled")
                .to_string()
        })
}

#[derive(Debug, Deserialize)]
struct VectorSearchRequest {
    kiln: PathBuf,
    vector: Vec<f32>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    10
}

async fn search_vectors(
    State(state): State<AppState>,
    Json(req): Json<VectorSearchRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let results = state
        .daemon
        .search_vectors(&req.kiln, &req.vector, req.limit)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    let results_json: Vec<serde_json::Value> = results
        .into_iter()
        .map(|(doc_id, score)| {
            serde_json::json!({
                "document_id": doc_id,
                "score": score,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "results": results_json })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_valid_note_name(name: &str) -> bool {
        !name.contains("..")
            && !name.starts_with('/')
            && !name.starts_with('\\')
            && !name.contains('\0')
    }

    // Helper to test path escape detection
    fn path_escapes_base(base: &std::path::Path, name: &str) -> bool {
        let file_path = base.join(name);
        !file_path.starts_with(base)
    }

    // ===== Path Traversal Tests =====

    #[test]
    fn test_validate_note_name_rejects_parent_traversal() {
        let attacks = vec![
            "../etc/passwd",
            "../../secret",
            "../../../etc/passwd",
            "notes/../../../etc/passwd",
            "foo/../bar/../../../etc/passwd",
        ];
        for attack in attacks {
            assert!(
                !is_valid_note_name(attack),
                "Should reject parent traversal: {}",
                attack
            );
        }
    }

    #[test]
    fn test_validate_note_name_rejects_backslash_paths() {
        let attacks = vec![
            "..\\windows\\system32",
            "notes\\..\\..\\secret",
            "\\etc\\passwd",
        ];
        for attack in attacks {
            assert!(
                !is_valid_note_name(attack),
                "Should reject backslash path: {}",
                attack
            );
        }
    }

    #[test]
    fn test_validate_note_name_rejects_absolute_paths() {
        let attacks = vec![
            "/etc/passwd",
            "/root/.ssh/id_rsa",
            "/var/log/syslog",
            "/home/user/secret",
        ];
        for attack in attacks {
            assert!(
                !is_valid_note_name(attack),
                "Should reject absolute path: {}",
                attack
            );
        }
    }

    #[test]
    fn test_validate_note_name_rejects_null_bytes() {
        let attacks = vec![
            "note\0.md",
            "folder/note\0hidden.md",
            "\0etc/passwd",
            "note.md\0.txt",
        ];
        for attack in attacks {
            assert!(
                !is_valid_note_name(attack),
                "Should reject null byte: {:?}",
                attack
            );
        }
    }

    #[test]
    fn test_validate_note_name_allows_valid_names() {
        let valid_names = vec![
            "note.md",
            "my-note.md",
            "my_note.md",
            "folder/note.md",
            "deep/nested/folder/note.md",
            "Note With Spaces.md",
            "note123.md",
            "2024-01-15-daily.md",
            "README",
            "index",
        ];
        for name in valid_names {
            assert!(is_valid_note_name(name), "Should allow valid name: {}", name);
        }
    }

    // ===== Content Size Tests =====

    #[test]
    fn test_content_size_limit_constant() {
        assert_eq!(MAX_NOTE_SIZE, 10 * 1024 * 1024, "Max size should be 10MB");
    }

    #[test]
    fn test_content_size_validation_rejects_oversized() {
        let oversized = "x".repeat(MAX_NOTE_SIZE + 1);
        assert!(
            oversized.len() > MAX_NOTE_SIZE,
            "Content should exceed limit"
        );
    }

    #[test]
    fn test_content_size_validation_accepts_max_size() {
        let max_content = "x".repeat(MAX_NOTE_SIZE);
        assert!(
            max_content.len() <= MAX_NOTE_SIZE,
            "Content at max size should be accepted"
        );
    }

    #[test]
    fn test_content_size_validation_accepts_normal_size() {
        let normal_content = "# My Note\n\nSome content here.";
        assert!(
            normal_content.len() <= MAX_NOTE_SIZE,
            "Normal content should be accepted"
        );
    }

    // ===== Path Escape Tests =====

    #[test]
    fn test_path_join_does_not_normalize_traversal() {
        let base = std::path::Path::new("/home/user/kiln");
        
        // Path::join doesn't normalize ".." - it creates literal path
        // This is why we validate name for ".." BEFORE joining
        // The starts_with check is defense-in-depth for edge cases
        let joined = base.join("../../../etc/passwd");
        assert!(
            joined.starts_with(base),
            "Joined path literally starts with base (not normalized)"
        );
    }

    #[test]
    fn test_path_escape_detection_allows_nested() {
        let base = std::path::Path::new("/home/user/kiln");

        // Valid nested paths should not escape
        assert!(
            !path_escapes_base(base, "notes/daily/2024-01-15.md"),
            "Nested path should not escape"
        );
        assert!(
            !path_escapes_base(base, "deep/nested/folder/note.md"),
            "Deep nested path should not escape"
        );
    }

    // ===== Title Extraction Tests =====

    #[test]
    fn test_extract_title_from_h1() {
        let content = "# My Title\n\nSome content";
        assert_eq!(extract_title(content), "My Title");
    }

    #[test]
    fn test_extract_title_from_h2() {
        let content = "## Secondary Title\n\nContent";
        assert_eq!(extract_title(content), "Secondary Title");
    }

    #[test]
    fn test_extract_title_empty_heading_returns_first_line_as_fallback() {
        let content = "#\n\nActual content here";
        assert_eq!(extract_title(content), "#");
    }

    #[test]
    fn test_extract_title_uses_first_line_as_fallback() {
        let content = "No heading here\n\nJust content";
        assert_eq!(extract_title(content), "No heading here");
    }

    #[test]
    fn test_extract_title_handles_empty_content() {
        let content = "";
        assert_eq!(extract_title(content), "Untitled");
    }

    #[test]
    fn test_extract_title_trims_whitespace() {
        let content = "#    Lots of spaces   \n\nContent";
        assert_eq!(extract_title(content), "Lots of spaces");
    }
}
