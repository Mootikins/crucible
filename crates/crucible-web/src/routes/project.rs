use crate::services::daemon::AppState;
use crate::WebError;
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;

pub fn project_routes() -> Router<AppState> {
    Router::new()
        .route("/api/project/register", post(register_project))
        .route("/api/project/unregister", post(unregister_project))
        .route("/api/project/list", get(list_projects))
        .route("/api/project/get", get(get_project))
}

#[derive(Debug, Deserialize)]
struct ProjectPathRequest {
    path: PathBuf,
}

async fn register_project(
    State(state): State<AppState>,
    Json(req): Json<ProjectPathRequest>,
) -> Result<Json<crucible_core::Project>, WebError> {
    let project = state
        .daemon
        .project_register(&req.path)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(project))
}

async fn unregister_project(
    State(state): State<AppState>,
    Json(req): Json<ProjectPathRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    state
        .daemon
        .project_unregister(&req.path)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_projects(
    State(state): State<AppState>,
) -> Result<Json<Vec<crucible_core::Project>>, WebError> {
    let projects = state
        .daemon
        .project_list()
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(projects))
}

#[derive(Debug, Deserialize)]
struct GetProjectQuery {
    path: PathBuf,
}

async fn get_project(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<GetProjectQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    match state.daemon.project_get(&query.path).await {
        Ok(Some(project)) => Ok(Json(serde_json::to_value(project).unwrap())),
        Ok(None) => Err(WebError::NotFound(format!(
            "Project not found: {}",
            query.path.display()
        ))),
        Err(e) => Err(WebError::Daemon(e.to_string())),
    }
}
