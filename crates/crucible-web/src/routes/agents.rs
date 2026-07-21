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

/// Extract the `profiles` array from the daemon's `agents.list_profiles`
/// response, failing safe to an empty array. A missing/non-array key must not
/// leak `null` to the client — the picker expects `agents` to be iterable.
fn agents_from_profiles(result: &serde_json::Value) -> serde_json::Value {
    match result.get("profiles") {
        Some(profiles) if profiles.is_array() => profiles.clone(),
        _ => serde_json::Value::Array(Vec::new()),
    }
}

/// ACP agent profiles with probed availability, for the session-creation
/// agent picker. Shape: `{agents: [{name, description, command, is_builtin, available}]}`.
async fn list_agents(State(state): State<AppState>) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.agents_list_profiles().await.daemon_err()?;
    let agents = agents_from_profiles(&result);
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
    use super::*;
    use crate::test_support::request_json;

    #[tokio::test]
    async fn list_agents_returns_profiles_with_availability() {
        let (status, json) = request_json("GET", "/api/agents", None).await;
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
        let (status, json) = request_json("GET", "/api/models", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let models = json["models"].as_array().expect("models array");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0], "ollama/llama3.2");
    }

    #[test]
    fn agents_from_profiles_extracts_the_array() {
        let result = serde_json::json!({
            "profiles": [{"name": "claude"}, {"name": "opencode"}]
        });
        let agents = agents_from_profiles(&result);
        assert!(agents.is_array());
        assert_eq!(agents.as_array().unwrap().len(), 2);
    }

    #[test]
    fn agents_from_profiles_fails_safe_to_empty_array_when_key_missing() {
        // A profiles-less payload must yield `[]`, not `null` — the picker
        // iterates `agents` and would break on a null.
        let result = serde_json::json!({ "something_else": true });
        let agents = agents_from_profiles(&result);
        assert_eq!(agents, serde_json::json!([]));
    }

    #[test]
    fn agents_from_profiles_fails_safe_when_profiles_is_not_an_array() {
        let result = serde_json::json!({ "profiles": "oops" });
        let agents = agents_from_profiles(&result);
        assert_eq!(agents, serde_json::json!([]));
    }
}
