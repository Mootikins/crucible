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
    if name.contains("..") || name.starts_with('/') || name.contains('\0') {
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
