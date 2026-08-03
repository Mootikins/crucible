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
        .route("/api/plugins/publications", get(list_publications))
        .route("/api/plugins/options", get(list_options))
        .route("/api/plugins/{name}/option", post(option_call))
        .route("/api/plugins/command", post(run_command))
}

/// One plugin command invocation.
#[derive(Debug, Deserialize)]
struct CommandRequest {
    name: String,
    #[serde(default)]
    args: serde_json::Value,
}

/// One read, write, or button press against a plugin's settings tree.
///
/// A single endpoint because the three differ only in which Lua callback they
/// reach — the same reason the daemon handler is one function. `value` is
/// present for a set and ignored otherwise.
#[derive(Debug, Deserialize)]
struct OptionRequest {
    action: OptionAction,
    path: Vec<String>,
    #[serde(default)]
    value: serde_json::Value,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum OptionAction {
    Get,
    Set,
    Execute,
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

/// `GET /api/plugins/publications` — what plugins published about themselves.
///
/// Passed through verbatim, keyed `key -> plugin -> value`. Nothing here
/// interprets a value, which is the point: the frontend used to learn what
/// isolation a box offered by having the server match on the shape of the `oci`
/// plugin's config, so one plugin's schema lived in the rendering layer and a
/// second isolating plugin would not have appeared at all.
async fn list_publications(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
    let publications = state.daemon.plugin_publications().await.daemon_err()?;
    Ok(Json(serde_json::json!({ "publications": publications })))
}

/// `GET /api/plugins/options` — the settings trees plugins declared.
///
/// Passed through verbatim, keyed by plugin. Nothing here knows what any
/// option means: a node is `{type, name, desc, order, …}` and the renderer
/// draws it from that, so a plugin shipped tomorrow gets a settings pane with
/// no change on this side. Re-read rather than cached — a tree's
/// function-valued fields describe the box as it is now.
async fn list_options(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let options = state.daemon.plugin_options().await.daemon_err()?;
    Ok(Json(serde_json::json!({ "options": options })))
}

/// `POST /api/plugins/:name/option` — read, write, or press one option.
async fn option_call(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<OptionRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    if req.path.is_empty() {
        return Err(WebError::Validation(
            "`path` must name an option".to_string(),
        ));
    }
    match req.action {
        OptionAction::Get => {
            let value = state
                .daemon
                .plugin_option_get(&name, req.path)
                .await
                .daemon_err()?;
            Ok(Json(serde_json::json!({ "value": value })))
        }
        OptionAction::Set => {
            state
                .daemon
                .plugin_option_set(&name, req.path, req.value)
                .await
                .daemon_err()?;
            Ok(Json(serde_json::json!({ "ok": true })))
        }
        OptionAction::Execute => {
            state
                .daemon
                .plugin_option_execute(&name, req.path)
                .await
                .daemon_err()?;
            Ok(Json(serde_json::json!({ "ok": true })))
        }
    }
}

/// `POST /api/plugins/command` — invoke a plugin command by name.
///
/// Not under `/{name}` because a command's name is already globally unique —
/// the daemon refuses a second plugin claiming one — so routing by plugin would
/// ask the caller for something it does not need to know. The result is passed
/// through verbatim, like publications and options: what a command returns is
/// the plugin's vocabulary, and a shape this layer validated would be a shape
/// only today's plugins could send.
async fn run_command(
    State(state): State<AppState>,
    Json(req): Json<CommandRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    if req.name.trim().is_empty() {
        return Err(WebError::Validation(
            "`name` must name a command".to_string(),
        ));
    }
    let result = state
        .daemon
        .plugin_run_command(&req.name, req.args)
        .await
        .daemon_err()?;
    Ok(Json(result))
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
