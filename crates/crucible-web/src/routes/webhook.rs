use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::post,
    Json, Router,
};
use std::collections::HashMap;

pub fn webhook_routes() -> Router<AppState> {
    Router::new().route("/api/webhook/{name}", post(handle_webhook))
}

async fn handle_webhook(
    State(state): State<AppState>,
    Path(name): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, WebError> {
    let header_map: HashMap<String, String> = headers
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();

    let result = state
        .daemon
        .webhook_receive(name, header_map, body)
        .await
        .daemon_err()?;

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_routes_builds() {
        let _router = webhook_routes();
    }
}
