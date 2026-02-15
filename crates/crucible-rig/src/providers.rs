//! Provider factory for creating Rig clients from Crucible configuration.
//!
//! This module maps `LlmProviderConfig` from crucible-config to Rig provider clients.

use crate::github_copilot::CopilotClient;
use crucible_config::llm::{LlmProviderConfig, LlmProviderType};
use rig::client::Nothing;
use rig::providers::{anthropic, ollama, openai, openrouter};
use thiserror::Error;

/// Errors from Rig provider operations
#[derive(Debug, Error)]
pub enum RigError {
    /// Missing required API key
    #[error("Missing API key for provider {provider}: set {env_var} environment variable")]
    MissingApiKey {
        /// Provider name
        provider: String,
        /// Expected environment variable
        env_var: String,
    },

    /// Provider not supported
    #[error("Provider type not supported: {0:?}")]
    UnsupportedProvider(LlmProviderType),

    /// Client creation failed
    #[error("Failed to create client: {0}")]
    ClientCreation(String),

    /// GitHub Copilot requires OAuth authentication
    #[error("GitHub Copilot requires OAuth authentication. Run device flow first or provide OAuth token via api_key.")]
    CopilotAuthRequired,
}

/// Result type for Rig operations
pub type RigResult<T> = Result<T, RigError>;

/// Enum wrapping different Rig client types
///
/// Since each provider has a different client type, we use an enum
/// to provide a unified interface.
#[derive(Debug, Clone)]
pub enum RigClient {
    /// Ollama client for local LLM inference
    Ollama(ollama::Client),
    /// OpenAI client (new responses API)
    OpenAI(openai::Client),
    /// OpenAI-compatible client (standard /chat/completions API)
    /// Use this for llama.cpp, vLLM, or other OpenAI-compatible servers
    OpenAICompat(openai::CompletionsClient),
    /// Anthropic client
    Anthropic(anthropic::Client),
    /// GitHub Copilot client (uses OAuth + Copilot API token exchange)
    GitHubCopilot(CopilotClient),
    /// OpenRouter client (meta-provider for multiple LLM APIs)
    OpenRouter(openrouter::Client),
}

impl RigClient {
    /// Get the provider name
    pub fn provider_name(&self) -> &'static str {
        match self {
            RigClient::Ollama(_) => "ollama",
            RigClient::OpenAI(_) => "openai",
            RigClient::OpenAICompat(_) => "openai-compat",
            RigClient::Anthropic(_) => "anthropic",
            RigClient::GitHubCopilot(_) => "github-copilot",
            RigClient::OpenRouter(_) => "openrouter",
        }
    }

    /// Get the inner Ollama client, if this is an Ollama client.
    pub fn as_ollama(&self) -> Option<&ollama::Client> {
        match self {
            RigClient::Ollama(c) => Some(c),
            _ => None,
        }
    }

    /// Get the inner OpenAI client, if this is an OpenAI client.
    pub fn as_openai(&self) -> Option<&openai::Client> {
        match self {
            RigClient::OpenAI(c) => Some(c),
            _ => None,
        }
    }

    /// Get the inner Anthropic client, if this is an Anthropic client.
    pub fn as_anthropic(&self) -> Option<&anthropic::Client> {
        match self {
            RigClient::Anthropic(c) => Some(c),
            _ => None,
        }
    }

    /// Get the inner OpenAI-compatible client, if this is an OpenAI-compatible client.
    ///
    /// This client uses the standard `/chat/completions` API rather than the
    /// newer "responses" API, making it compatible with llama.cpp, vLLM,
    /// and other OpenAI-compatible servers.
    pub fn as_openai_compat(&self) -> Option<&openai::CompletionsClient> {
        match self {
            RigClient::OpenAICompat(c) => Some(c),
            _ => None,
        }
    }

    /// Get the inner GitHub Copilot client, if this is a GitHub Copilot client.
    ///
    /// This client handles OAuth token exchange with GitHub's Copilot API,
    /// automatically refreshing the API token (30-minute TTL) as needed.
    pub fn as_github_copilot(&self) -> Option<&CopilotClient> {
        match self {
            RigClient::GitHubCopilot(c) => Some(c),
            _ => None,
        }
    }
}

/// Create a Rig client from Crucible LLM provider configuration.
///
/// # Arguments
///
/// * `config` - The LLM provider configuration from crucible-config
///
/// # Returns
///
/// A `RigClient` enum wrapping the appropriate provider client.
///
/// # Errors
///
/// Returns an error if:
/// - Required API key is missing for OpenAI/Anthropic
/// - Provider type is not supported
///
/// # Example
///
/// ```rust,ignore
/// use crucible_config::components::llm::{LlmProviderConfig, LlmProviderType};
/// use crucible_rig::providers::create_client;
///
/// let config = LlmProviderConfig {
///     provider_type: LlmProviderType::Ollama,
///     endpoint: Some("http://localhost:11434".into()),
///     default_model: Some("llama3.2".into()),
///     ..Default::default()
/// };
///
/// let client = create_client(&config)?;
/// ```
pub fn create_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    match config.provider_type {
        LlmProviderType::Ollama => create_ollama_client(config),
        LlmProviderType::OpenAI => create_openai_client(config),
        LlmProviderType::Anthropic => create_anthropic_client(config),
        LlmProviderType::GitHubCopilot => create_github_copilot_client(config),
        LlmProviderType::OpenRouter => create_openrouter_client(config),
    }
}

/// Create an Ollama client
fn create_ollama_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let endpoint = config.endpoint();

    tracing::debug!(endpoint = %endpoint, "Creating Ollama client");

    // Ollama uses builder pattern with Nothing as API key
    let client = if endpoint != "http://localhost:11434" {
        // Custom endpoint
        ollama::Client::builder()
            .api_key(Nothing)
            .base_url(&endpoint)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?
    } else {
        // Default endpoint
        ollama::Client::builder()
            .api_key(Nothing)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?
    };

    Ok(RigClient::Ollama(client))
}

/// Create an OpenAI client
///
/// For custom endpoints (llama.cpp, vLLM, etc.), this returns an OpenAICompat
/// client using the standard `/chat/completions` API. For the real OpenAI API,
/// it returns the standard OpenAI client.
fn create_openai_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let endpoint = config.endpoint();
    let is_real_openai = endpoint == "https://api.openai.com/v1";

    tracing::debug!(endpoint = %endpoint, is_real_openai, "Creating OpenAI client");

    if is_real_openai {
        // Real OpenAI - requires API key, uses responses API
        let api_key = config.api_key().ok_or_else(|| RigError::MissingApiKey {
            provider: "OpenAI".into(),
            env_var: config
                .api_key
                .clone()
                .unwrap_or_else(|| "OPENAI_API_KEY".into()),
        })?;

        let client = openai::Client::builder()
            .api_key(&api_key)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?;

        Ok(RigClient::OpenAI(client))
    } else {
        // OpenAI-compatible endpoint (llama.cpp, vLLM, etc.)
        // Use CompletionsClient for standard /chat/completions API
        // API key is optional for local servers
        let api_key = config.api_key().unwrap_or_else(|| "not-needed".to_string());

        let client = openai::CompletionsClient::builder()
            .api_key(&api_key)
            .base_url(&endpoint)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?;

        Ok(RigClient::OpenAICompat(client))
    }
}

/// Create an Anthropic client
///
/// Supports custom endpoints (e.g., Anthropic-compatible APIs) via the `endpoint` config field.
/// If no endpoint is specified, uses the default Anthropic API.
fn create_anthropic_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let api_key = config.api_key().ok_or_else(|| RigError::MissingApiKey {
        provider: "Anthropic".into(),
        env_var: config
            .api_key
            .clone()
            .unwrap_or_else(|| "ANTHROPIC_API_KEY".into()),
    })?;

    let endpoint = config.endpoint();
    let is_default_endpoint = endpoint == "https://api.anthropic.com/v1";

    tracing::debug!(endpoint = %endpoint, is_default_endpoint, "Creating Anthropic client");

    let client = if is_default_endpoint {
        // Default Anthropic API
        anthropic::Client::builder()
            .api_key(api_key)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?
    } else {
        // Custom endpoint (Anthropic-compatible API)
        anthropic::Client::builder()
            .api_key(api_key)
            .base_url(&endpoint)
            .build()
            .map_err(|e| RigError::ClientCreation(e.to_string()))?
    };

    Ok(RigClient::Anthropic(client))
}

/// Create a GitHub Copilot client
///
/// GitHub Copilot requires an OAuth token (obtained via device flow authentication).
/// The token should be stored in the config's `api_key` field.
///
/// To obtain an OAuth token, use [`crate::github_copilot::CopilotAuth`]:
///
/// ```rust,ignore
/// use crucible_rig::github_copilot::CopilotAuth;
///
/// let auth = CopilotAuth::new();
/// let oauth_token = auth.complete_device_flow(|code, uri| {
///     println!("Visit {} and enter code: {}", uri, code);
/// }).await?;
///
/// // Save oauth_token.access_token to config
/// ```
fn create_github_copilot_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let oauth_token = config.api_key().ok_or(RigError::CopilotAuthRequired)?;

    tracing::debug!("Creating GitHub Copilot client");

    let client = CopilotClient::new(oauth_token);

    Ok(RigClient::GitHubCopilot(client))
}

fn create_openrouter_client(config: &LlmProviderConfig) -> RigResult<RigClient> {
    let api_key = config.api_key().ok_or_else(|| RigError::MissingApiKey {
        provider: "OpenRouter".into(),
        env_var: "OPENROUTER_API_KEY".into(),
    })?;

    tracing::debug!("Creating OpenRouter client");

    let client =
        openrouter::Client::new(&api_key).map_err(|e| RigError::ClientCreation(e.to_string()))?;

    Ok(RigClient::OpenRouter(client))
}

/// Create an OpenAI-compatible client with explicit credentials.
///
/// This is useful for creating clients from dynamically-obtained tokens,
/// such as GitHub Copilot's API tokens.
pub fn create_openai_compat_client(
    api_key: &str,
    base_url: &str,
) -> RigResult<openai::CompletionsClient> {
    openai::CompletionsClient::builder()
        .api_key(api_key)
        .base_url(base_url)
        .build()
        .map_err(|e| RigError::ClientCreation(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ollama_config() -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: LlmProviderType::Ollama,
            endpoint: None,
            default_model: Some("llama3.2".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        }
    }

    fn ollama_config_custom_endpoint() -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: LlmProviderType::Ollama,
            endpoint: Some("http://192.168.1.100:11434".into()),
            default_model: Some("llama3.2".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        }
    }

    fn openai_config_with_key() -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: None,
            default_model: Some("gpt-4o".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("TEST_OPENAI_KEY".into()),
        }
    }

    fn anthropic_config_with_key() -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: LlmProviderType::Anthropic,
            endpoint: None,
            default_model: Some("claude-3-5-sonnet-20241022".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("TEST_ANTHROPIC_KEY".into()),
        }
    }

    fn copilot_config_with_token() -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: LlmProviderType::GitHubCopilot,
            endpoint: None,
            default_model: Some("gpt-4o".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("gho_test_oauth_token".into()),
        }
    }

    fn copilot_config_no_token() -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: LlmProviderType::GitHubCopilot,
            endpoint: None,
            default_model: Some("gpt-4o".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        }
    }

    #[test]
    fn test_create_ollama_client_default_endpoint() {
        let config = ollama_config();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "ollama");
    }

    #[test]
    fn test_create_ollama_client_custom_endpoint() {
        let config = ollama_config_custom_endpoint();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "ollama");
    }

    #[test]
    fn test_create_openai_client_with_api_key() {
        // Set test API key
        std::env::set_var("TEST_OPENAI_KEY", "test-key-12345");

        let config = openai_config_with_key();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "openai");

        std::env::remove_var("TEST_OPENAI_KEY");
    }

    #[test]
    fn test_create_openai_client_missing_api_key() {
        // With the new {env:VAR} system, api_key is None if no key is provided
        // (env var resolution happens at config load time, not client creation time)
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: None, // Real OpenAI endpoint requires API key
            default_model: Some("gpt-4o".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None, // No API key provided
        };

        let client = create_client(&config);

        assert!(client.is_err());
        let err = client.unwrap_err();
        assert!(matches!(err, RigError::MissingApiKey { .. }));
    }

    #[test]
    fn test_create_anthropic_client_with_api_key() {
        // Set test API key
        std::env::set_var("TEST_ANTHROPIC_KEY", "test-key-67890");

        let config = anthropic_config_with_key();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "anthropic");

        std::env::remove_var("TEST_ANTHROPIC_KEY");
    }

    #[test]
    fn test_create_anthropic_client_missing_api_key() {
        // With the new {env:VAR} system, api_key is None if no key is provided
        // (env var resolution happens at config load time, not client creation time)
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::Anthropic,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None, // No API key provided
        };

        let client = create_client(&config);

        assert!(client.is_err());
        let err = client.unwrap_err();
        assert!(matches!(err, RigError::MissingApiKey { .. }));
    }

    #[test]
    fn test_rig_client_provider_names() {
        // Ollama (no API key needed)
        let ollama = create_client(&ollama_config()).unwrap();
        assert_eq!(ollama.provider_name(), "ollama");

        // OpenAI
        std::env::set_var("TEST_OPENAI_KEY", "test");
        let openai = create_client(&openai_config_with_key()).unwrap();
        assert_eq!(openai.provider_name(), "openai");
        std::env::remove_var("TEST_OPENAI_KEY");

        // Anthropic
        std::env::set_var("TEST_ANTHROPIC_KEY", "test");
        let anthropic = create_client(&anthropic_config_with_key()).unwrap();
        assert_eq!(anthropic.provider_name(), "anthropic");
        std::env::remove_var("TEST_ANTHROPIC_KEY");
    }

    #[test]
    fn test_create_openai_compat_client_with_custom_endpoint() {
        // OpenAI with custom endpoint should return OpenAICompat variant
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: Some("https://llama.example.com/v1".into()),
            default_model: Some("qwen3-8b".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None, // No API key needed for local servers
        };

        let client = create_client(&config);
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "openai-compat");
        assert!(client.as_openai_compat().is_some());
        assert!(client.as_openai().is_none());
    }

    #[test]
    fn test_create_openai_compat_no_api_key_required() {
        // OpenAI-compatible endpoints don't require API key
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: Some("http://localhost:8080/v1".into()),
            default_model: Some("local-model".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("NONEXISTENT_API_KEY".into()), // Won't fail even if not set
        };

        // Should succeed without API key
        let client = create_client(&config);
        assert!(client.is_ok());
        assert_eq!(client.unwrap().provider_name(), "openai-compat");
    }

    #[test]
    fn test_real_openai_requires_api_key() {
        // Real OpenAI API (default endpoint) requires API key
        // With the new {env:VAR} system, api_key is None if no key is provided
        let config = LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: None, // Uses default https://api.openai.com/v1
            default_model: Some("gpt-4o".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None, // No API key provided
        };

        let client = create_client(&config);
        assert!(client.is_err());
        assert!(matches!(
            client.unwrap_err(),
            RigError::MissingApiKey { .. }
        ));
    }

    #[test]
    fn test_create_github_copilot_client_with_token() {
        let config = copilot_config_with_token();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "github-copilot");
        assert!(client.as_github_copilot().is_some());
    }

    #[test]
    fn test_create_github_copilot_client_missing_token() {
        // GitHub Copilot requires an OAuth token
        let config = copilot_config_no_token();
        let client = create_client(&config);

        assert!(client.is_err());
        let err = client.unwrap_err();
        assert!(matches!(err, RigError::CopilotAuthRequired));
    }

    #[test]
    fn test_github_copilot_oauth_token_preserved() {
        let config = copilot_config_with_token();
        let client = create_client(&config).unwrap();
        let copilot = client.as_github_copilot().unwrap();

        // OAuth token should be preserved
        assert_eq!(copilot.oauth_token(), "gho_test_oauth_token");
    }

    fn openrouter_config_with_key() -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: LlmProviderType::OpenRouter,
            endpoint: None,
            default_model: Some("openai/gpt-4o".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("sk-or-test-key".into()),
        }
    }

    fn openrouter_config_no_key() -> LlmProviderConfig {
        LlmProviderConfig {
            provider_type: LlmProviderType::OpenRouter,
            endpoint: None,
            default_model: Some("openai/gpt-4o".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
        }
    }

    #[test]
    fn test_create_openrouter_client_with_api_key() {
        let config = openrouter_config_with_key();
        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "openrouter");
    }

    #[test]
    fn test_create_openrouter_client_missing_api_key() {
        let config = openrouter_config_no_key();
        let client = create_client(&config);

        assert!(client.is_err());
        let err = client.unwrap_err();
        assert!(matches!(err, RigError::MissingApiKey { .. }));
    }

    #[test]
    fn test_create_anthropic_client_custom_endpoint() {
        std::env::set_var("TEST_ANTHROPIC_KEY", "test-key-custom");

        let config = LlmProviderConfig {
            provider_type: LlmProviderType::Anthropic,
            endpoint: Some("https://api.z.ai/api/anthropic".into()),
            default_model: Some("glm-4-flash".into()),
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: Some("TEST_ANTHROPIC_KEY".into()),
        };

        let client = create_client(&config);

        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.provider_name(), "anthropic");

        std::env::remove_var("TEST_ANTHROPIC_KEY");
    }
}
