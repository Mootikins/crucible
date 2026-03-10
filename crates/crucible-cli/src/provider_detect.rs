//! Local-only provider detection for `cru init`.
//!
//! This module exists because `cru init` runs before the daemon is started,
//! so it cannot use the `providers.list` RPC. It performs env-var and
//! credential-store checks only — no HTTP probing.
//!
//! For runtime provider discovery (after daemon is running), use
//! `DaemonClient::list_providers()` instead.

use crucible_config::credentials::{CredentialSource, CredentialStore, SecretsFile};
use crucible_config::{BackendType, ChatConfig, DEFAULT_OLLAMA_ENDPOINT};
use std::time::Duration;

/// A detected provider with availability info
#[derive(Debug, Clone)]
pub struct DetectedProvider {
    pub name: String,
    pub provider_type: String,
    pub available: bool,
    pub reason: String,
    pub default_model: Option<String>,
    pub source: Option<CredentialSource>,
}

/// Get the Ollama endpoint from OLLAMA_HOST env var or default
pub fn ollama_endpoint() -> String {
    std::env::var("OLLAMA_HOST")
        .ok()
        .map(|host| {
            // OLLAMA_HOST can be just "host:port" or a full URL
            if host.starts_with("http://") || host.starts_with("https://") {
                host
            } else {
                format!("http://{}", host)
            }
        })
        .unwrap_or_else(|| DEFAULT_OLLAMA_ENDPOINT.to_string())
}

/// Check if an API key exists for a provider (env var or credential store)
pub fn has_api_key(provider: &str) -> bool {
    has_api_key_with_source(provider).is_some()
}

/// Check if an API key exists and return its source
pub fn has_api_key_with_source(provider: &str) -> Option<CredentialSource> {
    match provider.to_lowercase().as_str() {
        "openai" if std::env::var("OPENAI_API_KEY").is_ok() => {
            return Some(CredentialSource::EnvVar)
        }
        "anthropic" if std::env::var("ANTHROPIC_API_KEY").is_ok() => {
            return Some(CredentialSource::EnvVar)
        }
        _ => {}
    }

    let store = SecretsFile::new();
    if let Ok(Some(_)) = store.get(provider) {
        return Some(CredentialSource::Store);
    }

    None
}

/// Fetch context length for a model from OpenAI-compatible /v1/models endpoint
/// Falls back to Ollama /api/show if /v1/models doesn't provide context length
pub async fn fetch_model_context_length(endpoint: &str, model_id: &str) -> Option<usize> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    // Try OpenAI-compatible /v1/models endpoint first
    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let resp = client.get(&url).send().await.ok()?;

    if !resp.status().is_success() {
        return try_ollama_api_show(&client, endpoint, model_id).await;
    }

    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelData>,
    }

    #[derive(serde::Deserialize)]
    struct ModelData {
        id: String,
        #[serde(default)]
        meta: Option<ModelMeta>,
    }

    #[derive(serde::Deserialize)]
    struct ModelMeta {
        #[serde(default)]
        llamaswap: Option<LlamaSwapMeta>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LlamaSwapMeta {
        context_length: Option<usize>,
    }

    let models: ModelsResponse = resp.json().await.ok()?;
    let result = models
        .data
        .iter()
        .find(|m| m.id == model_id)
        .and_then(|m| m.meta.as_ref())
        .and_then(|meta| meta.llamaswap.as_ref())
        .and_then(|ls| ls.context_length);

    // If llamaswap didn't provide context length, try Ollama /api/show
    if result.is_none() {
        return try_ollama_api_show(&client, endpoint, model_id).await;
    }

    result
}

/// Try to fetch context length from Ollama's /api/show endpoint
async fn try_ollama_api_show(
    client: &reqwest::Client,
    endpoint: &str,
    model_id: &str,
) -> Option<usize> {
    // Strip /v1 suffix if present to get the base Ollama endpoint
    let base_url = endpoint
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_string();

    let url = format!("{}/api/show", base_url);

    #[derive(serde::Serialize)]
    struct ShowRequest {
        model: String,
    }

    #[derive(serde::Deserialize)]
    struct ShowResponse {
        #[serde(default)]
        model_info: Option<serde_json::Value>,
        #[serde(default)]
        parameters: Option<String>,
    }

    let req_body = ShowRequest {
        model: model_id.to_string(),
    };

    let resp = client.post(&url).json(&req_body).send().await.ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let show_resp: ShowResponse = resp.json().await.ok()?;

    // Try to extract context length from model_info
    if let Some(model_info) = show_resp.model_info {
        if let Some(ctx_len) = model_info.get("llama.context_length") {
            if let Some(n) = ctx_len.as_u64() {
                return Some(n as usize);
            }
        }
        if let Some(ctx_len) = model_info.get("context_length") {
            if let Some(n) = ctx_len.as_u64() {
                return Some(n as usize);
            }
        }
        // Try any key containing "context_length"
        for (key, value) in model_info.as_object().iter().flat_map(|o| o.iter()) {
            if key.contains("context_length") {
                if let Some(n) = value.as_u64() {
                    return Some(n as usize);
                }
            }
        }
    }

    // Try to extract from parameters string (e.g., "num_ctx 4096")
    if let Some(params) = show_resp.parameters {
        if let Some(pos) = params.find("num_ctx") {
            let after_num_ctx = &params[pos + 7..];
            if let Some(num_str) = after_num_ctx.split_whitespace().next() {
                if let Ok(n) = num_str.parse::<usize>() {
                    return Some(n);
                }
            }
        }
    }

    None
}

/// Detect available providers from config and environment only (no HTTP probes).
///
/// Checks: config file provider, OLLAMA_HOST env, API key env vars, credential store.
pub fn detect_providers(config: &ChatConfig) -> Vec<DetectedProvider> {
    let mut providers = Vec::new();
    let provider_backend = BackendType::Ollama;

    match provider_backend {
        BackendType::Ollama => {
            let endpoint = config
                .endpoint
                .as_deref()
                .unwrap_or(DEFAULT_OLLAMA_ENDPOINT);
            let reason = if std::env::var("OLLAMA_HOST").is_ok() {
                format!("OLLAMA_HOST={}", ollama_endpoint())
            } else if config.endpoint.is_some() {
                format!("config endpoint={}", endpoint)
            } else {
                "config provider=ollama".to_string()
            };
            providers.push(DetectedProvider {
                name: "Ollama (Local)".to_string(),
                provider_type: "ollama".to_string(),
                available: true,
                reason,
                default_model: config.model.clone(),
                source: None,
            });
        }
        BackendType::OpenAI => {
            if let Some(src) = has_api_key_with_source("openai") {
                providers.push(DetectedProvider {
                    name: "OpenAI".to_string(),
                    provider_type: "openai".to_string(),
                    available: true,
                    reason: format!("API key found ({})", src),
                    default_model: config.model.clone().or(Some("gpt-4o-mini".to_string())),
                    source: Some(src),
                });
            }
        }
        BackendType::Anthropic => {
            if let Some(src) = has_api_key_with_source("anthropic") {
                providers.push(DetectedProvider {
                    name: "Anthropic".to_string(),
                    provider_type: "anthropic".to_string(),
                    available: true,
                    reason: format!("API key found ({})", src),
                    default_model: config
                        .model
                        .clone()
                        .or(Some("claude-3-5-sonnet-latest".to_string())),
                    source: Some(src),
                });
            }
        }
        BackendType::GitHubCopilot => {}
        BackendType::OpenRouter => {
            if let Some(src) = has_api_key_with_source("openrouter") {
                providers.push(DetectedProvider {
                    name: "OpenRouter".to_string(),
                    provider_type: "openrouter".to_string(),
                    available: true,
                    reason: format!("API key found ({})", src),
                    default_model: config.model.clone().or(Some("openai/gpt-4o".to_string())),
                    source: Some(src),
                });
            }
        }
        BackendType::ZAI => {
            if let Some(src) = has_api_key_with_source("zai") {
                providers.push(DetectedProvider {
                    name: "Z.AI".to_string(),
                    provider_type: "zai".to_string(),
                    available: true,
                    reason: format!("API key found ({})", src),
                    default_model: config.model.clone().or(Some("GLM-4.7".to_string())),
                    source: Some(src),
                });
            }
        }
        BackendType::Cohere
        | BackendType::VertexAI
        | BackendType::FastEmbed
        | BackendType::Burn
        | BackendType::Custom
        | BackendType::Mock => {}
    }

    // Also detect providers not in config but available via env/credentials
    if !providers.iter().any(|p| p.provider_type == "openai") {
        if let Some(src) = has_api_key_with_source("openai") {
            providers.push(DetectedProvider {
                name: "OpenAI".to_string(),
                provider_type: "openai".to_string(),
                available: true,
                reason: format!("API key found ({})", src),
                default_model: Some("gpt-4o-mini".to_string()),
                source: Some(src),
            });
        }
    }

    if !providers.iter().any(|p| p.provider_type == "anthropic") {
        if let Some(src) = has_api_key_with_source("anthropic") {
            providers.push(DetectedProvider {
                name: "Anthropic".to_string(),
                provider_type: "anthropic".to_string(),
                available: true,
                reason: format!("API key found ({})", src),
                default_model: Some("claude-3-5-sonnet-latest".to_string()),
                source: Some(src),
            });
        }
    }

    // Ollama via OLLAMA_HOST env even if not the configured provider
    if !providers.iter().any(|p| p.provider_type == "ollama")
        && std::env::var("OLLAMA_HOST").is_ok()
    {
        providers.push(DetectedProvider {
            name: "Ollama (Local)".to_string(),
            provider_type: "ollama".to_string(),
            available: true,
            reason: format!("OLLAMA_HOST={}", ollama_endpoint()),
            default_model: None,
            source: None,
        });
    }

    providers
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::test_support::EnvVarGuard;
    use serial_test::serial;

    #[test]
    fn test_detect_ollama_from_default_config() {
        let config = ChatConfig::default();
        let detected = detect_providers(&config);
        assert!(!detected.is_empty());
        assert_eq!(detected[0].provider_type, "ollama");
        assert!(detected[0].reason.contains("config provider=ollama"));
    }

    #[test]
    #[serial]
    fn test_detect_ollama_from_env() {
        let _guard = EnvVarGuard::set("OLLAMA_HOST", "http://myhost:11434".to_string());
        let config = ChatConfig::default();
        let detected = detect_providers(&config);
        assert!(!detected.is_empty());
        let ollama = detected
            .iter()
            .find(|p| p.provider_type == "ollama")
            .unwrap();
        assert!(ollama.reason.contains("OLLAMA_HOST"));
    }

    #[test]
    #[serial]
    fn test_detect_openai_from_config_with_key() {
        let _guard = EnvVarGuard::set("OPENAI_API_KEY", "sk-test".to_string());
        let config = ChatConfig::default();
        let detected = detect_providers(&config);
        assert!(detected.iter().any(|p| p.provider_type == "openai"));
    }

    #[test]
    #[serial]
    fn test_detect_openai_from_config_without_key_is_empty() {
        let _guard1 = EnvVarGuard::remove("OPENAI_API_KEY");
        let _guard2 = EnvVarGuard::remove("ANTHROPIC_API_KEY");
        let config = ChatConfig::default();
        let detected = detect_providers(&config);
        // No API key = no provider detected for cloud providers
        assert!(!detected.iter().any(|p| p.provider_type == "openai"));
    }

    #[test]
    #[serial]
    fn test_detect_extra_providers_from_env() {
        let _guard = EnvVarGuard::set("ANTHROPIC_API_KEY", "sk-ant-test".to_string());
        let config = ChatConfig::default(); // ollama config
        let detected = detect_providers(&config);
        // Should have ollama from config + anthropic from env
        assert!(detected.iter().any(|p| p.provider_type == "ollama"));
        assert!(detected.iter().any(|p| p.provider_type == "anthropic"));
    }

    #[test]
    #[serial]
    fn test_has_api_key_openai() {
        let _guard = EnvVarGuard::set("OPENAI_API_KEY", "sk-test".to_string());
        assert!(has_api_key("openai"));
    }

    #[test]
    #[serial]
    fn test_has_api_key_anthropic() {
        let _guard = EnvVarGuard::set("ANTHROPIC_API_KEY", "sk-ant-test".to_string());
        assert!(has_api_key("anthropic"));
    }

    #[test]
    fn test_has_api_key_unknown_provider() {
        assert!(!has_api_key("unknown"));
        assert!(!has_api_key("google"));
    }

    #[test]
    #[serial]
    fn test_has_api_key_case_insensitive() {
        let _guard = EnvVarGuard::set("OPENAI_API_KEY", "sk-test".to_string());
        assert!(has_api_key("OpenAI"));
        assert!(has_api_key("OPENAI"));
        assert!(has_api_key("openai"));
    }

    #[test]
    #[serial]
    fn test_has_api_key_missing() {
        let _guard1 = EnvVarGuard::remove("OPENAI_API_KEY");
        let _guard2 = EnvVarGuard::remove("ANTHROPIC_API_KEY");
        assert!(!has_api_key("openai"));
        assert!(!has_api_key("anthropic"));
    }

    #[test]
    fn test_detected_provider_struct() {
        let provider = DetectedProvider {
            name: "Test Provider".to_string(),
            provider_type: "test".to_string(),
            available: true,
            reason: "Test reason".to_string(),
            default_model: Some("test-model".to_string()),
            source: Some(CredentialSource::EnvVar),
        };

        assert_eq!(provider.name, "Test Provider");
        assert_eq!(provider.provider_type, "test");
        assert!(provider.available);
        assert_eq!(provider.reason, "Test reason");
        assert_eq!(provider.default_model, Some("test-model".to_string()));
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_default() {
        let _guard = EnvVarGuard::remove("OLLAMA_HOST");
        assert_eq!(ollama_endpoint(), "http://localhost:11434");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_custom_host_port() {
        let _guard = EnvVarGuard::set("OLLAMA_HOST", "myhost:11435".to_string());
        assert_eq!(ollama_endpoint(), "http://myhost:11435");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_full_url() {
        let _guard = EnvVarGuard::set("OLLAMA_HOST", "http://custom-ollama.local:8080".to_string());
        assert_eq!(ollama_endpoint(), "http://custom-ollama.local:8080");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_https() {
        let _guard = EnvVarGuard::set(
            "OLLAMA_HOST",
            "https://secure-ollama.example.com".to_string(),
        );
        assert_eq!(ollama_endpoint(), "https://secure-ollama.example.com");
    }

    #[tokio::test]
    async fn test_fetch_model_context_length_nonexistent_endpoint() {
        let result = fetch_model_context_length("http://localhost:99999", "test-model").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_fetch_model_context_length_ollama_api_show_context_length() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock the /v1/models endpoint to return 404 (not found)
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Mock the /api/show endpoint with llama.context_length
        let show_response = serde_json::json!({
            "model_info": {
                "llama.context_length": 131072,
                "llama.embedding_length": 4096
            }
        });

        Mock::given(method("POST"))
            .and(path("/api/show"))
            .respond_with(ResponseTemplate::new(200).set_body_json(show_response))
            .mount(&mock_server)
            .await;

        let result = fetch_model_context_length(&mock_server.uri(), "test-model").await;
        assert_eq!(result, Some(131072));
    }

    #[tokio::test]
    async fn test_fetch_model_context_length_ollama_api_show_num_ctx_in_parameters() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock the /v1/models endpoint to return 404
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Mock the /api/show endpoint with num_ctx in parameters
        let show_response = serde_json::json!({
            "model_info": {},
            "parameters": "num_ctx 4096\nstop \"<|im_start|>\""
        });

        Mock::given(method("POST"))
            .and(path("/api/show"))
            .respond_with(ResponseTemplate::new(200).set_body_json(show_response))
            .mount(&mock_server)
            .await;

        let result = fetch_model_context_length(&mock_server.uri(), "test-model").await;
        assert_eq!(result, Some(4096));
    }

    #[tokio::test]
    async fn test_fetch_model_context_length_ollama_api_show_generic_context_length_key() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock the /v1/models endpoint to return 404
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Mock the /api/show endpoint with generic context_length key
        let show_response = serde_json::json!({
            "model_info": {
                "context_length": 8192
            }
        });

        Mock::given(method("POST"))
            .and(path("/api/show"))
            .respond_with(ResponseTemplate::new(200).set_body_json(show_response))
            .mount(&mock_server)
            .await;

        let result = fetch_model_context_length(&mock_server.uri(), "test-model").await;
        assert_eq!(result, Some(8192));
    }

    #[tokio::test]
    async fn test_fetch_model_context_length_ollama_api_show_fallback_on_empty_model_info() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock the /v1/models endpoint to return 404
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Mock the /api/show endpoint with empty model_info
        let show_response = serde_json::json!({
            "model_info": {}
        });

        Mock::given(method("POST"))
            .and(path("/api/show"))
            .respond_with(ResponseTemplate::new(200).set_body_json(show_response))
            .mount(&mock_server)
            .await;

        let result = fetch_model_context_length(&mock_server.uri(), "test-model").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_fetch_model_context_length_ollama_api_show_strips_v1_suffix() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock the /v1/models endpoint to return 404
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Mock the /api/show endpoint
        let show_response = serde_json::json!({
            "model_info": {
                "llama.context_length": 2048
            }
        });

        Mock::given(method("POST"))
            .and(path("/api/show"))
            .respond_with(ResponseTemplate::new(200).set_body_json(show_response))
            .mount(&mock_server)
            .await;

        // Call with /v1 suffix in endpoint
        let endpoint_with_v1 = format!("{}/v1", mock_server.uri());
        let result = fetch_model_context_length(&endpoint_with_v1, "test-model").await;
        assert_eq!(result, Some(2048));
    }

    #[tokio::test]
    #[ignore = "requires llm.example.com endpoint"]
    async fn test_fetch_model_context_length_real_endpoint() {
        let result =
            fetch_model_context_length("https://llm.example.com", "qwen3-4b-instruct-2507-q8_0")
                .await;
        assert!(result.is_some());
        assert!(result.unwrap() > 0);
    }
}
