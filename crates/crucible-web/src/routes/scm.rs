//! Repository cloning — a thin proxy over the daemon's `scm.clone` RPC.
//!
//! Branch listing and worktree creation used to live here too. They are the
//! worktree plugin's business now: this layer held a second copy of what a
//! branch is, and the daemon held a third.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, routing::post, Json, Router};
use serde::Deserialize;
use std::path::PathBuf;

pub fn scm_routes() -> Router<AppState> {
    Router::new().route("/api/scm/clone", post(clone_repo))
}

#[derive(Debug, Deserialize)]
struct CloneRequest {
    /// Remote repo: https://…, git@host:…, or `owner/repo` shorthand.
    url: String,
    dest: Option<PathBuf>,
    name: Option<String>,
}

/// Clone a remote repo into `[workspace] root_dir` and register it as a
/// project. Slow by nature — the daemon call carries a long timeout.
async fn clone_repo(
    State(state): State<AppState>,
    Json(req): Json<CloneRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .scm_clone(&req.url, req.dest.as_deref(), req.name.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(serde_json::to_value(result).map_err(|e| {
        WebError::Internal(format!("Failed to serialize clone result: {e}"))
    })?))
}
