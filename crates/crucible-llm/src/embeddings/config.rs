//\! Configuration for embedding providers

use serde::{Deserialize, Serialize};

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
            ProviderType::Ollama => "https://llama.terminal.krohnos.io",
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
}

// Re-export canonical EmbeddingProviderConfig as EmbeddingConfig for compatibility
pub use crucible_config::EmbeddingProviderConfig as EmbeddingConfig;

// Re-export EmbeddingProviderType for compatibility
pub use crucible_config::EmbeddingProviderType;

/// Get expected embedding dimensions based on provider and model
///
/// This is a simplified version - in production, we'd have a more
/// comprehensive model dimension mapping.
pub fn expected_dimensions_for_model(provider: &EmbeddingProviderType, model: &str) -> usize {
    match (provider, model) {
        // Ollama models
        (EmbeddingProviderType::Ollama, "nomic-embed-text") => 768,
        // OpenAI models
        (EmbeddingProviderType::OpenAI, "text-embedding-3-small") => 1536,
        (EmbeddingProviderType::OpenAI, "text-embedding-3-large") => 3072,
        (EmbeddingProviderType::OpenAI, "text-embedding-ada-002") => 1536,
        // Mock models for testing
        (EmbeddingProviderType::Mock, _) => 768,
        // Default to provider defaults for unknown models
        (EmbeddingProviderType::Ollama, _) => 768,
        (EmbeddingProviderType::OpenAI, _) => 1536,
        // Other providers default to 768
        _ => 768,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_type_from_str() {
        assert_eq!(
            ProviderType::from_str("ollama").unwrap(),
            ProviderType::Ollama
        );
        assert_eq!(
            ProviderType::from_str("Ollama").unwrap(),
            ProviderType::Ollama
        );
        assert_eq!(
            ProviderType::from_str("OLLAMA").unwrap(),
            ProviderType::Ollama
        );

        assert_eq!(
            ProviderType::from_str("openai").unwrap(),
            ProviderType::OpenAI
        );
        assert_eq!(
            ProviderType::from_str("OpenAI").unwrap(),
            ProviderType::OpenAI
        );
        assert_eq!(
            ProviderType::from_str("OPENAI").unwrap(),
            ProviderType::OpenAI
        );

        assert!(ProviderType::from_str("unknown").is_err());
        assert!(ProviderType::from_str("").is_err());

        assert_eq!(
            ProviderType::from_str("candle").unwrap(),
            ProviderType::Candle
        );
        assert_eq!(
            ProviderType::from_str("Candle").unwrap(),
            ProviderType::Candle
        );
        assert_eq!(
            ProviderType::from_str("CANDLE").unwrap(),
            ProviderType::Candle
        );
    }

    #[test]
    fn test_provider_defaults() {
        let ollama = ProviderType::Ollama;
        assert_eq!(
            ollama.default_endpoint(),
            "https://llama.terminal.krohnos.io"
        );
        assert_eq!(ollama.default_model(), "nomic-embed-text");
        assert_eq!(ollama.default_dimensions(), 768);
        assert!(!ollama.requires_api_key());

        let openai = ProviderType::OpenAI;
        assert_eq!(openai.default_endpoint(), "https://api.openai.com/v1");
        assert_eq!(openai.default_model(), "text-embedding-3-small");
        assert_eq!(openai.default_dimensions(), 1536);
        assert!(openai.requires_api_key());

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

        let config = EmbeddingConfig::candle(None, None, None, None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_requires_api_key_for_openai() {
        let mut config = EmbeddingConfig::openai("test-key".to_string(), None);
        config.api.key = None;

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_empty_model() {
        let mut config = EmbeddingConfig::ollama(None, None);
        config.model.name = String::new();

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_expected_dimensions_ollama() {
        let config = EmbeddingConfig::ollama(None, Some("nomic-embed-text".to_string()));
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type, config.model_name()),
            768
        );

        let config = EmbeddingConfig::ollama(None, Some("unknown-model".to_string()));
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type, config.model_name()),
            768
        );
    }

    #[test]
    fn test_expected_dimensions_openai() {
        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("text-embedding-3-small".to_string()),
        );
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type, config.model_name()),
            1536
        );

        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("text-embedding-3-large".to_string()),
        );
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type, config.model_name()),
            3072
        );

        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("text-embedding-ada-002".to_string()),
        );
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type, config.model_name()),
            1536
        );

        let config =
            EmbeddingConfig::openai("test-key".to_string(), Some("unknown-model".to_string()));
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type, config.model_name()),
            1536
        );
    }

    #[test]
    fn test_candle_config_creation() {
        let config = EmbeddingConfig::candle(None, None, None, None);
        assert_eq!(config.provider_type, EmbeddingProviderType::Candle);
        assert_eq!(config.endpoint(), "local");
        assert_eq!(config.model_name(), "nomic-embed-text-v1.5");
        assert!(config.api_key().is_none());
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_candle_config_with_custom_model() {
        let config = EmbeddingConfig::candle(
            Some("jina-embeddings-v2-base-en".to_string()),
            None,
            None,
            None,
        );
        assert_eq!(config.provider_type, EmbeddingProviderType::Candle);
        assert_eq!(config.model_name(), "jina-embeddings-v2-base-en");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_candle_expected_dimensions() {
        let config = EmbeddingConfig::candle(None, None, None, None);
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type, config.model_name()),
            768
        );

        let models = vec![
            ("nomic-embed-text-v1.5", 768),
            ("jina-embeddings-v2-base-en", 768),
            ("jina-embeddings-v3-base-en", 768),
            ("all-MiniLM-L6-v2", 384),
            ("bge-small-en-v1.5", 384),
        ];

        for (model, expected_dims) in models {
            let config = EmbeddingConfig::candle(Some(model.to_string()), None, None, None);
            assert_eq!(
                expected_dimensions_for_model(&config.provider_type, config.model_name()),
                expected_dims,
                "Model {} should have {} dimensions",
                model,
                expected_dims
            );
        }

        let config = EmbeddingConfig::candle(Some("unknown-model".to_string()), None, None, None);
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type, config.model_name()),
            768
        );
    }
}
