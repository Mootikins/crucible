use crate::services::daemon::AppState;
use crate::WebError;
use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use crucible_core::session::SessionAgent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;

pub fn session_routes() -> Router<AppState> {
    Router::new()
        .route("/api/session", post(create_session))
        .route("/api/session/list", get(list_sessions))
        .route("/api/session/{id}", get(get_session))
        .route("/api/session/{id}/history", get(get_session_history))
        .route("/api/session/{id}/pause", post(pause_session))
        .route("/api/session/{id}/resume", post(resume_session))
        .route("/api/session/{id}/end", post(end_session))
        .route("/api/session/{id}/cancel", post(cancel_session))
        .route("/api/session/{id}/models", get(list_models))
        .route("/api/session/{id}/model", post(switch_model))
        .route("/api/session/{id}/title", put(set_session_title))
        .route("/api/providers", get(list_providers))
}

#[derive(Debug, Deserialize)]
struct CreateSessionRequest {
    #[serde(default = "default_session_type")]
    session_type: String,
    kiln: PathBuf,
    workspace: Option<PathBuf>,
    /// LLM provider (e.g., "ollama", "openai", "anthropic")
    #[serde(default = "default_provider")]
    provider: String,
    /// Model name (e.g., "llama3.2", "gpt-4o", "claude-3-5-sonnet")
    #[serde(default = "default_model")]
    model: String,
    /// Custom endpoint URL (optional, for self-hosted models)
    endpoint: Option<String>,
}

fn default_session_type() -> String {
    "chat".to_string()
}

fn default_provider() -> String {
    "ollama".to_string()
}

fn default_model() -> String {
    "llama3.2".to_string()
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
        )
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    let session_id = result["session_id"].as_str().unwrap_or("");

    // Configure agent for the session (required before sending messages)
    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some(req.provider.clone()),
        provider: req.provider,
        model: req.model,
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
    };

    state
        .daemon
        .session_configure_agent(session_id, &agent)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct ListSessionsQuery {
    kiln: Option<PathBuf>,
    workspace: Option<PathBuf>,
    #[serde(rename = "type")]
    session_type: Option<String>,
    state: Option<String>,
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
        )
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_get(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

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
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

async fn pause_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_pause(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

async fn resume_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_resume(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    let session_id = id.as_str();
    state
        .daemon
        .session_subscribe(&[session_id])
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(result))
}

async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .session_end(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    state.events.remove_session(&id).await;

    Ok(Json(result))
}

async fn cancel_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let cancelled = state
        .daemon
        .session_cancel(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "cancelled": cancelled })))
}

async fn list_models(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let models = state
        .daemon
        .session_list_models(&id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "models": models })))
}

#[derive(Debug, Deserialize)]
struct SwitchModelRequest {
    model_id: String,
}

async fn switch_model(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SwitchModelRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    state
        .daemon
        .session_switch_model(&id, &req.model_id)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
struct SetTitleRequest {
    title: String,
}

async fn set_session_title(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetTitleRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    state
        .daemon
        .session_set_title(&id, &req.title)
        .await
        .map_err(|e| WebError::Daemon(e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Serialize)]
struct ProviderInfo {
    name: String,
    provider_type: String,
    available: bool,
    default_model: Option<String>,
    models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
}

async fn list_providers(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, WebError> {
    let mut providers = Vec::new();
    let mut seen_types = std::collections::HashSet::new();

    // 1. Check `providers` config (new format with named providers)
    if state.config.providers.has_providers() {
        for (name, provider_config) in state.config.providers.chat_providers() {
            let provider_type = provider_config.backend.as_str();
            seen_types.insert(provider_type.to_string());

            let endpoint = provider_config
                .endpoint()
                .unwrap_or_else(|| default_endpoint_for(provider_type));

            let models =
                fetch_models_for_provider(&state.http_client, provider_type, &endpoint).await;

            providers.push(ProviderInfo {
                name: format_provider_name(name, provider_type),
                provider_type: provider_type.to_string(),
                available: !models.is_empty() || provider_type != "ollama",
                default_model: provider_config
                    .chat_model()
                    .or_else(|| models.first().cloned()),
                models,
                endpoint: Some(endpoint),
            });
        }
    }

    // 2. Fall back to `chat` config (legacy format) if no providers found
    if providers.is_empty() {
        let chat = &state.config.chat;
        let provider_type = llm_provider_to_str(&chat.provider);
        let endpoint = chat
            .endpoint
            .clone()
            .unwrap_or_else(|| default_endpoint_for(provider_type));

        let models = fetch_models_for_provider(&state.http_client, provider_type, &endpoint).await;

        if !models.is_empty() || provider_type != "ollama" {
            seen_types.insert(provider_type.to_string());
            providers.push(ProviderInfo {
                name: format_provider_name("default", provider_type),
                provider_type: provider_type.to_string(),
                available: true,
                default_model: chat.model.clone().or_else(|| models.first().cloned()),
                models,
                endpoint: Some(endpoint),
            });
        }
    }

    // 3. Also detect providers from environment variables (that weren't already found)
    if !seen_types.contains("ollama") && std::env::var("OLLAMA_HOST").is_ok() {
        let endpoint = ollama_endpoint_from_env();
        let models = fetch_ollama_models(&state.http_client, &endpoint).await;
        if !models.is_empty() {
            providers.push(ProviderInfo {
                name: "Ollama (Environment)".to_string(),
                provider_type: "ollama".to_string(),
                available: true,
                default_model: models.first().cloned(),
                models,
                endpoint: Some(endpoint),
            });
        }
    }

    if !seen_types.contains("openai") && std::env::var("OPENAI_API_KEY").is_ok() {
        let openai_models = fetch_openai_models(&state.http_client).await;
        providers.push(ProviderInfo {
            name: "OpenAI".to_string(),
            provider_type: "openai".to_string(),
            available: true,
            default_model: Some("gpt-4o-mini".to_string()),
            // Fallback model list when the OpenAI API is unreachable.
            // Update periodically as new models are released.
            models: if openai_models.is_empty() {
                vec![
                    "gpt-4o".to_string(),
                    "gpt-4o-mini".to_string(),
                    "gpt-4-turbo".to_string(),
                    "o1".to_string(),
                    "o1-mini".to_string(),
                ]
            } else {
                openai_models
            },
            endpoint: Some("https://api.openai.com/v1".to_string()),
        });
    }

    if !seen_types.contains("anthropic") && std::env::var("ANTHROPIC_API_KEY").is_ok() {
        let models = anthropic_models();
        providers.push(ProviderInfo {
            name: "Anthropic".to_string(),
            provider_type: "anthropic".to_string(),
            available: true,
            default_model: models.first().cloned(),
            models,
            endpoint: Some("https://api.anthropic.com".to_string()),
        });
    }

    Ok(Json(serde_json::json!({ "providers": providers })))
}

fn llm_provider_to_str(provider: &crucible_config::LlmProviderType) -> &'static str {
    match provider {
        crucible_config::LlmProviderType::Ollama => "ollama",
        crucible_config::LlmProviderType::OpenAI => "openai",
        crucible_config::LlmProviderType::Anthropic => "anthropic",
        crucible_config::LlmProviderType::GitHubCopilot => "github-copilot",
        crucible_config::LlmProviderType::OpenRouter => "openrouter",
    }
}

fn format_provider_name(name: &str, provider_type: &str) -> String {
    let type_label = match provider_type {
        "ollama" => "Ollama",
        "openai" => "OpenAI",
        "anthropic" => "Anthropic",
        _ => return name.to_string(),
    };
    if name == "default" || name.eq_ignore_ascii_case(type_label) {
        type_label.to_string()
    } else {
        format!("{type_label} ({name})")
    }
}

fn default_endpoint_for(provider_type: &str) -> String {
    match provider_type {
        "ollama" => "http://localhost:11434".to_string(),
        "openai" => "https://api.openai.com/v1".to_string(),
        "anthropic" => "https://api.anthropic.com".to_string(),
        _ => String::new(),
    }
}

fn ollama_endpoint_from_env() -> String {
    std::env::var("OLLAMA_HOST")
        .ok()
        .map(|host| {
            if host.starts_with("http://") || host.starts_with("https://") {
                host
            } else {
                format!("http://{}", host)
            }
        })
        .unwrap_or_else(|| "http://localhost:11434".to_string())
}

async fn fetch_models_for_provider(
    client: &reqwest::Client,
    provider_type: &str,
    endpoint: &str,
) -> Vec<String> {
    match provider_type {
        "ollama" => fetch_ollama_models(client, endpoint).await,
        "openai" => fetch_openai_models(client).await,
        "anthropic" => anthropic_models(),
        _ => Vec::new(),
    }
}

/// Static Anthropic model list used when API enumeration is unavailable.
/// Update periodically as new models are released.
fn anthropic_models() -> Vec<String> {
    vec![
        "claude-sonnet-4-20250514".to_string(),
        "claude-3-7-sonnet-20250219".to_string(),
        "claude-3-5-sonnet-20241022".to_string(),
        "claude-3-5-haiku-20241022".to_string(),
        "claude-3-opus-20240229".to_string(),
    ]
}

async fn fetch_ollama_models(client: &reqwest::Client, endpoint: &str) -> Vec<String> {
    let base = endpoint.trim_end_matches('/').trim_end_matches("/v1");
    let url = format!("{}/api/tags", base);

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return Vec::new(),
    };

    #[derive(serde::Deserialize)]
    struct TagsResponse {
        models: Vec<ModelInfo>,
    }
    #[derive(serde::Deserialize)]
    struct ModelInfo {
        name: String,
    }

    match resp.json::<TagsResponse>().await {
        Ok(tags) => tags.models.into_iter().map(|m| m.name).collect(),
        Err(_) => Vec::new(),
    }
}

async fn fetch_openai_models(client: &reqwest::Client) -> Vec<String> {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(k) => k,
        Err(_) => return Vec::new(),
    };

    let resp = match client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return Vec::new(),
    };

    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelData>,
    }
    #[derive(serde::Deserialize)]
    struct ModelData {
        id: String,
    }

    match resp.json::<ModelsResponse>().await {
        Ok(models) => models
            .data
            .into_iter()
            .filter(|m| {
                m.id.starts_with("gpt-")
                    || m.id.starts_with("chatgpt-")
                    || m.id.starts_with("o1")
                    || m.id.starts_with("o3")
                    || m.id.starts_with("o4")
            })
            .map(|m| m.id)
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_config::LlmProviderType;
    use serial_test::serial;

    #[test]
    fn test_llm_provider_to_str_ollama() {
        assert_eq!(llm_provider_to_str(&LlmProviderType::Ollama), "ollama");
    }

    #[test]
    fn test_llm_provider_to_str_openai() {
        assert_eq!(llm_provider_to_str(&LlmProviderType::OpenAI), "openai");
    }

    #[test]
    fn test_llm_provider_to_str_anthropic() {
        assert_eq!(
            llm_provider_to_str(&LlmProviderType::Anthropic),
            "anthropic"
        );
    }

    #[test]
    fn test_format_provider_name_ollama() {
        assert_eq!(format_provider_name("local", "ollama"), "Ollama (local)");
        assert_eq!(format_provider_name("remote", "ollama"), "Ollama (remote)");
    }

    #[test]
    fn test_format_provider_name_openai_uses_configured_name() {
        assert_eq!(format_provider_name("default", "openai"), "OpenAI");
        assert_eq!(format_provider_name("OpenAI", "openai"), "OpenAI");
        assert_eq!(format_provider_name("work", "openai"), "OpenAI (work)");
    }

    #[test]
    fn test_format_provider_name_anthropic_uses_configured_name() {
        assert_eq!(format_provider_name("default", "anthropic"), "Anthropic");
        assert_eq!(format_provider_name("Anthropic", "anthropic"), "Anthropic");
        assert_eq!(
            format_provider_name("research", "anthropic"),
            "Anthropic (research)"
        );
    }

    #[test]
    fn test_format_provider_name_unknown_uses_name() {
        assert_eq!(format_provider_name("my-custom", "custom"), "my-custom");
        assert_eq!(format_provider_name("cohere", "cohere"), "cohere");
    }

    #[test]
    fn test_default_endpoint_for_ollama() {
        assert_eq!(default_endpoint_for("ollama"), "http://localhost:11434");
    }

    #[test]
    fn test_default_endpoint_for_openai() {
        assert_eq!(default_endpoint_for("openai"), "https://api.openai.com/v1");
    }

    #[test]
    fn test_default_endpoint_for_anthropic() {
        assert_eq!(
            default_endpoint_for("anthropic"),
            "https://api.anthropic.com"
        );
    }

    #[test]
    fn test_default_endpoint_for_unknown_is_empty() {
        assert_eq!(default_endpoint_for("custom"), "");
        assert_eq!(default_endpoint_for("unknown"), "");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_from_env_default() {
        std::env::remove_var("OLLAMA_HOST");
        assert_eq!(ollama_endpoint_from_env(), "http://localhost:11434");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_from_env_host_port() {
        std::env::set_var("OLLAMA_HOST", "myhost:11435");
        assert_eq!(ollama_endpoint_from_env(), "http://myhost:11435");
        std::env::remove_var("OLLAMA_HOST");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_from_env_full_url() {
        std::env::set_var("OLLAMA_HOST", "http://custom.local:8080");
        assert_eq!(ollama_endpoint_from_env(), "http://custom.local:8080");
        std::env::remove_var("OLLAMA_HOST");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_from_env_https() {
        std::env::set_var("OLLAMA_HOST", "https://secure.ollama.io");
        assert_eq!(ollama_endpoint_from_env(), "https://secure.ollama.io");
        std::env::remove_var("OLLAMA_HOST");
    }

    #[test]
    fn test_anthropic_models_contains_expected() {
        let models = anthropic_models();
        assert!(models.contains(&"claude-sonnet-4-20250514".to_string()));
        assert!(models.contains(&"claude-3-5-sonnet-20241022".to_string()));
        assert!(models.contains(&"claude-3-opus-20240229".to_string()));
    }

    #[test]
    fn test_anthropic_models_not_empty() {
        assert!(!anthropic_models().is_empty());
    }

    #[test]
    fn test_provider_info_serialization() {
        let info = ProviderInfo {
            name: "Test".to_string(),
            provider_type: "ollama".to_string(),
            available: true,
            default_model: Some("llama3".to_string()),
            models: vec!["llama3".to_string(), "mistral".to_string()],
            endpoint: Some("http://localhost:11434".to_string()),
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "Test");
        assert_eq!(json["provider_type"], "ollama");
        assert_eq!(json["available"], true);
        assert_eq!(json["default_model"], "llama3");
        assert_eq!(json["models"].as_array().unwrap().len(), 2);
        assert_eq!(json["endpoint"], "http://localhost:11434");
    }

    #[test]
    fn test_provider_info_endpoint_none_skipped() {
        let info = ProviderInfo {
            name: "Test".to_string(),
            provider_type: "ollama".to_string(),
            available: true,
            default_model: None,
            models: vec![],
            endpoint: None,
        };

        let json = serde_json::to_value(&info).unwrap();
        assert!(json.get("endpoint").is_none());
    }
}
