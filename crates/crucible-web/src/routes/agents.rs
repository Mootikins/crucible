use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn agents_routes() -> Router<AppState> {
    Router::new()
        .route("/api/agents", get(list_agents))
        .route("/api/models", get(list_all_models))
}

/// ACP agent profiles with probed availability, for the session-creation
/// agent picker. Shape: `{agents: [{name, description, command, is_builtin, available}]}`.
async fn list_agents(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.agents_list_profiles().await.daemon_err()?;
    let agents = result.get("profiles").cloned().unwrap_or_default();
    Ok(Json(serde_json::json!({ "agents": agents })))
}

#[derive(Debug, Deserialize)]
struct ListModelsQuery {
    /// Optional kiln path — filters providers by the kiln's data classification.
    kiln: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct AllModelsResponse {
    models: Vec<String>,
}

/// All chat models across providers, no session required (draft-state picker).
async fn list_all_models(
    State(state): State<AppState>,
    Query(query): Query<ListModelsQuery>,
) -> Result<Json<AllModelsResponse>, WebError> {
    let models = state
        .daemon
        .list_all_models(query.kiln.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(AllModelsResponse { models }))
}

#[cfg(test)]
mod tests {
    use tower::ServiceExt;

    async fn get_json(uri: &str) -> (axum::http::StatusCode, serde_json::Value) {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn list_agents_returns_profiles_with_availability() {
        let (status, json) = get_json("/api/agents").await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let agents = json["agents"].as_array().expect("agents array");
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0]["name"], "claude");
        assert_eq!(agents[0]["available"], false);
        assert_eq!(agents[1]["name"], "opencode");
        assert_eq!(agents[1]["available"], true);
    }

    #[tokio::test]
    async fn list_all_models_works_without_a_session() {
        let (status, json) = get_json("/api/models").await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let models = json["models"].as_array().expect("models array");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0], "ollama/llama3.2");
    }
}
