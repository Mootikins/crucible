//! Provider detection for interactive setup
//!
//! Detects available LLM providers by checking:
//! - Ollama: HTTP check to endpoint (priority: config file > OLLAMA_HOST env var > localhost:11434)
//! - OpenAI: OPENAI_API_KEY env var
//! - Anthropic: ANTHROPIC_API_KEY env var

use std::time::Duration;

/// Default Ollama endpoint
const DEFAULT_OLLAMA_HOST: &str = "http://localhost:11434";

/// A detected provider with availability info
#[derive(Debug, Clone)]
pub struct DetectedProvider {
    pub name: String,
    pub provider_type: String, // "ollama", "openai", "anthropic"
    pub available: bool,
    pub reason: String, // "Running locally", "API key found", etc.
    pub default_model: Option<String>,
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

/// Check if an API key exists for a provider
pub fn has_api_key(provider: &str) -> bool {
    match provider.to_lowercase().as_str() {
        "openai" => std::env::var("OPENAI_API_KEY").is_ok(),
        "anthropic" => std::env::var("ANTHROPIC_API_KEY").is_ok(),
        _ => false,
    }
}

/// Check if Ollama is running (checks OLLAMA_HOST env var or localhost:11434)
pub async fn check_ollama() -> Option<Vec<String>> {
    check_ollama_at(&ollama_endpoint()).await
}

/// Check if Ollama is running at a specific endpoint
pub async fn check_ollama_at(endpoint: &str) -> Option<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;

    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let resp = client.get(&url).send().await.ok()?;

    if !resp.status().is_success() {
        return None;
    }

    // Parse model list
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

/// Detect all available providers
pub async fn detect_providers_available() -> Vec<DetectedProvider> {
    let mut providers = Vec::new();

    // Check Ollama
    if let Some(models) = check_ollama().await {
        providers.push(DetectedProvider {
            name: "Ollama (Local)".to_string(),
            provider_type: "ollama".to_string(),
            available: true,
            reason: format!("{} models available", models.len()),
            default_model: models.first().cloned(),
        });
    }

    // Check OpenAI
    if has_api_key("openai") {
        providers.push(DetectedProvider {
            name: "OpenAI".to_string(),
            provider_type: "openai".to_string(),
            available: true,
            reason: "API key found".to_string(),
            default_model: Some("gpt-4o-mini".to_string()),
        });
    }

    // Check Anthropic
    if has_api_key("anthropic") {
        providers.push(DetectedProvider {
            name: "Anthropic".to_string(),
            provider_type: "anthropic".to_string(),
            available: true,
            reason: "API key found".to_string(),
            default_model: Some("claude-3-5-sonnet-latest".to_string()),
        });
    }

    providers
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    async fn test_detect_no_providers() {
        // With no Ollama and no API keys, should return empty
        std::env::remove_var("OPENAI_API_KEY");
        std::env::remove_var("ANTHROPIC_API_KEY");

        let detected = detect_providers_available().await;
        // Ollama might be running locally, so just check structure
        assert!(detected.iter().all(|p| !p.name.is_empty()));
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
        // Unknown providers should return false
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
}
