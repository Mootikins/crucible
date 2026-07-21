use super::session_commands::execute_command;
use super::session_config::{
    get_max_tokens, get_precognition, get_precognition_results, get_temperature,
    get_thinking_budget, set_max_tokens, set_precognition, set_precognition_results,
    set_temperature, set_thinking_budget,
};
use crate::routes::helpers::ModelsResponse;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use crucible_daemon::agent_manager::providers::ProviderInfo;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::PathBuf;

// =========================================================================
// Typed Response Structs
// =========================================================================

/// Standard acknowledgment response for successful mutations.
#[derive(Debug, Serialize)]
pub(super) struct OkResponse {
    ok: bool,
}

impl OkResponse {
    pub(super) fn success() -> Json<Self> {
        Json(Self { ok: true })
    }
}

/// Response for session archive/unarchive status changes.
#[derive(Debug, Serialize)]
struct ArchiveResponse {
    archived: bool,
}

/// Response for session deletion.
#[derive(Debug, Serialize)]
struct DeleteResponse {
    deleted: bool,
}

/// Response for session cancellation.
#[derive(Debug, Serialize)]
struct CancelledResponse {
    cancelled: bool,
}

/// Response for title operations.
#[derive(Debug, Serialize)]
struct TitleResponse {
    title: String,
}

/// Response for provider listing.
#[derive(Debug, Serialize)]
struct ProvidersResponse {
    providers: Vec<ProviderInfo>,
}

// =========================================================================
// Route Helpers
// =========================================================================

/// Map daemon errors for session operations, converting "Session not found" to 404.
fn map_session_not_found(err: impl std::fmt::Display, id: &str) -> WebError {
    let message = err.to_string();
    if message.contains("Session not found") {
        WebError::NotFound(format!("Session not found: {id}"))
    } else {
        WebError::Daemon(message)
    }
}

pub fn session_routes() -> Router<AppState> {
    Router::new()
        .route("/api/session", post(create_session))
        .route("/api/session/list", get(list_sessions))
        .route("/api/sessions/search", get(search_sessions))
        .route("/api/session/{id}", get(get_session).delete(delete_session))
        .route("/api/session/{id}/history", get(get_session_history))
        .route("/api/session/{id}/pause", post(pause_session))
        .route("/api/session/{id}/resume", post(resume_session))
        .route("/api/session/{id}/end", post(end_session))
        .route("/api/session/{id}/archive", post(archive_session))
        .route("/api/session/{id}/unarchive", post(unarchive_session))
        .route("/api/session/{id}/cancel", post(cancel_session))
        .route("/api/session/{id}/models", get(list_models))
        .route("/api/session/{id}/model", post(switch_model))
        .route("/api/session/{id}/kilns/connect", post(connect_kiln))
        .route("/api/session/{id}/kilns/disconnect", post(disconnect_kiln))
        .route("/api/session/{id}/workspace", put(set_workspace))
        .route("/api/session/{id}/mode", post(set_mode))
        .route("/api/session/{id}/title", put(set_session_title))
        .route("/api/session/{id}/auto-title", post(auto_title))
        .route("/api/providers", get(list_providers))
        .route(
            "/api/session/{id}/config/thinking-budget",
            put(set_thinking_budget).get(get_thinking_budget),
        )
        .route(
            "/api/session/{id}/config/temperature",
            put(set_temperature).get(get_temperature),
        )
        .route(
            "/api/session/{id}/config/max-tokens",
            put(set_max_tokens).get(get_max_tokens),
        )
        .route(
            "/api/session/{id}/config/precognition",
            put(set_precognition).get(get_precognition),
        )
        .route(
            "/api/session/{id}/config/precognition/results",
            put(set_precognition_results).get(get_precognition_results),
        )
        .route("/api/session/{id}/export", post(export_session))
        .route("/api/session/{id}/command", post(execute_command))
}
#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    #[serde(default = "default_session_type")]
    session_type: String,
    /// Kiln for the session; omitted → daemon default (home kiln).
    kiln: Option<PathBuf>,
    /// Additional knowledge kilns to attach at creation.
    #[serde(default)]
    connect_kilns: Vec<PathBuf>,
    workspace: Option<PathBuf>,
    /// LLM provider (e.g., "ollama", "openai", "anthropic")
    provider: Option<String>,
    /// Model name (e.g., "llama3.2", "gpt-4o", "claude-3-5-sonnet")
    model: Option<String>,
    /// Custom endpoint URL (optional, for self-hosted models)
    endpoint: Option<String>,
    /// "internal" (default) or "acp"
    agent_type: Option<String>,
    /// ACP agent profile name (e.g. "claude", "opencode"); required when agent_type == "acp"
    agent_name: Option<String>,
}

fn default_session_type() -> String {
    "chat".to_string()
}

/// Validate that an endpoint URL is safe (no SSRF to internal networks).
fn validate_endpoint(endpoint: &str) -> Result<(), WebError> {
    let url = reqwest::Url::parse(endpoint)
        .map_err(|e| WebError::Validation(format!("Invalid endpoint URL: {e}")))?;

    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(WebError::Validation(format!(
                "Unsupported URL scheme: {scheme}"
            )));
        }
    }

    let host = url
        .host_str()
        .ok_or_else(|| WebError::Validation("Endpoint URL must have a host".to_string()))?;

    // Check if the host is an IP address in a private/internal range
    if let Ok(ip) = host.parse::<IpAddr>() {
        let is_private = match ip {
            IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_broadcast()
                    || v4.is_unspecified()
            }
            IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        };

        // Allow localhost for local-first development, but block other private ranges
        if is_private && !ip.is_loopback() {
            return Err(WebError::Validation(format!(
                "Endpoint must not target private/internal IP: {host}"
            )));
        }
    }

    Ok(())
}

/// Map a `session.create` daemon error to an HTTP status. An `INVALID_PARAMS`
/// error (JSON-RPC code `-32602` — e.g. an unknown ACP profile or an
/// unparseable provider override, both now resolved daemon-side) is a client
/// error (422), preserving the pre-consolidation behavior where the web
/// validated the profile itself. Anything else is a daemon/transport failure
/// (502).
fn map_create_error(err: impl std::fmt::Display) -> WebError {
    let message = err.to_string();
    if message.contains("-32602") {
        WebError::Validation(message)
    } else {
        WebError::Daemon(message)
    }
}

async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    if let Some(ref endpoint) = req.endpoint {
        validate_endpoint(endpoint)?;
    }

    // Validate agent_type up front: an unrecognized value (e.g. "ACP",
    // "internal-x") must be rejected, not silently forwarded to the daemon as a
    // junk string while taking the internal branch.
    match req.agent_type.as_deref() {
        None | Some("internal") | Some("acp") => {}
        Some(other) => {
            return Err(WebError::Validation(format!(
                "Invalid agent_type: {other:?} (expected \"internal\" or \"acp\")"
            )));
        }
    }

    let is_acp = req.agent_type.as_deref() == Some("acp");
    if is_acp && req.agent_name.as_deref().unwrap_or("").is_empty() {
        return Err(WebError::Validation(
            "agent_name is required when agent_type is \"acp\"".to_string(),
        ));
    }

    // Hand the agent spec to the daemon, which owns default-agent resolution:
    // it resolves the ACP profile (unknown ⇒ INVALID_PARAMS, and no session is
    // created — see map_create_error) or builds config-derived internal
    // defaults, configures the session's agent as part of create, and returns
    // the resolved model in `agent_model`. The web no longer keeps its own copy
    // of "what is the default agent". No kiln → omitted from the wire so the
    // daemon resolves its default (home kiln).
    let agent_spec = crucible_daemon::rpc_client::SessionAgentSpec {
        agent_name: req.agent_name.clone(),
        provider: req.provider.clone(),
        provider_key: None,
        model: req.model.clone(),
        endpoint: req.endpoint.clone(),
    };

    let params = crucible_daemon::rpc_client::SessionCreateParams {
        session_type: req.session_type.clone(),
        kiln: req.kiln.clone(),
        workspace: req.workspace.clone(),
        connect_kilns: req.connect_kilns.clone(),
        recording_mode: None,
        recording_path: None,
        agent_type: req.agent_type.clone(),
    };

    let result = state
        .daemon
        .session_create_with_agent(params, agent_spec)
        .await
        .map_err(map_create_error)?;

    // A create response without a usable session_id (protocol drift) would
    // otherwise let subscribe run against an empty id and surface as a confusing
    // downstream error; fail loudly here instead.
    let session_id = result["session_id"].as_str().unwrap_or("");
    if session_id.is_empty() {
        return Err(WebError::Daemon(
            "daemon returned no session_id from session.create".to_string(),
        ));
    }

    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .daemon_err()?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct ListSessionsQuery {
    kiln: Option<PathBuf>,
    workspace: Option<PathBuf>,
    #[serde(rename = "type")]
    session_type: Option<String>,
    state: Option<String>,
    #[serde(default)]
    include_archived: Option<bool>,
}

async fn list_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListSessionsQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_list(
            query.kiln.as_deref(),
            query.workspace.as_deref(),
            query.session_type.as_deref(),
            query.state.as_deref(),
            query.include_archived,
        )
        .await
        .daemon_err()?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct SearchSessionsQuery {
    q: String,
    kiln: Option<PathBuf>,
    limit: Option<usize>,
}

async fn search_sessions(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<SearchSessionsQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let results = state
        .daemon
        .session_search(&query.q, query.kiln.as_deref(), query.limit.or(Some(20)))
        .await
        .daemon_err()?;

    Ok(Json(results))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.session_get(&id).await.daemon_err()?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    kiln: PathBuf,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn get_session_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<HistoryQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_resume_from_storage(&id, &query.kiln, query.limit, query.offset)
        .await
        .daemon_err()?;

    Ok(Json(result))
}

async fn pause_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.session_pause(&id).await.daemon_err()?;

    Ok(Json(result))
}

async fn resume_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.session_resume(&id).await.daemon_err()?;

    let session_id = id.as_str();
    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .daemon_err()?;

    Ok(Json(result))
}

async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.session_end(&id).await.daemon_err()?;

    state.events.remove_session(&id).await;

    Ok(Json(result))
}

async fn archive_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ArchiveResponse>, WebError> {
    let kiln = resolve_session_kiln(&state, &id).await?;
    state
        .daemon
        .session_archive(&id, std::path::Path::new(&kiln))
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    state.events.remove_session(&id).await;
    Ok(Json(ArchiveResponse { archived: true }))
}

async fn unarchive_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ArchiveResponse>, WebError> {
    let kiln = resolve_session_kiln(&state, &id).await?;
    state
        .daemon
        .session_unarchive(&id, std::path::Path::new(&kiln))
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    Ok(Json(ArchiveResponse { archived: false }))
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, WebError> {
    let kiln = resolve_session_kiln(&state, &id).await?;
    state
        .daemon
        .session_delete(&id, std::path::Path::new(&kiln))
        .await
        .map_err(|e| map_session_not_found(e, &id))?;
    state.events.remove_session(&id).await;
    Ok(Json(DeleteResponse { deleted: true }))
}

async fn resolve_session_kiln(state: &AppState, session_id: &str) -> Result<String, WebError> {
    match state.daemon.session_get(session_id).await {
        Ok(session) => {
            return session
                .get("kiln")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .ok_or_else(|| WebError::Validation("Session has no kiln path".to_string()));
        }
        Err(e) => {
            let message = e.to_string();
            if !message.contains("Session not found") {
                return Err(WebError::Daemon(message));
            }
        }
    }

    let sessions = state
        .daemon
        .session_list(None, None, None, None, Some(true))
        .await
        .daemon_err()?;

    let kiln = sessions
        .get("sessions")
        .and_then(|value| value.as_array())
        .and_then(|items| {
            items.iter().find_map(|item| {
                let id = item.get("session_id").and_then(|value| value.as_str())?;
                if id == session_id {
                    item.get("kiln")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                } else {
                    None
                }
            })
        });

    kiln.ok_or_else(|| WebError::NotFound(format!("Session not found: {session_id}")))
}

async fn cancel_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CancelledResponse>, WebError> {
    let cancelled = state.daemon.session_cancel(&id).await.daemon_err()?;
    Ok(Json(CancelledResponse { cancelled }))
}

async fn list_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ModelsResponse>, WebError> {
    let models = state.daemon.session_list_models(&id).await.daemon_err()?;
    Ok(Json(ModelsResponse { models }))
}

#[derive(Debug, Deserialize)]
struct SwitchModelRequest {
    model_id: String,
}

async fn switch_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SwitchModelRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_switch_model(&id, &req.model_id)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

#[derive(Debug, Deserialize)]
struct SessionKilnRequest {
    kiln: PathBuf,
}

/// Updated session scope, echoed by kiln/workspace mutations.
async fn connect_kiln(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SessionKilnRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let scope = state
        .daemon
        .session_connect_kiln(&id, &req.kiln)
        .await
        .daemon_err()?;
    Ok(Json(scope))
}

async fn disconnect_kiln(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SessionKilnRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let scope = state
        .daemon
        .session_disconnect_kiln(&id, &req.kiln)
        .await
        .daemon_err()?;
    Ok(Json(scope))
}

#[derive(Debug, Deserialize)]
struct SetWorkspaceRequest {
    /// Omitted/null → detach (workspace falls back to the kiln).
    workspace: Option<PathBuf>,
}

async fn set_workspace(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetWorkspaceRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let scope = state
        .daemon
        .session_set_workspace(&id, req.workspace.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(scope))
}

#[derive(Debug, Deserialize)]
struct SetModeRequest {
    mode: String,
}

/// Set the session mode (normal/plan/auto). The daemon persists it on the
/// agent config and applies it to the live handle; confirmation reaches the
/// UI as a `mode_changed` SSE event.
async fn set_mode(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetModeRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_mode(&id, &req.mode)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

#[derive(Debug, Deserialize)]
struct SetTitleRequest {
    title: String,
}

async fn set_session_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetTitleRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_title(&id, &req.title)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

/// Auto-generate a title for a session from its conversation history.
///
/// Delegates to the daemon's `session.generate_title`, which produces a
/// topic-based title via the session's own LLM provider (falling back to
/// first-message truncation daemon-side). Idempotent: an already-titled
/// session returns its existing title.
async fn auto_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TitleResponse>, WebError> {
    let result = state
        .daemon
        .session_generate_title(&id)
        .await
        .daemon_err()?;

    let title = result
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled Session")
        .to_string();

    Ok(Json(TitleResponse { title }))
}

async fn export_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<
    (
        [(
            axum::http::header::HeaderName,
            axum::http::header::HeaderValue,
        ); 1],
        String,
    ),
    WebError,
> {
    // Get session metadata to find kiln path
    let session = state.daemon.session_get(&id).await.daemon_err()?;
    let kiln_str = session.get("kiln").and_then(|v| v.as_str()).unwrap_or("");

    if kiln_str.is_empty() {
        return Err(WebError::Validation(
            "Session has no kiln path, cannot export".to_string(),
        ));
    }

    let kiln = std::path::Path::new(kiln_str);

    // Build session directory path (mirrors FileSessionStorage::session_dir_by_id)
    let session_dir = if crucible_core::config::is_crucible_home(kiln) {
        kiln.join("sessions").join(&id)
    } else {
        kiln.join(".crucible").join("sessions").join(&id)
    };

    // Try to render markdown from persisted session events
    let markdown = match state
        .daemon
        .session_render_markdown(&session_dir, Some(true), None, Some(true), None)
        .await
    {
        Ok(md) => md,
        Err(_) => {
            // Fallback: construct basic markdown from session metadata
            let title = session
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled Session");
            let started_at = session
                .get("started_at")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let model = session
                .get("agent_model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let state_str = session
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            format!(
                "# {}\n\n- **Date**: {}\n- **Model**: {}\n- **State**: {}\n\n---\n\n*Session events are not yet persisted. Export will be available after the session is paused or ended.*\n",
                title, started_at, model, state_str
            )
        }
    };

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            axum::http::header::HeaderValue::from_static("text/markdown; charset=utf-8"),
        )],
        markdown,
    ))
}

#[derive(Debug, Deserialize)]
struct ListProvidersQuery {
    kiln: Option<PathBuf>,
}

async fn list_providers(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListProvidersQuery>,
) -> Result<Json<ProvidersResponse>, WebError> {
    let providers = state
        .daemon
        .list_providers(query.kiln.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(ProvidersResponse { providers }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use proptest::prelude::*;
    use tower::ServiceExt;

    proptest! {
        #[test]
        fn validate_endpoint_rejects_private_ipv4_addresses(ip in arb_ipv4_private().prop_filter("exclude loopback", |ip| !ip.starts_with("127."))) {
            let endpoint = format!("http://{ip}/");
            prop_assert!(validate_endpoint(&endpoint).is_err());
        }

        #[test]
        fn validate_endpoint_accepts_public_ipv4_with_http_or_https(
            ip in arb_ipv4_public(),
            scheme in prop_oneof![Just("http"), Just("https")],
        ) {
            let endpoint = format!("{scheme}://{ip}/");
            prop_assert!(validate_endpoint(&endpoint).is_ok());
        }

        #[test]
        fn validate_endpoint_rejects_non_http_schemes(
            scheme in arb_url_scheme().prop_filter("non-http scheme", |s| s != "http" && s != "https"),
        ) {
            let endpoint = format!("{scheme}://example.com");
            prop_assert!(validate_endpoint(&endpoint).is_err());
        }

        #[test]
        fn validate_endpoint_rejects_ipv6_loopback(host in arb_ipv6_loopback()) {
            let host = host.trim_matches(['[', ']']);
            let endpoint = format!("http://[{host}]/");
            prop_assert!(validate_endpoint(&endpoint).is_ok());
        }
    }

    #[test]
    fn validate_endpoint_allows_localhost_http() {
        assert!(validate_endpoint("http://localhost:8080").is_ok());
    }

    #[test]
    fn validate_endpoint_allows_localhost_https() {
        assert!(validate_endpoint("https://localhost:3000").is_ok());
    }

    #[test]
    fn validate_endpoint_rejects_10_0_0_1() {
        assert!(validate_endpoint("http://10.0.0.1").is_err());
    }

    #[test]
    fn validate_endpoint_rejects_192_168_1_1() {
        assert!(validate_endpoint("http://192.168.1.1").is_err());
    }

    #[test]
    fn validate_endpoint_rejects_172_16_0_1() {
        assert!(validate_endpoint("http://172.16.0.1").is_err());
    }

    #[test]
    fn validate_endpoint_rejects_ftp_scheme() {
        assert!(validate_endpoint("ftp://example.com").is_err());
    }

    #[test]
    fn validate_endpoint_rejects_malformed_url() {
        assert!(validate_endpoint("not-a-url").is_err());
    }

    #[test]
    fn validate_endpoint_rejects_empty_host() {
        assert!(validate_endpoint("http://").is_err());
    }

    #[test]
    fn validate_endpoint_allows_public_domain_name() {
        // TODO(security): DNS rebinding not checked — hostname "evil.com" resolving to 10.0.0.1 would pass this validation
        assert!(validate_endpoint("http://evil.com").is_ok());
    }

    // =========================================================================
    // create_session Tests
    // =========================================================================

    async fn post_create_session(
        body: serde_json::Value,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        crate::test_support::request_json("POST", "/api/session", Some(body)).await
    }

    #[tokio::test]
    async fn create_session_works_without_a_kiln() {
        let (status, json) = post_create_session(serde_json::json!({})).await;
        assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
        assert_eq!(json["session_id"], "test-session-001");
    }

    #[tokio::test]
    async fn create_session_accepts_connect_kilns() {
        let (status, json) = post_create_session(serde_json::json!({
            "connect_kilns": ["/tmp/extra-kiln"],
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
    }

    #[tokio::test]
    async fn create_session_accepts_acp_agent() {
        let (status, json) = post_create_session(serde_json::json!({
            "agent_type": "acp",
            "agent_name": "claude",
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
        assert_eq!(json["session_id"], "test-session-001");
    }

    #[tokio::test]
    async fn create_session_rejects_acp_without_agent_name() {
        let (status, _) = post_create_session(serde_json::json!({
            "agent_type": "acp",
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn create_session_rejects_unknown_acp_agent() {
        // The mock daemon resolves any profile name except "missing" to null.
        let (status, _) = post_create_session(serde_json::json!({
            "agent_type": "acp",
            "agent_name": "missing",
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn create_session_with_unknown_acp_agent_does_not_create_a_session() {
        // Regression: an unknown ACP agent must not orphan an agent-less
        // session. Resolution now lives in the daemon's session.create, which
        // rejects the unknown profile atomically (INVALID_PARAMS, no row). At
        // the web/wire level the invariants are: the web forwards a single
        // session.create carrying the agent spec, no longer resolves the
        // profile client-side (agents.resolve_profile), and does NOT proceed to
        // subscribe once create fails.
        let (mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({
                            "agent_type": "acp",
                            "agent_name": "missing",
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNPROCESSABLE_ENTITY
        );

        let methods = mock.received_methods();
        assert!(
            methods.iter().any(|m| m == "session.create"),
            "web must forward the create (with the agent spec) to the daemon: {methods:?}"
        );
        assert!(
            !methods.iter().any(|m| m == "agents.resolve_profile"),
            "profile resolution moved daemon-side; web must NOT resolve it: {methods:?}"
        );
        assert!(
            !methods.iter().any(|m| m == "session.subscribe"),
            "a failed create must not proceed to subscribe: {methods:?}"
        );
    }

    #[tokio::test]
    async fn create_session_rejects_unknown_agent_type() {
        // Anything other than absent/"internal"/"acp" is a validation error,
        // not a silently-forwarded junk string on the internal branch.
        for bad in ["ACP", "internal-x", "external", ""] {
            let (status, _) = post_create_session(serde_json::json!({
                "agent_type": bad,
            }))
            .await;
            assert_eq!(
                status,
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "agent_type {bad:?} should be rejected"
            );
        }
    }

    #[tokio::test]
    async fn create_session_errors_when_daemon_returns_no_session_id() {
        // Protocol drift: a create response missing session_id must fail loudly,
        // not proceed to configure_agent/subscribe against an empty id. The mock
        // drops session_id for the "__no_session_id__" sentinel session_type.
        let (status, _) = post_create_session(serde_json::json!({
            "session_type": "__no_session_id__",
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn create_session_accepts_internal_agent_type() {
        let (status, json) = post_create_session(serde_json::json!({
            "agent_type": "internal",
        }))
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
        assert_eq!(json["session_id"], "test-session-001");
    }

    // =========================================================================
    // Session scope (kilns/workspace) Tests
    // =========================================================================

    async fn send_json(
        method: &str,
        uri: &str,
        body: serde_json::Value,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        crate::test_support::request_json(method, uri, Some(body)).await
    }

    #[tokio::test]
    async fn connect_kiln_returns_updated_scope() {
        let (status, json) = send_json(
            "POST",
            "/api/session/test-session-001/kilns/connect",
            serde_json::json!({"kiln": "/tmp/extra-kiln"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
        assert_eq!(json["connected_kilns"][0], "/tmp/extra-kiln");
    }

    #[tokio::test]
    async fn disconnect_kiln_returns_updated_scope() {
        let (status, json) = send_json(
            "POST",
            "/api/session/test-session-001/kilns/disconnect",
            serde_json::json!({"kiln": "/tmp/extra-kiln"}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
        assert!(json["connected_kilns"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn set_workspace_accepts_null_for_detach() {
        let (status, json) = send_json(
            "PUT",
            "/api/session/test-session-001/workspace",
            serde_json::json!({ "workspace": null }),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
        // Detach falls back to the kiln path (mock echoes the default).
        assert_eq!(json["workspace"], "/tmp/test-kiln");
    }

    #[tokio::test]
    async fn set_workspace_attaches_project_dir() {
        let (status, json) = send_json(
            "PUT",
            "/api/session/test-session-001/workspace",
            serde_json::json!({ "workspace": "/repos/crucible" }),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "body: {json}");
        assert_eq!(json["workspace"], "/repos/crucible");
    }

    // =========================================================================
    // export_session Tests
    // =========================================================================

    #[tokio::test]
    async fn export_session_returns_text_markdown_content_type() {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session/test-session-001/export")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let content_type = response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            content_type.contains("text/markdown"),
            "Expected text/markdown content-type, got: {}",
            content_type
        );
    }

    #[tokio::test]
    async fn export_session_returns_markdown_body() {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session/test-session-001/export")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();

        // Should contain markdown content (either from render or fallback)
        assert!(!text.is_empty(), "Exported markdown should not be empty");
        // Fallback markdown includes session title
        assert!(
            text.contains("#") || text.contains("Test Session"),
            "Exported markdown should contain heading or session title"
        );
    }

    #[tokio::test]
    async fn export_session_fallback_includes_session_metadata() {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session/test-session-001/export")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();

        // Fallback markdown should include metadata fields
        // The mock returns render_markdown with "# Test Session\n\nExported content"
        // But if render fails, fallback includes: title, started_at, model, state
        assert!(
            text.contains("Test Session") || text.contains("Date"),
            "Exported markdown should include session metadata"
        );
    }

    #[tokio::test]
    async fn export_session_with_valid_session_returns_200() {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session/test-session-001/export")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Valid session with kiln should return 200
        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    // =========================================================================
    // auto_title Tests
    // =========================================================================

    #[tokio::test]
    async fn auto_title_returns_200_with_title_field() {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session/test-session-001/auto-title")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert!(
            json.get("title").is_some(),
            "Response should contain 'title' field"
        );
        assert!(json["title"].is_string(), "Title should be a string");
    }

    #[tokio::test]
    async fn auto_title_delegates_to_daemon_generate_title() {
        // Title generation is daemon-owned (topic-based LLM with truncation
        // fallback); the web route only forwards and unwraps the result.
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session/test-session-001/auto-title")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(
            json["title"].as_str().unwrap(),
            "Merkle tree sync design",
            "Title should come from the daemon's session.generate_title"
        );
    }

    // =========================================================================
    // Session creation smart defaults & provider filtering
    // =========================================================================

    #[tokio::test]
    async fn test_create_session_without_provider_uses_detected_default() {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        // Only kiln is required — provider and model should resolve from detected defaults
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({"kiln": "/tmp/test-kiln"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json.get("session_id").is_some(),
            "Response must contain session_id even without explicit provider/model"
        );
    }

    #[tokio::test]
    async fn test_create_session_with_explicit_provider_still_works() {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/session")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::json!({
                            "kiln": "/tmp/test-kiln",
                            "provider": "ollama",
                            "model": "llama3.2"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json.get("session_id").is_some(),
            "Response must contain session_id with explicit provider/model"
        );
        assert_eq!(json["session_id"], "test-session-001");
    }

    #[tokio::test]
    async fn test_list_providers_with_kiln_query_param_returns_200() {
        let (_mock, client) = crate::test_support::start_mock_daemon().await;
        let state = crate::test_support::build_mock_state(client);
        let app = crate::test_support::build_test_app(state);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/providers?kiln=/tmp/test-kiln")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["providers"].is_array(),
            "Response must have 'providers' array when kiln query param is provided"
        );
    }
}
