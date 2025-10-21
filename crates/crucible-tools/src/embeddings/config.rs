//! Configuration for embedding providers

use serde::{Deserialize, Serialize};
use std::env;
use url::Url;

use super::error::{EmbeddingError, EmbeddingResult};

/// Type of embedding provider
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// Ollama local/remote embedding service
    Ollama,
    /// OpenAI embedding API
    OpenAI,
}

impl ProviderType {
    /// Parse provider type from string
    pub fn from_str(s: &str) -> EmbeddingResult<Self> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(ProviderType::Ollama),
            "openai" => Ok(ProviderType::OpenAI),
            _ => Err(EmbeddingError::ConfigError(format!(
                "Unknown provider type: {}. Valid options: ollama, openai",
                s
            ))),
        }
    }

    /// Get default endpoint for this provider
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            ProviderType::Ollama => "http://localhost:11434",
            ProviderType::OpenAI => "https://api.openai.com/v1",
        }
    }

    /// Get default model for this provider
    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderType::Ollama => "nomic-embed-text",
            ProviderType::OpenAI => "text-embedding-3-small",
        }
    }

    /// Get expected embedding dimensions for this provider's default model
    pub fn default_dimensions(&self) -> usize {
        match self {
            ProviderType::Ollama => 768,  // nomic-embed-text
            ProviderType::OpenAI => 1536, // text-embedding-3-small
        }
    }

    /// Whether this provider requires an API key
    pub fn requires_api_key(&self) -> bool {
        match self {
            ProviderType::Ollama => false,
            ProviderType::OpenAI => true,
        }
    }

    /// Get provider description
    pub fn description(&self) -> &'static str {
        match self {
            ProviderType::Ollama => "Ollama - Local and remote embedding service",
            ProviderType::OpenAI => "OpenAI - Cloud-based embedding API",
        }
    }

    /// Get supported models for this provider
    pub fn supported_models(&self) -> Vec<&'static str> {
        match self {
            ProviderType::Ollama => vec![
                "nomic-embed-text",
                "all-minilm",
                "mxbai-embed-large",
            ],
            ProviderType::OpenAI => vec![
                "text-embedding-3-small",
                "text-embedding-3-large",
                "text-embedding-ada-002",
            ],
        }
    }
}

/// Configuration for embedding provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Provider type (ollama, openai)
    pub provider: ProviderType,

    /// API endpoint URL
    pub endpoint: String,

    /// API key (optional, required for some providers)
    pub api_key: Option<String>,

    /// Model name to use for embeddings
    pub model: String,

    /// Request timeout in seconds
    pub timeout_secs: u64,

    /// Maximum number of retry attempts
    pub max_retries: u32,

    /// Batch size for bulk embedding operations
    pub batch_size: usize,

    /// Additional headers to send with requests
    pub headers: std::collections::HashMap<String, String>,

    /// Whether to validate SSL certificates
    pub validate_ssl: bool,

    /// Request retry delay in milliseconds
    pub retry_delay_ms: u64,

    /// Maximum request size in bytes
    pub max_request_size_bytes: usize,
}

impl EmbeddingConfig {
    /// Create configuration from environment variables
    pub fn from_env() -> EmbeddingResult<Self> {
        let provider_str = env::var("EMBEDDING_PROVIDER")
            .unwrap_or_else(|_| "ollama".to_string());

        let provider = ProviderType::from_str(&provider_str)?;

        let endpoint = env::var("EMBEDDING_ENDPOINT")
            .unwrap_or_else(|_| provider.default_endpoint().to_string());

        let api_key = env::var("EMBEDDING_API_KEY").ok();

        let model = env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| provider.default_model().to_string());

        let timeout_secs = env::var("EMBEDDING_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let max_retries = env::var("EMBEDDING_MAX_RETRIES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        let batch_size = env::var("EMBEDDING_BATCH_SIZE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);  // Default to 1 to avoid Ollama batch size issues

        let retry_delay_ms = env::var("EMBEDDING_RETRY_DELAY_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1000);

        let validate_ssl = env::var("EMBEDDING_VALIDATE_SSL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(true);

        let max_request_size_bytes = env::var("EMBEDDING_MAX_REQUEST_SIZE_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10 * 1024 * 1024); // 10MB

        let config = Self {
            provider,
            endpoint,
            api_key,
            model: model.to_string(),
            timeout_secs,
            max_retries,
            batch_size,
            headers: std::collections::HashMap::new(),
            validate_ssl,
            retry_delay_ms,
            max_request_size_bytes,
        };

        config.validate()?;
        Ok(config)
    }

    /// Create configuration for Ollama provider
    pub fn ollama(endpoint: Option<String>, model: Option<String>) -> Self {
        Self {
            provider: ProviderType::Ollama,
            endpoint: endpoint.unwrap_or_else(|| "http://localhost:11434".to_string()),
            api_key: None,
            model: model.unwrap_or_else(|| "nomic-embed-text".to_string()),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 1,  // Process one file at a time to avoid Ollama batch size issues
            headers: std::collections::HashMap::new(),
            validate_ssl: true,
            retry_delay_ms: 1000,
            max_request_size_bytes: 10 * 1024 * 1024, // 10MB
        }
    }

    /// Create configuration for OpenAI provider
    pub fn openai(api_key: String, model: Option<String>) -> Self {
        Self {
            provider: ProviderType::OpenAI,
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: Some(api_key),
            model: model.unwrap_or_else(|| "text-embedding-3-small".to_string()),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 10,
            headers: std::collections::HashMap::new(),
            validate_ssl: true,
            retry_delay_ms: 1000,
            max_request_size_bytes: 10 * 1024 * 1024, // 10MB
        }
    }

    /// Add a custom header
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    /// Set custom headers
    pub fn with_headers(mut self, headers: std::collections::HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set batch size
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    /// Set max retries
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set SSL validation
    pub fn with_ssl_validation(mut self, validate_ssl: bool) -> Self {
        self.validate_ssl = validate_ssl;
        self
    }

    /// Set retry delay
    pub fn with_retry_delay(mut self, retry_delay_ms: u64) -> Self {
        self.retry_delay_ms = retry_delay_ms;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> EmbeddingResult<()> {
        // Check API key requirement
        if self.provider.requires_api_key() && self.api_key.is_none() {
            return Err(EmbeddingError::ConfigError(format!(
                "Provider {} requires an API key (set EMBEDDING_API_KEY environment variable)",
                match self.provider {
                    ProviderType::OpenAI => "OpenAI",
                    ProviderType::Ollama => "Ollama",
                }
            )));
        }

        // Validate endpoint URL
        if self.endpoint.is_empty() {
            return Err(EmbeddingError::ConfigError(
                "Endpoint URL cannot be empty".to_string()
            ));
        }

        // Try to parse endpoint as URL
        if let Err(_) = Url::parse(&self.endpoint) {
            return Err(EmbeddingError::ConfigError(
                format!("Invalid endpoint URL: {}", self.endpoint)
            ));
        }

        // Validate model name
        if self.model.is_empty() {
            return Err(EmbeddingError::ConfigError(
                "Model name cannot be empty".to_string()
            ));
        }

        // Validate timeout
        if self.timeout_secs == 0 {
            return Err(EmbeddingError::ConfigError(
                "Timeout must be greater than 0".to_string()
            ));
        }

        // Validate batch size
        if self.batch_size == 0 {
            return Err(EmbeddingError::ConfigError(
                "Batch size must be greater than 0".to_string()
            ));
        }

        // Validate max request size
        if self.max_request_size_bytes == 0 {
            return Err(EmbeddingError::ConfigError(
                "Max request size must be greater than 0".to_string()
            ));
        }

        Ok(())
    }

    /// Get expected embedding dimensions based on provider and model
    pub fn expected_dimensions(&self) -> usize {
        // This is a simplified version - in production, we'd have a more
        // comprehensive model dimension mapping
        match (&self.provider, self.model.as_str()) {
            (ProviderType::Ollama, "nomic-embed-text") => 768,
            (ProviderType::Ollama, "all-minilm") => 384,
            (ProviderType::Ollama, "mxbai-embed-large") => 1024,
            (ProviderType::OpenAI, "text-embedding-3-small") => 1536,
            (ProviderType::OpenAI, "text-embedding-3-large") => 3072,
            (ProviderType::OpenAI, "text-embedding-ada-002") => 1536,
            // Default to provider defaults for unknown models
            (ProviderType::Ollama, _) => 768,
            (ProviderType::OpenAI, _) => 1536,
        }
    }

    /// Check if model is supported by provider
    pub fn is_model_supported(&self) -> bool {
        self.provider.supported_models().contains(&self.model.as_str())
    }

    /// Get configuration summary
    pub fn summary(&self) -> String {
        format!(
            "Provider: {}, Model: {}, Endpoint: {}, Batch Size: {}, Timeout: {}s",
            match self.provider {
                ProviderType::Ollama => "Ollama",
                ProviderType::OpenAI => "OpenAI",
            },
            self.model,
            self.endpoint,
            self.batch_size,
            self.timeout_secs
        )
    }

    /// Clone without sensitive information
    pub fn clone_safe(&self) -> Self {
        let mut clone = self.clone();
        if clone.api_key.is_some() {
            clone.api_key = Some("[REDACTED]".to_string());
        }
        clone
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self::ollama(None, None)
    }
}

/// Configuration builder for embedding providers
pub struct EmbeddingConfigBuilder {
    config: EmbeddingConfig,
}

impl EmbeddingConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: EmbeddingConfig::default(),
        }
    }

    /// Set provider type
    pub fn provider(mut self, provider: ProviderType) -> Self {
        self.config.provider = provider;
        self
    }

    /// Set endpoint
    pub fn endpoint<S: Into<String>>(mut self, endpoint: S) -> Self {
        self.config.endpoint = endpoint.into();
        self
    }

    /// Set API key
    pub fn api_key<S: Into<String>>(mut self, api_key: S) -> Self {
        self.config.api_key = Some(api_key.into());
        self
    }

    /// Set model
    pub fn model<S: Into<String>>(mut self, model: S) -> Self {
        self.config.model = model.into();
        self
    }

    /// Set timeout in seconds
    pub fn timeout(mut self, timeout_secs: u64) -> Self {
        self.config.timeout_secs = timeout_secs;
        self
    }

    /// Set max retries
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.config.max_retries = max_retries;
        self
    }

    /// Set batch size
    pub fn batch_size(mut self, batch_size: usize) -> Self {
        self.config.batch_size = batch_size;
        self
    }

    /// Add a header
    pub fn header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.config.headers.insert(key.into(), value.into());
        self
    }

    /// Set SSL validation
    pub fn validate_ssl(mut self, validate_ssl: bool) -> Self {
        self.config.validate_ssl = validate_ssl;
        self
    }

    /// Build the configuration
    pub fn build(self) -> EmbeddingResult<EmbeddingConfig> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for EmbeddingConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_from_str() {
        assert_eq!(ProviderType::from_str("ollama").unwrap(), ProviderType::Ollama);
        assert_eq!(ProviderType::from_str("Ollama").unwrap(), ProviderType::Ollama);
        assert_eq!(ProviderType::from_str("OLLAMA").unwrap(), ProviderType::Ollama);

        assert_eq!(ProviderType::from_str("openai").unwrap(), ProviderType::OpenAI);
        assert_eq!(ProviderType::from_str("OpenAI").unwrap(), ProviderType::OpenAI);
        assert_eq!(ProviderType::from_str("OPENAI").unwrap(), ProviderType::OpenAI);

        assert!(ProviderType::from_str("unknown").is_err());
        assert!(ProviderType::from_str("").is_err());
    }

    #[test]
    fn test_provider_defaults() {
        let ollama = ProviderType::Ollama;
        assert_eq!(ollama.default_endpoint(), "http://localhost:11434");
        assert_eq!(ollama.default_model(), "nomic-embed-text");
        assert_eq!(ollama.default_dimensions(), 768);
        assert!(!ollama.requires_api_key());

        let openai = ProviderType::OpenAI;
        assert_eq!(openai.default_endpoint(), "https://api.openai.com/v1");
        assert_eq!(openai.default_model(), "text-embedding-3-small");
        assert_eq!(openai.default_dimensions(), 1536);
        assert!(openai.requires_api_key());
    }

    #[test]
    fn test_config_validation_success() {
        let config = EmbeddingConfig::ollama(None, None);
        assert!(config.validate().is_ok());

        let config = EmbeddingConfig::openai("test-key".to_string(), None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_requires_api_key_for_openai() {
        let mut config = EmbeddingConfig::openai("test-key".to_string(), None);
        config.api_key = None;

        let result = config.validate();
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, EmbeddingError::ConfigError(_)));
            assert!(e.to_string().contains("requires an API key"));
        }
    }

    #[test]
    fn test_config_builder() {
        let config = EmbeddingConfigBuilder::new()
            .provider(ProviderType::OpenAI)
            .api_key("test-key")
            .model("text-embedding-3-small")
            .timeout(60)
            .batch_size(5)
            .header("User-Agent", "crucible-rune")
            .build()
            .unwrap();

        assert_eq!(config.provider, ProviderType::OpenAI);
        assert_eq!(config.api_key, Some("test-key".to_string()));
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.batch_size, 5);
        assert!(config.headers.contains_key("User-Agent"));
    }

    #[test]
    fn test_expected_dimensions() {
        let config = EmbeddingConfig::ollama(None, Some("nomic-embed-text".to_string()));
        assert_eq!(config.expected_dimensions(), 768);

        let config = EmbeddingConfig::ollama(None, Some("all-minilm".to_string()));
        assert_eq!(config.expected_dimensions(), 384);

        let config = EmbeddingConfig::openai("test-key".to_string(), Some("text-embedding-3-large".to_string()));
        assert_eq!(config.expected_dimensions(), 3072);
    }

    #[test]
    fn test_model_support() {
        let config = EmbeddingConfig::ollama(None, Some("nomic-embed-text".to_string()));
        assert!(config.is_model_supported());

        let config = EmbeddingConfig::ollama(None, Some("unsupported-model".to_string()));
        assert!(!config.is_model_supported());
    }

    #[test]
    fn test_clone_safe() {
        let config = EmbeddingConfig::openai("secret-key".to_string(), None);
        let safe_clone = config.clone_safe();
        assert_eq!(safe_clone.api_key, Some("[REDACTED]".to_string()));
    }

    #[test]
    fn test_config_summary() {
        let config = EmbeddingConfig::ollama(
            Some("http://localhost:11434".to_string()),
            Some("nomic-embed-text".to_string()),
        );
        let summary = config.summary();
        assert!(summary.contains("Ollama"));
        assert!(summary.contains("nomic-embed-text"));
        assert!(summary.contains("http://localhost:11434"));
    }

    #[test]
    fn test_supported_models() {
        let ollama_models = ProviderType::Ollama.supported_models();
        assert!(ollama_models.contains(&"nomic-embed-text"));

        let openai_models = ProviderType::OpenAI.supported_models();
        assert!(openai_models.contains(&"text-embedding-3-small"));
    }
}