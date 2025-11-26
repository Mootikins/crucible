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
}

// Re-export canonical EmbeddingProviderConfig as EmbeddingConfig for compatibility
pub use crucible_config::EmbeddingProviderConfig as EmbeddingConfig;

// Re-export EmbeddingProviderType for compatibility
pub use crucible_config::embedding::EmbeddingProviderType;

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
    }

    #[test]
    fn test_provider_defaults() {
        let ollama = ProviderType::Ollama;
        assert_eq!(
            ollama.default_endpoint(),
            "http://localhost:11434"
        );
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

        let config = EmbeddingConfig::fastembed(None, None, None);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_requires_api_key_for_openai() {
        // Create a config with empty API key (which should fail validation)
        use crucible_config::OpenAIConfig;
        let config = EmbeddingConfig::OpenAI(OpenAIConfig::default());

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_empty_model() {
        // Create a config with empty model name (which should fail validation)
        use crucible_config::OllamaConfig;
        let config = EmbeddingConfig::Ollama(OllamaConfig {
            model: String::new(),
            ..Default::default()
        });

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_expected_dimensions_ollama() {
        let config = EmbeddingConfig::ollama(None, Some("nomic-embed-text".to_string()));
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type(), config.model_name()),
            768
        );

        let config = EmbeddingConfig::ollama(None, Some("unknown-model".to_string()));
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type(), config.model_name()),
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
            expected_dimensions_for_model(&config.provider_type(), config.model_name()),
            1536
        );

        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("text-embedding-3-large".to_string()),
        );
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type(), config.model_name()),
            3072
        );

        let config = EmbeddingConfig::openai(
            "test-key".to_string(),
            Some("text-embedding-ada-002".to_string()),
        );
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type(), config.model_name()),
            1536
        );

        let config =
            EmbeddingConfig::openai("test-key".to_string(), Some("unknown-model".to_string()));
        assert_eq!(
            expected_dimensions_for_model(&config.provider_type(), config.model_name()),
            1536
        );
    }
}
