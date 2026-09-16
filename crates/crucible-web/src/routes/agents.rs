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
fn agents_from_profiles(result: &serde_json::Value) -> Vec<AgentProfileEntry> {
    result
        .get("profiles")
        .and_then(|profiles| serde_json::from_value(profiles.clone()).ok())
        .unwrap_or_default()
}

/// ACP agent profiles with probed availability, for the session-creation
/// agent picker.
///
/// Served through the SWR catalog cache — the daemon probe takes ~0.5s and
/// must not gate every splash render.
#[utoipa::path(
    get,
    path = "/api/agents",
    responses(
        (status = 200, body = AgentListResponse),
        (status = 502, description = "The daemon could not list the agent profiles"),
    )
)]
async fn list_agents(State(state): State<AppState>) -> Result<Json<AgentListResponse>, WebError> {
    let result = crate::services::catalog::agents_value(&state)
        .await
        .daemon_err()?;
    Ok(Json(AgentListResponse {
        agents: agents_from_profiles(&result),
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

    /// A round trip through the row type the daemon owns.
    #[test]
    fn agents_from_profiles_reads_the_daemon_rows() {
        let daemon = crucible_daemon::AgentProfilesReply {
            profiles: vec![AgentProfileEntry {
                name: "claude".to_string(),
                description: "Claude Code via ACP".to_string(),
                command: "npx".to_string(),
                is_builtin: true,
                available: false,
            }],
        };
        let wire = serde_json::to_value(&daemon).expect("the daemon reply serialises");
        assert_eq!(agents_from_profiles(&wire), daemon.profiles);
    }

    #[test]
    fn agents_from_profiles_fails_safe_to_an_empty_list_when_key_missing() {
        // A profiles-less payload must yield `[]`, not `null` — the picker
        // iterates `agents` and would break on a null.
        let result = serde_json::json!({ "something_else": true });
        assert!(agents_from_profiles(&result).is_empty());
    }

    #[test]
    fn agents_from_profiles_fails_safe_when_profiles_is_not_an_array() {
        let result = serde_json::json!({ "profiles": "oops" });
        assert!(agents_from_profiles(&result).is_empty());
    }

    /// A row the daemon writes and this crate cannot read is an empty picker,
    /// not a 502: the catalog answer is cached and a partial list is worse
    /// than none.
    #[test]
    fn agents_from_profiles_fails_safe_when_a_row_is_unreadable() {
        let result = serde_json::json!({ "profiles": [{ "name": "claude" }] });
        assert!(agents_from_profiles(&result).is_empty());
    }
}
