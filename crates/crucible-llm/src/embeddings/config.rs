//\! Configuration for embedding providers

use serde::{Deserialize, Serialize};
use std::env;

use super::error::{EmbeddingError, EmbeddingResult};

/// Type of embedding provider
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    /// Ollama local/remote embedding service
    Ollama,
    /// OpenAI embedding API
    OpenAI,
    /// Candle local embedding framework
    Candle,
}

impl ProviderType {
    /// Parse provider type from string
    pub fn from_str(s: &str) -> EmbeddingResult<Self> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(ProviderType::Ollama),
            "openai" => Ok(ProviderType::OpenAI),
            "candle" => Ok(ProviderType::Candle),
            _ => Err(EmbeddingError::ConfigError(format!(
                "Unknown provider type: {}. Valid options: ollama, openai, candle",
                s
            ))),
        }
    }

    /// Get default endpoint for this provider
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            ProviderType::Ollama => "https://llama.terminal.krohnos.io",
            ProviderType::OpenAI => "https://api.openai.com/v1",
            ProviderType::Candle => "local",
        }
    }

    /// Get default model for this provider
    pub fn default_model(&self) -> &'static str {
        match self {
            ProviderType::Ollama => "nomic-embed-text",
            ProviderType::OpenAI => "text-embedding-3-small",
            ProviderType::Candle => "nomic-embed-text-v1.5",
        }
    }

    /// Get expected embedding dimensions for this provider's default model
    pub fn default_dimensions(&self) -> usize {
        match self {
            ProviderType::Ollama => 768,  // nomic-embed-text
            ProviderType::OpenAI => 1536, // text-embedding-3-small
            ProviderType::Candle => 768,  // nomic-embed-text-v1.5
        }
    }

    /// Whether this provider requires an API key
    pub fn requires_api_key(&self) -> bool {
        match self {
            ProviderType::Ollama => false,
            ProviderType::OpenAI => true,
            ProviderType::Candle => false,
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
        
        let config = Self {
            provider,
            endpoint,
            api_key,
            model: model.to_string(),
            timeout_secs,
            max_retries,
            batch_size,
        };
        
        config.validate()?;
        Ok(config)
    }

    /// Create configuration for Ollama provider
    pub fn ollama(endpoint: Option<String>, model: Option<String>) -> Self {
        Self {
            provider: ProviderType::Ollama,
            endpoint: endpoint.unwrap_or_else(|| "https://llama.terminal.krohnos.io".to_string()),
            api_key: None,
            model: model.unwrap_or_else(|| "nomic-embed-text".to_string()),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 1,  // Process one file at a time to avoid Ollama batch size issues
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
        }
    }

    /// Create configuration for Candle provider
    pub fn candle(_endpoint: Option<String>, model: Option<String>) -> Self {
        Self {
            provider: ProviderType::Candle,
            endpoint: "local".to_string(),
            api_key: None,
            model: model.unwrap_or_else(|| "nomic-embed-text-v1.5".to_string()),
            timeout_secs: 120,  // Longer timeout for local processing
            max_retries: 1,     // Fewer retries for local processing
            batch_size: 1,      // Conservative batch size for local processing
        }
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
                    ProviderType::Candle => "Candle",
                }
            )));
        }

        // Validate endpoint URL
        if self.endpoint.is_empty() {
            return Err(EmbeddingError::ConfigError(
                "Endpoint URL cannot be empty".to_string()
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

        Ok(())
    }

    /// Get expected embedding dimensions based on provider and model
    pub fn expected_dimensions(&self) -> usize {
        // This is a simplified version - in production, we'd have a more
        // comprehensive model dimension mapping
        match (&self.provider, self.model.as_str()) {
            (ProviderType::Ollama, "nomic-embed-text") => 768,
            (ProviderType::OpenAI, "text-embedding-3-small") => 1536,
            (ProviderType::OpenAI, "text-embedding-3-large") => 3072,
            (ProviderType::OpenAI, "text-embedding-ada-002") => 1536,
            // Candle models
            (ProviderType::Candle, "nomic-embed-text-v1.5") => 768,
            (ProviderType::Candle, "jina-embeddings-v2-base-en") => 768,
            (ProviderType::Candle, "jina-embeddings-v3-base-en") => 768,
            (ProviderType::Candle, "all-MiniLM-L6-v2") => 384,
            (ProviderType::Candle, "bge-small-en-v1.5") => 384,
            // Default to provider defaults for unknown models
            (ProviderType::Ollama, _) => 768,
            (ProviderType::OpenAI, _) => 1536,
            (ProviderType::Candle, _) => 768,
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self::ollama(None, None)
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

        // GREEN Phase: Test for Candle provider (should now pass)
        assert_eq!(ProviderType::from_str("candle").unwrap(), ProviderType::Candle);
        assert_eq!(ProviderType::from_str("Candle").unwrap(), ProviderType::Candle);
        assert_eq!(ProviderType::from_str("CANDLE").unwrap(), ProviderType::Candle);
    }

    #[test]
    fn test_provider_defaults() {
        let ollama = ProviderType::Ollama;
        assert_eq!(ollama.default_endpoint(), "https://llama.terminal.krohnos.io");
        assert_eq!(ollama.default_model(), "nomic-embed-text");
        assert_eq!(ollama.default_dimensions(), 768);
        assert!(!ollama.requires_api_key());

        let openai = ProviderType::OpenAI;
        assert_eq!(openai.default_endpoint(), "https://api.openai.com/v1");
        assert_eq!(openai.default_model(), "text-embedding-3-small");
        assert_eq!(openai.default_dimensions(), 1536);
        assert!(openai.requires_api_key());

        // GREEN Phase: Test Candle provider defaults (should now pass)
        let candle = ProviderType::Candle;
        assert_eq!(candle.default_endpoint(), "local");
        assert_eq!(candle.default_model(), "nomic-embed-text-v1.5");
        assert_eq!(candle.default_dimensions(), 768);
        assert!(!candle.requires_api_key());
    }

    #[test]
    fn test_config_validation_success() {
        let config = EmbeddingConfig::ollama(None, None);
        assert!(config.validate().is_ok());

        let config = EmbeddingConfig::openai("test-key".to_string(), None);
        assert!(config.validate().is_ok());

        // GREEN Phase: Test Candle configuration creation (should now pass)
        let config = EmbeddingConfig::candle(None, None);
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
    fn test_config_validation_empty_endpoint() {
        let mut config = EmbeddingConfig::ollama(None, None);
        config.endpoint = String::new();

        let result = config.validate();
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, EmbeddingError::ConfigError(_)));
            assert!(e.to_string().contains("Endpoint URL"));
        }
    }

    #[test]
    fn test_config_validation_empty_model() {
        let mut config = EmbeddingConfig::ollama(None, None);
        config.model = String::new();

        let result = config.validate();
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, EmbeddingError::ConfigError(_)));
            assert!(e.to_string().contains("Model name"));
        }
    }

    #[test]
    fn test_config_validation_zero_timeout() {
        let mut config = EmbeddingConfig::ollama(None, None);
        config.timeout_secs = 0;

        let result = config.validate();
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, EmbeddingError::ConfigError(_)));
            assert!(e.to_string().contains("Timeout"));
        }
    }

    #[test]
    fn test_config_validation_zero_batch_size() {
        let mut config = EmbeddingConfig::ollama(None, None);
        config.batch_size = 0;

        let result = config.validate();
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, EmbeddingError::ConfigError(_)));
            assert!(e.to_string().contains("Batch size"));
        }
    }

    #[test]
    fn test_expected_dimensions_ollama() {
        let config = EmbeddingConfig::ollama(None, Some("nomic-embed-text".to_string()));
        assert_eq!(config.expected_dimensions(), 768);

        let config = EmbeddingConfig::ollama(None, Some("unknown-model".to_string()));
        assert_eq!(config.expected_dimensions(), 768); // Default for Ollama
    }

    #[test]
    fn test_expected_dimensions_openai() {
        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("text-embedding-3-small".to_string()),
        );
        assert_eq!(config.expected_dimensions(), 1536);

        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("text-embedding-3-large".to_string()),
        );
        assert_eq!(config.expected_dimensions(), 3072);

        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("text-embedding-ada-002".to_string()),
        );
        assert_eq!(config.expected_dimensions(), 1536);

        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("unknown-model".to_string()),
        );
        assert_eq!(config.expected_dimensions(), 1536); // Default for OpenAI
    }

    #[test]
    fn test_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.provider, ProviderType::Ollama);
        assert_eq!(config.model, "nomic-embed-text");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_from_env_missing_vars() {
        // Clear environment variables
        std::env::remove_var("EMBEDDING_PROVIDER");
        std::env::remove_var("EMBEDDING_ENDPOINT");
        std::env::remove_var("EMBEDDING_API_KEY");
        std::env::remove_var("EMBEDDING_MODEL");
        std::env::remove_var("EMBEDDING_TIMEOUT_SECS");
        std::env::remove_var("EMBEDDING_MAX_RETRIES");
        std::env::remove_var("EMBEDDING_BATCH_SIZE");

        let config = EmbeddingConfig::from_env();
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.provider, ProviderType::Ollama); // Default
        assert_eq!(config.model, "nomic-embed-text"); // Default
        assert_eq!(config.timeout_secs, 30); // Default
    }

    // RED Phase: Candle-specific tests (will fail until implementation)
    #[test]
    fn test_candle_provider_from_str() {
        assert_eq!(ProviderType::from_str("candle").unwrap(), ProviderType::Candle);
        assert_eq!(ProviderType::from_str("Candle").unwrap(), ProviderType::Candle);
        assert_eq!(ProviderType::from_str("CANDLE").unwrap(), ProviderType::Candle);
    }

    #[test]
    fn test_candle_provider_defaults() {
        let candle = ProviderType::Candle;
        assert_eq!(candle.default_endpoint(), "local");
        assert_eq!(candle.default_model(), "nomic-embed-text-v1.5");
        assert_eq!(candle.default_dimensions(), 768);
        assert!(!candle.requires_api_key());
    }

    #[test]
    fn test_candle_config_creation() {
        let config = EmbeddingConfig::candle(None, None);
        assert_eq!(config.provider, ProviderType::Candle);
        assert_eq!(config.endpoint, "local");
        assert_eq!(config.model, "nomic-embed-text-v1.5");
        assert!(config.api_key.is_none());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_candle_config_with_custom_model() {
        let config = EmbeddingConfig::candle(None, Some("jina-embeddings-v2-base-en".to_string()));
        assert_eq!(config.provider, ProviderType::Candle);
        assert_eq!(config.model, "jina-embeddings-v2-base-en");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_candle_expected_dimensions() {
        // Test default model
        let config = EmbeddingConfig::candle(None, None);
        assert_eq!(config.expected_dimensions(), 768);

        // Test different models
        let models = vec![
            ("nomic-embed-text-v1.5", 768),
            ("jina-embeddings-v2-base-en", 768),
            ("jina-embeddings-v3-base-en", 768),
            ("all-MiniLM-L6-v2", 384),
            ("bge-small-en-v1.5", 384),
        ];

        for (model, expected_dims) in models {
            let config = EmbeddingConfig::candle(None, Some(model.to_string()));
            assert_eq!(config.expected_dimensions(), expected_dims, "Model {} should have {} dimensions", model, expected_dims);
        }

        // Test unknown model defaults to 768
        let config = EmbeddingConfig::candle(None, Some("unknown-model".to_string()));
        assert_eq!(config.expected_dimensions(), 768);
    }

    #[test]
    fn test_candle_config_from_env() {
        // Set environment variables for Candle
        std::env::set_var("EMBEDDING_PROVIDER", "candle");
        std::env::set_var("EMBEDDING_MODEL", "all-MiniLM-L6-v2");

        let config = EmbeddingConfig::from_env();
        assert!(config.is_ok());

        let config = config.unwrap();
        assert_eq!(config.provider, ProviderType::Candle);
        assert_eq!(config.model, "all-MiniLM-L6-v2");

        // Clean up
        std::env::remove_var("EMBEDDING_PROVIDER");
        std::env::remove_var("EMBEDDING_MODEL");
    }
}
