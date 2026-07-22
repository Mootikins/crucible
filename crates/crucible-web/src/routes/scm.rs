//! SCM browsing endpoints — branch listing and worktree creation for the
//! files-pane root picker. Thin proxies over the daemon's `scm.*` RPCs.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;

pub fn scm_routes() -> Router<AppState> {
    Router::new()
        .route("/api/scm/branches", get(list_branches))
        .route("/api/scm/worktree", post(worktree_add))
}

#[derive(Debug, Deserialize)]
struct BranchesQuery {
    /// Any path inside the repo (project root, worktree, subdir).
    path: PathBuf,
}

async fn list_branches(
    State(state): State<AppState>,
    Query(query): Query<BranchesQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let branches = state.daemon.scm_branches(&query.path).await.daemon_err()?;
    Ok(Json(serde_json::to_value(branches).map_err(|e| {
        WebError::Internal(format!("Failed to serialize branches: {e}"))
    })?))
}

#[derive(Debug, Deserialize)]
struct WorktreeAddRequest {
    repo_root: PathBuf,
    branch: String,
    #[serde(default)]
    create_branch: bool,
}

async fn worktree_add(
    State(state): State<AppState>,
    Json(req): Json<WorktreeAddRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .scm_worktree_add(&req.repo_root, &req.branch, req.create_branch)
        .await
        .daemon_err()?;
    Ok(Json(serde_json::to_value(result).map_err(|e| {
        WebError::Internal(format!("Failed to serialize worktree result: {e}"))
    })?))
}
