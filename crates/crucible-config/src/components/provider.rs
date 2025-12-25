//! Unified provider configuration
//!
//! This module defines the `ProviderConfig` struct that represents a single
//! provider instance configuration. A provider is a named instance of a backend
//! with specific settings like endpoint, models, and authentication.
//!
//! API keys can be specified using:
//! - Direct value: `api_key = "sk-123..."`
//! - Environment variable: `api_key = "{env:OPENAI_API_KEY}"`
//! - File reference: `api_key = "{file:~/.secrets/openai.key}"`
//!
//! The `{env:}` and `{file:}` patterns are resolved at config load time.

use super::backend::BackendType;
use serde::{Deserialize, Serialize};

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

    /// API key (resolved from `{env:VAR}` or `{file:path}` at load time)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

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
        self.models.embedding.clone().or_else(|| {
            self.backend
                .default_embedding_model()
                .map(|s| s.to_string())
        })
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

    /// Get the API key (already resolved from `{env:}` or `{file:}` at load time)
    pub fn api_key(&self) -> Option<String> {
        self.api_key.clone()
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

    /// Set the API key (use `{env:VAR}` syntax for env vars)
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
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

    /// Convert to EmbeddingProviderConfig for use with LLM crate
    ///
    /// This creates the appropriate embedding provider configuration based on
    /// the backend type.
    pub fn to_embedding_provider_config(&self) -> crate::enrichment::EmbeddingProviderConfig {
        match self.backend {
            BackendType::FastEmbed => {
                crate::enrichment::EmbeddingProviderConfig::FastEmbed(
                    crate::enrichment::FastEmbedConfig {
                        model: self
                            .embedding_model()
                            .unwrap_or_else(|| "BAAI/bge-small-en-v1.5".to_string()),
                        cache_dir: None, // Use default
                        batch_size: self.batch_size() as u32,
                        num_threads: None,
                        dimensions: 0, // Use default
                    },
                )
            }
            BackendType::OpenAI => crate::enrichment::EmbeddingProviderConfig::OpenAI(
                crate::enrichment::OpenAIConfig {
                    api_key: self.api_key().unwrap_or_default(),
                    model: self
                        .embedding_model()
                        .unwrap_or_else(|| "text-embedding-3-small".to_string()),
                    base_url: self
                        .endpoint()
                        .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                    timeout_seconds: self.timeout_secs,
                    retry_attempts: 3,
                    dimensions: 0,
                    headers: std::collections::HashMap::new(),
                },
            ),
            BackendType::Ollama => crate::enrichment::EmbeddingProviderConfig::Ollama(
                crate::enrichment::OllamaConfig {
                    model: self
                        .embedding_model()
                        .unwrap_or_else(|| "nomic-embed-text".to_string()),
                    base_url: self
                        .endpoint()
                        .unwrap_or_else(|| "http://localhost:11434".to_string()),
                    timeout_seconds: self.timeout_secs,
                    retry_attempts: 3,
                    dimensions: 0,
                    batch_size: self.batch_size() as u32,
                },
            ),
            BackendType::Mock => {
                crate::enrichment::EmbeddingProviderConfig::Mock(crate::enrichment::MockConfig {
                    model: "mock-test-model".to_string(),
                    dimensions: 768,
                    simulated_latency_ms: 0,
                })
            }
            BackendType::Burn => crate::enrichment::EmbeddingProviderConfig::Burn(
                crate::enrichment::BurnEmbedConfig {
                    model: self
                        .embedding_model()
                        .unwrap_or_else(|| "nomic-embed-text".to_string()),
                    backend: crate::enrichment::BurnBackendConfig::Auto,
                    model_dir: crate::enrichment::BurnEmbedConfig::default_model_dir(),
                    model_search_paths: Vec::new(),
                    dimensions: 0,
                },
            ),
            BackendType::LlamaCpp => crate::enrichment::EmbeddingProviderConfig::LlamaCpp(
                crate::enrichment::LlamaCppConfig {
                    model_path: self
                        .embedding_model()
                        .unwrap_or_else(|| "nomic-embed-text-v1.5.Q8_0.gguf".to_string()),
                    device: "auto".to_string(),
                    gpu_layers: -1,
                    batch_size: self.batch_size(),
                    context_size: 512,
                    dimensions: 0,
                },
            ),
            // Fallback for backends without embedding support
            _ => crate::enrichment::EmbeddingProviderConfig::FastEmbed(
                crate::enrichment::FastEmbedConfig {
                    model: "BAAI/bge-small-en-v1.5".to_string(),
                    cache_dir: None,
                    batch_size: 16,
                    num_threads: None,
                    dimensions: 0,
                },
            ),
        }
    }

    /// Create a ProviderConfig from a legacy EmbeddingConfig
    ///
    /// This is used for automatic migration from the old `[embedding]` config
    /// format to the new `[providers]` format.
    pub fn from_legacy_embedding(config: &super::EmbeddingConfig) -> Self {
        use super::EmbeddingProviderType;

        let backend = match config.provider {
            EmbeddingProviderType::FastEmbed => BackendType::FastEmbed,
            EmbeddingProviderType::OpenAI => BackendType::OpenAI,
            EmbeddingProviderType::Anthropic => BackendType::Anthropic,
            EmbeddingProviderType::Ollama => BackendType::Ollama,
            EmbeddingProviderType::Cohere => BackendType::Cohere,
            EmbeddingProviderType::VertexAI => BackendType::VertexAI,
            EmbeddingProviderType::Custom => BackendType::Custom,
            EmbeddingProviderType::Mock => BackendType::Mock,
            EmbeddingProviderType::Burn => BackendType::Burn,
            EmbeddingProviderType::LlamaCpp => BackendType::LlamaCpp,
            EmbeddingProviderType::None => BackendType::FastEmbed, // Default fallback
        };

        let mut provider_config = Self::new(backend);
        provider_config.endpoint = config.api_url.clone();
        provider_config.batch_size = Some(config.batch_size);
        provider_config.max_concurrent = config.max_concurrent;

        if let Some(model) = &config.model {
            provider_config.models.embedding = Some(model.clone());
        }

        provider_config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_direct() {
        // With new model, api_key is the resolved value
        let config = ProviderConfig::new(BackendType::OpenAI)
            .with_api_key("sk-direct-key");
        assert_eq!(config.api_key(), Some("sk-direct-key".to_string()));
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
        // Ensure environment variables are clear for consistent validation
        let env_vars = [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "COHERE_API_KEY",
            "GOOGLE_API_KEY",
        ];
        for var in env_vars {
            std::env::remove_var(var);
        }

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
