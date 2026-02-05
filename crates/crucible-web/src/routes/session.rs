use crate::services::daemon::AppState;
use crate::WebError;
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;

pub fn session_routes() -> Router<AppState> {
    Router::new()
        .route("/api/session", post(create_session))
        .route("/api/session/list", get(list_sessions))
        .route("/api/session/{id}", get(get_session))
        .route("/api/session/{id}/pause", post(pause_session))
        .route("/api/session/{id}/resume", post(resume_session))
        .route("/api/session/{id}/end", post(end_session))
        .route("/api/session/{id}/cancel", post(cancel_session))
        .route("/api/session/{id}/models", get(list_models))
        .route("/api/session/{id}/model", post(switch_model))
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    #[serde(default = "default_session_type")]
    session_type: String,
    kiln: PathBuf,
    workspace: Option<PathBuf>,
}

fn default_session_type() -> String {
    "chat".to_string()
}

async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_create(
            &req.session_type,
            &req.kiln,
            req.workspace.as_deref(),
            vec![],
        )
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    let session_id = result["session_id"].as_str().unwrap_or("");
    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct ListSessionsQuery {
    kiln: Option<PathBuf>,
    workspace: Option<PathBuf>,
    #[serde(rename = "type")]
    session_type: Option<String>,
    state: Option<String>,
}

async fn list_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListSessionsQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_list(
            query.kiln.as_deref(),
            query.workspace.as_deref(),
            query.session_type.as_deref(),
            query.state.as_deref(),
        )
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_get(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

async fn pause_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_pause(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

async fn resume_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_resume(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    let session_id = id.as_str();
    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_end(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    state.events.remove_session(&id).await;

    Ok(Json(result))
}

async fn cancel_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let cancelled = state
        .daemon
        .session_cancel(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "cancelled": cancelled })))
}

async fn list_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let models = state
        .daemon
        .session_list_models(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "models": models })))
}

#[derive(Debug, Deserialize)]
struct SwitchModelRequest {
    model_id: String,
}

async fn switch_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SwitchModelRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    state
        .daemon
        .session_switch_model(&id, &req.model_id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
