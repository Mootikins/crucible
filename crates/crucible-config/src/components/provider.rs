//! Unified provider configuration
//!
//! This module defines the `ProviderConfig` struct that represents a single
//! provider instance configuration. A provider is a named instance of a backend
//! with specific settings like endpoint, models, and authentication.

use super::backend::BackendType;
use serde::{Deserialize, Serialize};

/// Configuration for API key authentication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ApiKeyConfig {
    /// API key value provided directly (not recommended for production)
    Value(String),
    /// API key from environment variable (recommended)
    EnvVar {
        /// Environment variable name containing the API key
        env: String,
    },
}

impl ApiKeyConfig {
    /// Create an ApiKeyConfig from an environment variable name
    pub fn from_env(env_var: &str) -> Self {
        Self::EnvVar {
            env: env_var.to_string(),
        }
    }

    /// Create an ApiKeyConfig from a direct value
    pub fn from_value(value: &str) -> Self {
        Self::Value(value.to_string())
    }

    /// Resolve the API key value
    ///
    /// For `EnvVar`, reads from the environment variable.
    /// For `Value`, returns the value directly.
    pub fn resolve(&self) -> Option<String> {
        match self {
            Self::Value(v) => Some(v.clone()),
            Self::EnvVar { env } => std::env::var(env).ok(),
        }
    }

    /// Check if the API key is available (can be resolved)
    pub fn is_available(&self) -> bool {
        self.resolve().is_some()
    }
}

/// Model configuration for a provider
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModelConfig {
    /// Model to use for embeddings
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<String>,
    /// Model to use for chat
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
}

impl ModelConfig {
    /// Create a new ModelConfig with the given embedding model
    pub fn with_embedding(model: impl Into<String>) -> Self {
        Self {
            embedding: Some(model.into()),
            chat: None,
        }
    }

    /// Create a new ModelConfig with the given chat model
    pub fn with_chat(model: impl Into<String>) -> Self {
        Self {
            embedding: None,
            chat: Some(model.into()),
        }
    }

    /// Create a new ModelConfig with both embedding and chat models
    pub fn with_both(embedding: impl Into<String>, chat: impl Into<String>) -> Self {
        Self {
            embedding: Some(embedding.into()),
            chat: Some(chat.into()),
        }
    }
}

/// Unified provider instance configuration
///
/// A provider represents a configured instance of a backend service.
/// Multiple providers can use the same backend with different settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// The backend type this provider uses
    pub backend: BackendType,

    /// Custom API endpoint (uses backend default if not set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// API key configuration (required for cloud providers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<ApiKeyConfig>,

    /// Timeout for API calls in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Model configuration
    #[serde(default)]
    pub models: ModelConfig,

    /// Maximum concurrent requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<usize>,

    /// Temperature for generation (0.0-2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,

    /// Batch size for embedding operations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,
}

fn default_timeout() -> u64 {
    120
}

impl ProviderConfig {
    /// Create a new provider config for the given backend with defaults
    pub fn new(backend: BackendType) -> Self {
        Self {
            backend,
            endpoint: None,
            api_key: None,
            timeout_secs: default_timeout(),
            models: ModelConfig::default(),
            max_concurrent: None,
            temperature: None,
            max_tokens: None,
            batch_size: None,
        }
    }

    /// Get the effective endpoint (configured or backend default)
    pub fn endpoint(&self) -> Option<String> {
        self.endpoint
            .clone()
            .or_else(|| self.backend.default_endpoint().map(|s| s.to_string()))
    }

    /// Get the effective embedding model (configured or backend default)
    pub fn embedding_model(&self) -> Option<String> {
        self.models
            .embedding
            .clone()
            .or_else(|| self.backend.default_embedding_model().map(|s| s.to_string()))
    }

    /// Get the effective chat model (configured or backend default)
    pub fn chat_model(&self) -> Option<String> {
        self.models
            .chat
            .clone()
            .or_else(|| self.backend.default_chat_model().map(|s| s.to_string()))
    }

    /// Get the effective max concurrent requests
    pub fn max_concurrent(&self) -> usize {
        self.max_concurrent
            .unwrap_or_else(|| self.backend.default_max_concurrent())
    }

    /// Get the effective temperature (default 0.7)
    pub fn temperature(&self) -> f32 {
        self.temperature.unwrap_or(0.7)
    }

    /// Get the effective max tokens (default 4096)
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens.unwrap_or(4096)
    }

    /// Get the effective batch size (default 16)
    pub fn batch_size(&self) -> usize {
        self.batch_size.unwrap_or(16)
    }

    /// Resolve the API key
    pub fn api_key(&self) -> Option<String> {
        self.api_key
            .as_ref()
            .and_then(|k| k.resolve())
            .or_else(|| {
                // Fallback to default env var for the backend
                self.backend
                    .default_api_key_env()
                    .and_then(|env| std::env::var(env).ok())
            })
    }

    /// Check if this provider supports embeddings
    pub fn supports_embeddings(&self) -> bool {
        self.backend.supports_embeddings()
    }

    /// Check if this provider supports chat
    pub fn supports_chat(&self) -> bool {
        self.backend.supports_chat()
    }

    /// Check if the provider is properly configured
    ///
    /// Returns an error message if configuration is invalid
    pub fn validate(&self) -> Result<(), String> {
        // Check API key requirement
        if self.backend.requires_api_key() && self.api_key().is_none() {
            return Err(format!(
                "Backend {} requires an API key but none is configured",
                self.backend
            ));
        }

        // Check endpoint for custom backend
        if matches!(self.backend, BackendType::Custom) && self.endpoint.is_none() {
            return Err("Custom backend requires an endpoint to be configured".to_string());
        }

        Ok(())
    }

    // === Builder pattern for fluent configuration ===

    /// Set the endpoint
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Set the API key from environment variable
    pub fn with_api_key_env(mut self, env_var: impl Into<String>) -> Self {
        self.api_key = Some(ApiKeyConfig::EnvVar {
            env: env_var.into(),
        });
        self
    }

    /// Set the API key directly
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(ApiKeyConfig::Value(key.into()));
        self
    }

    /// Set the models
    pub fn with_models(mut self, models: ModelConfig) -> Self {
        self.models = models;
        self
    }

    /// Set the embedding model
    pub fn with_embedding_model(mut self, model: impl Into<String>) -> Self {
        self.models.embedding = Some(model.into());
        self
    }

    /// Set the chat model
    pub fn with_chat_model(mut self, model: impl Into<String>) -> Self {
        self.models.chat = Some(model.into());
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set max concurrent requests
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = Some(max);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = Some(max);
        self
    }

    /// Set batch size
    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = Some(size);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_from_env() {
        std::env::set_var("TEST_PROVIDER_KEY", "secret-key-123");

        let config = ApiKeyConfig::from_env("TEST_PROVIDER_KEY");
        assert_eq!(config.resolve(), Some("secret-key-123".to_string()));
        assert!(config.is_available());

        std::env::remove_var("TEST_PROVIDER_KEY");
        assert!(config.resolve().is_none());
        assert!(!config.is_available());
    }

    #[test]
    fn test_api_key_value() {
        let config = ApiKeyConfig::from_value("direct-key");
        assert_eq!(config.resolve(), Some("direct-key".to_string()));
        assert!(config.is_available());
    }

    #[test]
    fn test_provider_config_defaults() {
        let config = ProviderConfig::new(BackendType::Ollama);

        assert_eq!(
            config.endpoint(),
            Some("http://localhost:11434".to_string())
        );
        assert_eq!(
            config.embedding_model(),
            Some("nomic-embed-text".to_string())
        );
        assert_eq!(config.chat_model(), Some("llama3.2".to_string()));
        assert_eq!(config.max_concurrent(), 1);
        assert!(config.supports_embeddings());
        assert!(config.supports_chat());
    }

    #[test]
    fn test_provider_config_builder() {
        let config = ProviderConfig::new(BackendType::OpenAI)
            .with_endpoint("https://custom.openai.com")
            .with_api_key("sk-test-key")
            .with_embedding_model("text-embedding-ada-002")
            .with_chat_model("gpt-3.5-turbo")
            .with_temperature(0.9)
            .with_max_tokens(8192);

        assert_eq!(
            config.endpoint(),
            Some("https://custom.openai.com".to_string())
        );
        assert_eq!(config.api_key(), Some("sk-test-key".to_string()));
        assert_eq!(
            config.embedding_model(),
            Some("text-embedding-ada-002".to_string())
        );
        assert_eq!(config.chat_model(), Some("gpt-3.5-turbo".to_string()));
        assert_eq!(config.temperature(), 0.9);
        assert_eq!(config.max_tokens(), 8192);
    }

    #[test]
    fn test_provider_validation() {
        // OpenAI without API key should fail
        let config = ProviderConfig::new(BackendType::OpenAI);
        assert!(config.validate().is_err());

        // OpenAI with API key should pass
        let config = ProviderConfig::new(BackendType::OpenAI).with_api_key("sk-test");
        assert!(config.validate().is_ok());

        // FastEmbed doesn't need API key
        let config = ProviderConfig::new(BackendType::FastEmbed);
        assert!(config.validate().is_ok());

        // Custom without endpoint should fail
        let config = ProviderConfig::new(BackendType::Custom);
        assert!(config.validate().is_err());

        // Custom with endpoint should pass
        let config =
            ProviderConfig::new(BackendType::Custom).with_endpoint("http://my-server.local");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_serde_roundtrip() {
        let config = ProviderConfig::new(BackendType::Ollama)
            .with_endpoint("http://192.168.1.100:11434")
            .with_embedding_model("mxbai-embed-large")
            .with_chat_model("llama3.1:70b");

        let toml = toml::to_string_pretty(&config).unwrap();
        let parsed: ProviderConfig = toml::from_str(&toml).unwrap();

        assert_eq!(parsed.backend, BackendType::Ollama);
        assert_eq!(
            parsed.endpoint,
            Some("http://192.168.1.100:11434".to_string())
        );
        assert_eq!(
            parsed.models.embedding,
            Some("mxbai-embed-large".to_string())
        );
        assert_eq!(parsed.models.chat, Some("llama3.1:70b".to_string()));
    }
}
