//! Configuration for embedding providers

// Re-export canonical EmbeddingProviderConfig as EmbeddingConfig for compatibility.
pub use crucible_core::config::EmbeddingProviderConfig as EmbeddingConfig;

pub use crucible_core::config::BackendType;

/// Get expected embedding dimensions based on provider and model
pub fn expected_dimensions_for_model(provider: &BackendType, model: &str) -> usize {
    match (provider, model) {
        (BackendType::Ollama, "nomic-embed-text") => 768,
        (BackendType::OpenAI, "text-embedding-3-small") => 1536,
        (BackendType::OpenAI, "text-embedding-3-large") => 3072,
        (BackendType::OpenAI, "text-embedding-ada-002") => 1536,
        (BackendType::Mock, _) => 768,
        (BackendType::Ollama, _) => 768,
        (BackendType::OpenAI, _) => 1536,
        _ => 768,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        use crucible_core::config::OpenAIConfig;
        let config = EmbeddingConfig::OpenAI(OpenAIConfig::default());

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validation_empty_model() {
        // Create a config with empty model name (which should fail validation)
        use crucible_core::config::OllamaConfig;
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
