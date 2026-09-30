use crate::routes::helpers::ModelsResponse;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{extract::State, Json};
use crucible_daemon::AgentProfileEntry;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

/// The route group. `routes!` carries the path from the `#[utoipa::path]`
/// attribute beside the handler, so the path is written once.
pub fn agents_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_agents))
        .routes(routes!(list_all_models))
}

/// The agent picker's list.
///
/// The key is `agents`, where the daemon's own answer says `profiles`: this
/// route names the list after what the picker shows, and the ROW is the
/// daemon's, so a probed field cannot be dropped on the way through.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
struct AgentListResponse {
    agents: Vec<AgentProfileEntry>,
}

/// Read the `profiles` array out of the daemon's `agents.list_profiles`
/// answer, failing safe to an empty list. A missing, non-array or unreadable
/// key must not leak `null` to the client — the picker expects `agents` to be
/// iterable.
/// ACP agent profiles with probed availability, for the session-creation
/// agent picker.
///
/// Cached in the daemon (`agents.list_profiles`,
/// `crate::agent_manager::CATALOG_CACHE_TTL`) — the probe takes ~0.5s and
/// must not gate every splash render. Every caller of that RPC method shares
/// the cache now; it used to be a cache in this crate alone.
#[utoipa::path(
    get,
    path = "/api/agents",
    responses(
        (status = 200, body = AgentListResponse),
        (status = 502, description = "The daemon could not list the agent profiles"),
    )
)]
async fn list_agents(State(state): State<AppState>) -> Result<Json<AgentListResponse>, WebError> {
    let result = state.daemon.agents_list_profiles().await.daemon_err()?;
    Ok(Json(AgentListResponse {
        agents: result.profiles,
    }))
}

/// All chat models across providers, no session required (draft-state picker).
///
/// Takes no `kiln` parameter, for the reason `list_providers` does not: it
/// used to accept a raw `PathBuf` that reached the daemon's classification
/// resolver unfloored, and no caller ever sent one.
#[utoipa::path(
    get,
    path = "/api/models",
    responses((status = 200, body = ModelsResponse))
)]
async fn list_all_models(State(state): State<AppState>) -> Result<Json<ModelsResponse>, WebError> {
    let models = state.daemon.list_all_models(None).await.daemon_err()?;
    Ok(Json(ModelsResponse { models }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::request_json;

    /// The daemon builds the rows, this route re-keys the envelope, and the
    /// body deserialises back into the declared type. A row that lost the
    /// probe verdict on the way through fails here.
    #[tokio::test]
    async fn list_agents_answers_the_declared_shape() {
        let (status, json) = request_json("GET", "/api/agents", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let parsed: AgentListResponse =
            serde_json::from_value(json.clone()).unwrap_or_else(|e| panic!("{e}: {json}"));
        assert_eq!(parsed.agents.len(), 2);
        assert_eq!(parsed.agents[0].name, "claude");
        assert!(!parsed.agents[0].available);
        assert_eq!(parsed.agents[1].name, "opencode");
        assert!(parsed.agents[1].available);
        assert!(parsed.agents[0].is_builtin);
    }

    #[tokio::test]
    async fn list_all_models_works_without_a_session() {
        let (status, json) = request_json("GET", "/api/models", None).await;
        assert_eq!(status, axum::http::StatusCode::OK);

        let models = json["models"].as_array().expect("models array");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0], "ollama/llama3.2");
    }
}
