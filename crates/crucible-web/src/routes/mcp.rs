use crate::services::daemon::AppState;
use crate::error::WebResultExt;
use crate::WebError;
use axum::{
    extract::State,
    routing::get,
    Json, Router,
};

pub fn mcp_routes() -> Router<AppState> {
    Router::new().route("/api/mcp/status", get(mcp_status))
}

async fn mcp_status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.mcp_status().await.daemon_err()?;

    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_routes_builds() {
        let _router = mcp_routes();
    }
}
