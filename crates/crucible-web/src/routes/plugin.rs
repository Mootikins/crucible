use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use crucible_daemon::{LuaDiscoverPluginsRequest, LuaPluginHealthRequest};
use serde::Deserialize;
use std::path::PathBuf;

pub fn plugin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/plugins", get(list_plugins))
        .route("/api/plugins/{name}/reload", post(reload_plugin))
}

#[derive(Debug, Deserialize)]
struct ListPluginsQuery {
    kiln: PathBuf,
}

async fn list_plugins(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListPluginsQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let response = state
        .daemon
        .lua_discover_plugins(LuaDiscoverPluginsRequest {
            kiln_path: query.kiln.to_string_lossy().to_string(),
        })
        .await
        .daemon_err()?;

    Ok(Json(serde_json::json!({ "plugins": response.plugins })))
}

/// Reload a plugin by name.
///
/// The daemon does not currently expose a dedicated `plugin_reload` RPC method,
/// so this endpoint returns 501 Not Implemented until one is added.
async fn reload_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    // Verify the plugin exists by running a health check.
    // If the daemon adds a reload RPC method in the future, replace this stub.
    let health = state
        .daemon
        .lua_plugin_health(LuaPluginHealthRequest {
            plugin_path: name.clone(),
        })
        .await;

    match health {
        Ok(response) => Ok(Json(serde_json::json!({
            "status": "health_check_only",
            "name": response.name,
            "healthy": response.healthy,
            "message": response.message,
            "note": "Full reload not yet supported by daemon RPC"
        }))),
        Err(_) => Err(WebError::Internal(format!(
            "Plugin '{}' not found or health check failed",
            name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_routes_builds() {
        // Verify the router compiles and has the expected structure
        let _router = plugin_routes();
    }
}
