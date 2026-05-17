use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};

pub fn plugin_routes() -> Router<AppState> {
    Router::new()
        .route("/api/plugins", get(list_plugins))
        .route("/api/plugins/{name}/reload", post(reload_plugin))
}

/// `GET /api/plugins` — list discovered plugins with rich metadata
/// (name, version, source, state, dir, capability counts).
async fn list_plugins(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_routes_builds() {
        let _router = plugin_routes();
    }
}
