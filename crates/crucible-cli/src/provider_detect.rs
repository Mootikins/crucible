//! Provider detection for interactive setup
//!
//! Detects available LLM providers by checking:
//! - Config file: chat.provider setting
//! - Ollama: OLLAMA_HOST env var or config endpoint
//! - OpenAI: OPENAI_API_KEY env var or credential store
//! - Anthropic: ANTHROPIC_API_KEY env var or credential store

use crucible_config::credentials::{CredentialSource, CredentialStore, SecretsFile};
use crucible_config::{ChatConfig, LlmProvider};
use std::time::Duration;

/// Default Ollama endpoint
const DEFAULT_OLLAMA_HOST: &str = "http://localhost:11434";

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
        .unwrap_or_else(|| DEFAULT_OLLAMA_HOST.to_string())
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

/// Fetch available models from an Ollama endpoint (used by `cru models`)
pub async fn check_ollama_models(endpoint: &str) -> Option<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let resp = client.get(&url).send().await.ok()?;

    if !resp.status().is_success() {
        return None;
    }

    #[derive(serde::Deserialize)]
    struct TagsResponse {
        models: Vec<ModelInfo>,
    }
    #[derive(serde::Deserialize)]
    struct ModelInfo {
        name: String,
    }

    let tags: TagsResponse = resp.json().await.ok()?;
    Some(tags.models.into_iter().map(|m| m.name).collect())
}

/// Fetch available models for a provider, returning formatted as "provider/model"
pub async fn fetch_provider_models(
    provider: &LlmProvider,
    endpoint: &str,
) -> Vec<String> {
    match provider {
        LlmProvider::Ollama => fetch_ollama_models(endpoint).await,
        LlmProvider::OpenAI => fetch_openai_models(endpoint).await,
        LlmProvider::Anthropic => anthropic_models(),
    }
}

async fn fetch_ollama_models(endpoint: &str) -> Vec<String> {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

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

async fn fetch_openai_models(endpoint: &str) -> Vec<String> {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(k) => k,
        Err(_) => return Vec::new(),
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let url = format!("{}/models", endpoint.trim_end_matches('/'));
    let resp = match client
        .get(&url)
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
                m.id.starts_with("gpt-") || m.id.starts_with("o1") || m.id.starts_with("o3")
            })
            .map(|m| m.id)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn anthropic_models() -> Vec<String> {
    vec![
        "claude-sonnet-4-20250514".to_string(),
        "claude-3-7-sonnet-20250219".to_string(),
        "claude-3-5-sonnet-20241022".to_string(),
        "claude-3-5-haiku-20241022".to_string(),
        "claude-3-opus-20240229".to_string(),
    ]
}

/// Fetch context length for a model from OpenAI-compatible /v1/models endpoint
pub async fn fetch_model_context_length(endpoint: &str, model_id: &str) -> Option<usize> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let resp = client.get(&url).send().await.ok()?;

    if !resp.status().is_success() {
        return None;
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
    models
        .data
        .iter()
        .find(|m| m.id == model_id)
        .and_then(|m| m.meta.as_ref())
        .and_then(|meta| meta.llamaswap.as_ref())
        .and_then(|ls| ls.context_length)
}

/// Detect available providers from config and environment only (no HTTP probes).
///
/// Checks: config file provider, OLLAMA_HOST env, API key env vars, credential store.
pub fn detect_providers(config: &ChatConfig) -> Vec<DetectedProvider> {
    let mut providers = Vec::new();

    match config.provider {
        LlmProvider::Ollama => {
            let endpoint = config.endpoint.as_deref().unwrap_or(DEFAULT_OLLAMA_HOST);
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
        LlmProvider::OpenAI => {
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
        LlmProvider::Anthropic => {
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
    if !providers.iter().any(|p| p.provider_type == "ollama") && std::env::var("OLLAMA_HOST").is_ok()
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
        std::env::set_var("OLLAMA_HOST", "http://myhost:11434");
        let config = ChatConfig::default();
        let detected = detect_providers(&config);
        assert!(!detected.is_empty());
        let ollama = detected.iter().find(|p| p.provider_type == "ollama").unwrap();
        assert!(ollama.reason.contains("OLLAMA_HOST"));
        std::env::remove_var("OLLAMA_HOST");
    }

    #[test]
    #[serial]
    fn test_detect_openai_from_config_with_key() {
        std::env::set_var("OPENAI_API_KEY", "sk-test");
        let config = ChatConfig {
            provider: LlmProvider::OpenAI,
            ..ChatConfig::default()
        };
        let detected = detect_providers(&config);
        assert!(detected.iter().any(|p| p.provider_type == "openai"));
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_detect_openai_from_config_without_key_is_empty() {
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
        let config = ChatConfig {
            provider: LlmProvider::OpenAI,
            ..ChatConfig::default()
        };
        let detected = detect_providers(&config);
        // No API key = no provider detected for cloud providers
        assert!(!detected.iter().any(|p| p.provider_type == "openai"));
    }

    #[test]
    #[serial]
    fn test_detect_extra_providers_from_env() {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test");
        let config = ChatConfig::default(); // ollama config
        let detected = detect_providers(&config);
        // Should have ollama from config + anthropic from env
        assert!(detected.iter().any(|p| p.provider_type == "ollama"));
        assert!(detected.iter().any(|p| p.provider_type == "anthropic"));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    #[serial]
    fn test_has_api_key_openai() {
        std::env::set_var("OPENAI_API_KEY", "sk-test");
        assert!(has_api_key("openai"));
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_has_api_key_anthropic() {
        std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test");
        assert!(has_api_key("anthropic"));
        std::env::remove_var("ANTHROPIC_API_KEY");
    }

    #[test]
    fn test_has_api_key_unknown_provider() {
        assert!(!has_api_key("unknown"));
        assert!(!has_api_key("google"));
    }

    #[test]
    #[serial]
    fn test_has_api_key_case_insensitive() {
        std::env::set_var("OPENAI_API_KEY", "sk-test");
        assert!(has_api_key("OpenAI"));
        assert!(has_api_key("OPENAI"));
        assert!(has_api_key("openai"));
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    #[serial]
    fn test_has_api_key_missing() {
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");
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
        std::env::remove_var("OLLAMA_HOST");
        assert_eq!(ollama_endpoint(), "http://localhost:11434");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_custom_host_port() {
        std::env::set_var("OLLAMA_HOST", "myhost:11435");
        assert_eq!(ollama_endpoint(), "http://myhost:11435");
        std::env::remove_var("OLLAMA_HOST");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_full_url() {
        std::env::set_var("OLLAMA_HOST", "http://custom-ollama.local:8080");
        assert_eq!(ollama_endpoint(), "http://custom-ollama.local:8080");
        std::env::remove_var("OLLAMA_HOST");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_https() {
        std::env::set_var("OLLAMA_HOST", "https://secure-ollama.example.com");
        assert_eq!(ollama_endpoint(), "https://secure-ollama.example.com");
        std::env::remove_var("OLLAMA_HOST");
    }

    #[tokio::test]
    async fn test_fetch_model_context_length_nonexistent_endpoint() {
        let result = fetch_model_context_length("http://localhost:99999", "test-model").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    #[ignore = "requires llama.krohnos.io endpoint"]
    async fn test_fetch_model_context_length_real_endpoint() {
        let result =
            fetch_model_context_length("https://llama.krohnos.io", "qwen3-4b-instruct-2507-q8_0")
                .await;
        assert!(result.is_some());
        assert!(result.unwrap() > 0);
    }
}
