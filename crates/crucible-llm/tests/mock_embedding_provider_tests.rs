//! Integration tests for MockEmbeddingProvider with pre-generated deterministic fixtures
//!
//! These tests verify that MockEmbeddingProvider uses pre-generated fixture data
//! instead of algorithmic generation, ensuring consistent behavior across test runs
//! and eliminating test pollution from real embedding provider usage.

use crucible_llm::embeddings::config::EmbeddingConfig;
use crucible_llm::embeddings::EmbeddingProvider;

#[cfg(test)]
mod tests {
    use super::*;

    // Mock provider tests are only available with test-utils feature
    #[cfg(feature = "test-utils")]
    mod mock_tests {
        use super::*;
        use crucible_llm::embeddings::mock::{EmbeddingFixtures, FixtureBasedMockProvider};

        /// Test basic single text embedding with fixtures
        #[tokio::test]
        async fn test_single_text_embedding_with_fixtures() {
            let provider = FixtureBasedMockProvider::nomic();

            let text = "Hello, world!";
            let result = provider.embed(text).await.unwrap();

            let fixtures = EmbeddingFixtures::load();
            let expected_embedding = fixtures
                .get_embedding(text)
                .expect("Embedding should exist in fixtures");

            assert_eq!(result.embedding, *expected_embedding);
            assert_eq!(result.model, "nomic-embed-text-v1.5");
            assert_eq!(result.dimensions, 768);
            assert_eq!(result.tokens, Some(text.split_whitespace().count()));
        }

        /// Test configuration loading from fixtures
        #[tokio::test]
        async fn test_configuration_loading_from_fixtures() {
            let config = EmbeddingConfig::ollama(
                Some("https://mock-endpoint.com".to_string()),
                Some("nomic-embed-text-v1.5".to_string()),
            );

            assert_eq!(config.model, "nomic-embed-text-v1.5");
            assert_eq!(config.expected_dimensions(), 768);

            let provider = FixtureBasedMockProvider::new(config.model.clone());

            assert_eq!(provider.model_name(), config.model);
            assert_eq!(provider.dimensions(), config.expected_dimensions());
        }
    }

    /// Basic embedding provider test without mock functionality
    #[tokio::test]
    async fn test_basic_embedding_config() {
        let config = EmbeddingConfig::ollama(
            Some("https://mock-endpoint.com".to_string()),
            Some("nomic-embed-text-v1.5".to_string()),
        );

        assert_eq!(config.model, "nomic-embed-text-v1.5");
        assert_eq!(config.expected_dimensions(), 768);
        assert_eq!(
            config.provider,
            crucible_llm::embeddings::config::ProviderType::Ollama
        );
    }

    /// Test config validation
    #[test]
    fn test_config_validation() {
        let config = EmbeddingConfig::ollama(
            Some("https://valid-endpoint.com".to_string()),
            Some("valid-model".to_string()),
        );

        // Should not panic when creating valid config
        assert_eq!(config.model, "valid-model");
        assert_eq!(config.endpoint, "https://valid-endpoint.com");
    }
}
