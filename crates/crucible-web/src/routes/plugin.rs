use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;

pub fn plugin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/plugins", get(list_plugins).post(install_plugin))
        .route("/api/plugins/{name}", delete(remove_plugin))
        .route("/api/plugins/{name}/reload", post(reload_plugin))
}

#[derive(Debug, Deserialize)]
struct InstallRequest {
    /// Plugin URL (e.g. "user/repo" or full git URL).
    url: String,
    branch: Option<String>,
    pin: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RemoveQuery {
    #[serde(default)]
    purge: bool,
}

/// `GET /api/plugins` — list discovered plugins with rich metadata
/// (name, version, source, state, dir, capability counts).
async fn list_plugins(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let info = state.daemon.plugin_list_info().await.daemon_err()?;
    Ok(Json(serde_json::json!({ "plugins": info })))
}

/// `POST /api/plugins/:name/reload` — reload a plugin by name.
/// Returns the daemon's reload response (counts of tools, commands, etc.).
async fn reload_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.plugin_reload(&name).await.daemon_err()?;
    Ok(Json(result))
}

/// `POST /api/plugins` — clone a plugin from a git URL and declare it
/// in plugins.toml. Synchronous; can take 10+ seconds.
async fn install_plugin(
    State(state): State<AppState>,
    Json(req): Json<InstallRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    if req.url.trim().is_empty() {
        return Err(WebError::Validation("plugin URL must not be empty".into()));
    }
    let result = state
        .daemon
        .plugin_install(&req.url, req.branch.as_deref(), req.pin.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(result))
}

/// `DELETE /api/plugins/:name?purge=true` — remove a plugin declaration.
async fn remove_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<RemoveQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .plugin_remove(&name, query.purge)
        .await
        .daemon_err()?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_routes_builds() {
        let _router = plugin_routes();
    }
}
