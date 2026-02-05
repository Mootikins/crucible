use crate::services::daemon::AppState;
use crate::WebError;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;

pub fn search_routes() -> Router<AppState> {
    Router::new()
        .route("/api/kilns", get(list_kilns))
        .route("/api/notes", get(list_notes))
        .route("/api/notes/{name}", get(get_note))
        .route("/api/search/vectors", post(search_vectors))
}

async fn list_kilns(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
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
