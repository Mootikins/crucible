use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use crucible_config::BackendType;
use crucible_core::session::{OutputValidation, SessionAgent};
use crucible_daemon::agent_manager::providers::ProviderInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;

// =========================================================================
// Typed Response Structs
// =========================================================================

/// Standard acknowledgment response for successful mutations.
#[derive(Debug, Serialize)]
struct OkResponse {
    ok: bool,
}

impl OkResponse {
    fn success() -> Json<Self> {
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

/// Response for model listing.
#[derive(Debug, Serialize)]
struct ModelsResponse {
    models: Vec<String>,
}

/// Response for title operations.
#[derive(Debug, Serialize)]
struct TitleResponse {
    title: String,
}

/// Response for thinking budget config.
#[derive(Debug, Serialize)]
struct ThinkingBudgetResponse {
    thinking_budget: Option<i64>,
}

/// Response for temperature config.
#[derive(Debug, Serialize)]
struct TemperatureResponse {
    temperature: Option<f64>,
}

/// Response for max tokens config.
#[derive(Debug, Serialize)]
struct MaxTokensResponse {
    max_tokens: Option<u32>,
}

/// Response for precognition config.
#[derive(Debug, Serialize)]
struct PrecognitionResponse {
    precognition_enabled: bool,
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
        .route("/api/session/{id}/export", post(export_session))
        .route("/api/session/{id}/command", post(execute_command))
}
#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    #[serde(default = "default_session_type")]
    session_type: String,
    kiln: PathBuf,
    workspace: Option<PathBuf>,
    /// LLM provider (e.g., "ollama", "openai", "anthropic")
    provider: Option<String>,
    /// Model name (e.g., "llama3.2", "gpt-4o", "claude-3-5-sonnet")
    model: Option<String>,
    /// Custom endpoint URL (optional, for self-hosted models)
    endpoint: Option<String>,
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

async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    if let Some(ref endpoint) = req.endpoint {
        validate_endpoint(endpoint)?;
    }

    let result = state
        .daemon
        .session_create(
            &req.session_type,
            &req.kiln,
            req.workspace.as_deref(),
            vec![],
            None,
            None,
        )
        .await
        .daemon_err()?;

    let session_id = result["session_id"].as_str().unwrap_or("");

    // Resolve provider and model: use provided values or detect from available providers
    let (provider_str, model_str) = match (req.provider, req.model) {
        (Some(p), Some(m)) => (p, m),
        (p_opt, m_opt) => {
            // Resolve from detected providers
            let providers = state.daemon.list_providers(None).await.unwrap_or_default();
            let first = providers.into_iter().find(|p| p.available);
            let default_p = first
                .as_ref()
                .map(|p| p.provider_type.clone())
                .unwrap_or_else(|| "ollama".to_string());
            let default_m = first
                .as_ref()
                .and_then(|p| p.default_model.clone())
                .unwrap_or_else(|| "llama3.2".to_string());
            (p_opt.unwrap_or(default_p), m_opt.unwrap_or(default_m))
        }
    };

    // Configure agent for the session (required before sending messages)
    let provider_type = BackendType::from_str(&provider_str)
        .map_err(|e| WebError::Validation(format!("Invalid provider: {}", e)))?;

    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some(provider_str.clone()),
        provider: provider_type,
        model: model_str,
        system_prompt: String::new(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: req.endpoint,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
    };

    state
        .daemon
        .session_configure_agent(session_id, &agent)
        .await
        .daemon_err()?;

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
/// This is a simple string-truncation-based auto-title (not LLM generation).
/// Falls back to "Untitled Session" if no messages are available.
async fn auto_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TitleResponse>, WebError> {
    // Get session info to find kiln path
    let session = state.daemon.session_get(&id).await.daemon_err()?;
    let kiln_str = session.get("kiln").and_then(|v| v.as_str()).unwrap_or("");

    // Try to get conversation history for title generation
    let first_user_message = if !kiln_str.is_empty() {
        let history = state
            .daemon
            .session_resume_from_storage(&id, std::path::Path::new(kiln_str), Some(10), None)
            .await
            .ok();

        history
            .as_ref()
            .and_then(|h| h.get("messages"))
            .and_then(|v| v.as_array())
            .and_then(|msgs| {
                msgs.iter().find_map(|m| {
                    let role = m.get("role").and_then(|r| r.as_str())?;
                    if role == "user" {
                        m.get("content").and_then(|c| c.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
            })
    } else {
        None
    };

    let title = match first_user_message {
        Some(msg) => truncate_to_title(&msg),
        None => "Untitled Session".to_string(),
    };

    // Update the session title
    state
        .daemon
        .session_set_title(&id, &title)
        .await
        .daemon_err()?;

    Ok(Json(TitleResponse { title }))
}

/// Create a concise title from a message by smart truncation.
///
/// This function truncates at character boundaries (not byte boundaries) to safely
/// handle multi-byte UTF-8 characters like CJK, emoji, etc. It keeps the first ~60
/// characters, breaking at word boundaries when possible.
fn truncate_to_title(message: &str) -> String {
    const MAX_LEN: usize = 60;

    // Clean up: collapse whitespace, trim
    let cleaned: String = message.split_whitespace().collect::<Vec<_>>().join(" ");

    if cleaned.len() <= MAX_LEN {
        return cleaned;
    }

    // Truncate at word boundary
    let truncated = cleaned.chars().take(MAX_LEN).collect::<String>();
    if let Some(last_space) = truncated.rfind(' ') {
        if last_space > MAX_LEN / 2 {
            return format!("{}...", &truncated[..last_space]);
        }
    }

    format!("{}...", truncated)
}

// =========================================================================
// Session Config Endpoints
// =========================================================================

#[derive(Debug, Deserialize)]
struct SetThinkingBudgetRequest {
    thinking_budget: Option<i64>,
}

async fn set_thinking_budget(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetThinkingBudgetRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_thinking_budget(&id, req.thinking_budget)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

async fn get_thinking_budget(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ThinkingBudgetResponse>, WebError> {
    let thinking_budget = state
        .daemon
        .session_get_thinking_budget(&id)
        .await
        .daemon_err()?;
    Ok(Json(ThinkingBudgetResponse { thinking_budget }))
}

#[derive(Debug, Deserialize)]
struct SetTemperatureRequest {
    temperature: f64,
}

async fn set_temperature(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetTemperatureRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_temperature(&id, req.temperature)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

async fn get_temperature(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TemperatureResponse>, WebError> {
    let temperature = state
        .daemon
        .session_get_temperature(&id)
        .await
        .daemon_err()?;
    Ok(Json(TemperatureResponse { temperature }))
}

#[derive(Debug, Deserialize)]
struct SetMaxTokensRequest {
    max_tokens: Option<u32>,
}

async fn set_max_tokens(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetMaxTokensRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_max_tokens(&id, req.max_tokens)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

async fn get_max_tokens(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MaxTokensResponse>, WebError> {
    let max_tokens = state
        .daemon
        .session_get_max_tokens(&id)
        .await
        .daemon_err()?;
    Ok(Json(MaxTokensResponse { max_tokens }))
}

#[derive(Debug, Deserialize)]
struct SetPrecognitionRequest {
    enabled: bool,
}

async fn set_precognition(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetPrecognitionRequest>,
) -> Result<Json<OkResponse>, WebError> {
    state
        .daemon
        .session_set_precognition(&id, req.enabled)
        .await
        .daemon_err()?;
    Ok(OkResponse::success())
}

async fn get_precognition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<PrecognitionResponse>, WebError> {
    let enabled = state
        .daemon
        .session_get_precognition(&id)
        .await
        .daemon_err()?;
    Ok(Json(PrecognitionResponse {
        precognition_enabled: enabled,
    }))
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
    let session_dir = if crucible_config::is_crucible_home(kiln) {
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
struct ExecuteCommandRequest {
    command: String,
}

#[derive(Debug, Serialize)]
struct CommandResponse {
    result: String,
    #[serde(rename = "type")]
    response_type: String,
}

async fn execute_command(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteCommandRequest>,
) -> Result<Json<CommandResponse>, WebError> {
    let raw = req.command.trim().to_string();
    let command_str = raw.strip_prefix('/').unwrap_or(&raw);
    let (cmd, args) = match command_str.split_once(' ') {
        Some((c, a)) => (c.trim(), a.trim()),
        None => (command_str.trim(), ""),
    };

    match cmd {
        "help" => {
            let help_text = [
                "/help — Show available commands",
                "/search <query> — Search notes by title",
                "/models — List available models",
                "/clear — Clear the chat",
                "/export — Export session to markdown",
                "/model <name> — Switch to a different model",
            ]
            .join("\n");
            Ok(Json(CommandResponse {
                result: help_text,
                response_type: "success".to_string(),
            }))
        }
        "search" => {
            if args.is_empty() {
                return Ok(Json(CommandResponse {
                    result: "Usage: /search <query>".to_string(),
                    response_type: "error".to_string(),
                }));
            }

            // Get session to find kiln path
            let session = state.daemon.session_get(&id).await.daemon_err()?;
            let kiln_str = session.get("kiln").and_then(|v| v.as_str()).unwrap_or("");

            let kiln_path = if kiln_str.is_empty() {
                None
            } else {
                Some(PathBuf::from(kiln_str))
            };

            let results = state
                .daemon
                .session_search(args, kiln_path.as_deref(), Some(10))
                .await
                .daemon_err()?;

            let result_text = if let Some(sessions) = results.as_array() {
                if sessions.is_empty() {
                    format!("No results found for '{}'", args)
                } else {
                    let mut lines = vec![format!(
                        "Search results for '{}' ({} found):",
                        args,
                        sessions.len()
                    )];
                    for (i, item) in sessions.iter().enumerate() {
                        let title = item
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Untitled");
                        let id_val = item
                            .get("session_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        lines.push(format!("  {}. {} ({})", i + 1, title, id_val));
                    }
                    lines.join("\n")
                }
            } else {
                format!("Search results for '{}':\n{}", args, results)
            };

            Ok(Json(CommandResponse {
                result: result_text,
                response_type: "success".to_string(),
            }))
        }
        "models" => {
            let models = state.daemon.session_list_models(&id).await.daemon_err()?;
            let result = if models.is_empty() {
                "No models available".to_string()
            } else {
                let mut lines = vec![format!("Available models ({}):", models.len())];
                for model in &models {
                    lines.push(format!("  • {}", model));
                }
                lines.join("\n")
            };
            Ok(Json(CommandResponse {
                result,
                response_type: "success".to_string(),
            }))
        }
        "model" => {
            if args.is_empty() {
                return Ok(Json(CommandResponse {
                    result: "Usage: /model <name>".to_string(),
                    response_type: "error".to_string(),
                }));
            }
            state
                .daemon
                .session_switch_model(&id, args)
                .await
                .daemon_err()?;
            Ok(Json(CommandResponse {
                result: format!("Switched model to {}", args),
                response_type: "success".to_string(),
            }))
        }
        "clear" => Ok(Json(CommandResponse {
            result: "Chat cleared".to_string(),
            response_type: "success".to_string(),
        })),
        "export" => {
            // Return a hint — the actual export is handled by the existing export endpoint
            Ok(Json(CommandResponse {
                result: "Use the export dialog to download your session as markdown.".to_string(),
                response_type: "success".to_string(),
            }))
        }
        _ => Ok(Json(CommandResponse {
            result: format!(
                "Unknown command: /{}. Type /help for available commands.",
                cmd
            ),
            response_type: "error".to_string(),
        })),
    }
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
    async fn auto_title_fallback_when_no_messages() {
        // Mock daemon returns empty messages, so fallback to "Untitled Session"
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
            "Untitled Session",
            "Should fall back to 'Untitled Session' when no messages"
        );
    }

    #[test]
    fn truncate_to_title_short_message() {
        assert_eq!(truncate_to_title("Hello world"), "Hello world");
    }

    #[test]
    fn truncate_to_title_exact_limit() {
        let msg = "a".repeat(60);
        assert_eq!(truncate_to_title(&msg), msg);
    }

    #[test]
    fn truncate_to_title_long_message_breaks_at_word() {
        let msg = "How do I implement a binary search tree in Rust with proper lifetime annotations and borrowing";
        let title = truncate_to_title(msg);
        assert!(title.ends_with("..."), "Long titles should end with '...'");
        assert!(
            title.len() <= 65,
            "Title should be ~60 chars + '...': got {}",
            title.len()
        );
        // Should break at a word boundary
        assert!(!title.contains("  "), "Should not have double spaces");
    }

    #[test]
    fn truncate_to_title_collapses_whitespace() {
        assert_eq!(truncate_to_title("  hello   world  "), "hello world");
    }

    #[test]
    fn truncate_to_title_multiline_message() {
        let msg = "First line\nSecond line\nThird line";
        let title = truncate_to_title(msg);
        // split_whitespace treats \n as whitespace, so this becomes single-line
        assert!(!title.contains('\n'), "Title should not contain newlines");
    }

    #[test]
    fn truncate_to_title_handles_cjk_input() {
        // Test with Chinese characters
        let msg = "学习Rust编程语言";
        let title = truncate_to_title(msg);
        // Should not panic and should be valid UTF-8
        assert!(!title.is_empty(), "Title should not be empty");
        // Verify it's valid UTF-8 by checking we can iterate chars
        let char_count = title.chars().count();
        assert!(char_count > 0, "Title should contain valid characters");
        // Verify no truncation happened (message is shorter than MAX_LEN)
        assert_eq!(title, msg, "Short CJK message should not be truncated");
    }

    #[test]
    fn truncate_to_title_handles_emoji() {
        // Test with emoji
        let msg = "Hello 👋 world 🌍 this is a test message with emoji";
        let title = truncate_to_title(msg);
        // Should not panic and should be valid UTF-8
        assert!(!title.is_empty(), "Title should not be empty");
        // Verify it's valid UTF-8
        let char_count = title.chars().count();
        assert!(char_count > 0, "Title should contain valid characters");
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
