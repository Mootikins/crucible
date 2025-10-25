//! Integration tests for MockEmbeddingProvider with pre-generated deterministic fixtures
//!
//! These tests verify that MockEmbeddingProvider uses pre-generated fixture data
//! instead of algorithmic generation, ensuring consistent behavior across test runs
//! and eliminating test pollution from real embedding provider usage.

use crucible_llm::embeddings::EmbeddingProvider;
use crucible_llm::embeddings::mock::{FixtureBasedMockProvider, EmbeddingFixtures};
use crucible_llm::embeddings::config::EmbeddingConfig;
#[cfg(test)]
mod tests {
    use super::*;

    /// Test basic single text embedding with fixtures
    #[tokio::test]
    async fn test_single_text_embedding_with_fixtures() {
        let provider = FixtureBasedMockProvider::nomic();

        let text = "Hello, world!";
        let result = provider.embed(text).await.unwrap();

        let fixtures = EmbeddingFixtures::load();
        let expected_embedding = fixtures.get_embedding(text).expect("Embedding should exist in fixtures");

        assert_eq!(result.embedding, *expected_embedding);
        assert_eq!(result.model, "nomic-embed-text-v1.5");
        assert_eq!(result.dimensions, 768);
        assert_eq!(result.tokens, Some(text.split_whitespace().count()));
    }

    /// Test batch embedding generation with fixtures
    #[tokio::test]
    async fn test_batch_embedding_generation_with_fixtures() {
        let provider = FixtureBasedMockProvider::nomic();

        let texts = vec![
            "First document".to_string(),
            "Second document".to_string(),
            "Third document".to_string(),
        ];

        let results = provider.embed_batch(texts.clone()).await.unwrap();

        let fixtures = EmbeddingFixtures::load();
        let expected_responses = fixtures.batch_embeddings.get(&texts)
            .expect("Batch fixture not found");

        assert_eq!(results.len(), expected_responses.len());
        for (result, expected) in results.iter().zip(expected_responses.iter()) {
            assert_eq!(result.embedding, expected.embedding);
            assert_eq!(result.model, expected.model);
            assert_eq!(result.tokens, expected.tokens);
        }
    }

    /// Test deterministic behavior across multiple calls
    #[tokio::test]
    async fn test_deterministic_behavior_across_calls() {
        let provider = FixtureBasedMockProvider::nomic();

        let text = "This is a test document";

        // Generate embedding multiple times
        let result1 = provider.embed(text).await.unwrap();
        let result2 = provider.embed(text).await.unwrap();
        let result3 = provider.embed(text).await.unwrap();

        // All results should be identical
        assert_eq!(result1.embedding, result2.embedding);
        assert_eq!(result2.embedding, result3.embedding);
        assert_eq!(result1.model, result2.model);
        assert_eq!(result2.model, result3.model);
    }

    /// Test different models return different dimensions
    #[tokio::test]
    async fn test_different_models_different_dimensions() {
        let nomic_provider = FixtureBasedMockProvider::nomic();
        let openai_provider = FixtureBasedMockProvider::openai_small();

        let text = "Test dimensions";

        let nomic_result = nomic_provider.embed(text).await.unwrap();
        let openai_result = openai_provider.embed(text).await.unwrap();

        assert_eq!(nomic_result.dimensions, 768);
        assert_eq!(openai_result.dimensions, 1536);
        assert_ne!(nomic_result.model, openai_result.model);
    }

    /// Test model information and dimensions from fixtures
    #[tokio::test]
    async fn test_model_information_from_fixtures() {
        let provider = FixtureBasedMockProvider::nomic();

        assert_eq!(provider.model_name(), "nomic-embed-text-v1.5");
        assert_eq!(provider.dimensions(), 768);
        assert_eq!(provider.provider_name(), "mock");

        let models = provider.list_models().await.unwrap();
        assert!(!models.is_empty());

        let fixtures = EmbeddingFixtures::load();
        let _expected_model_info = fixtures.get_model_info("nomic-embed-text-v1.5");

        // Should contain the nomic model info from fixtures
        assert!(models.iter().any(|m| m.name == "nomic-embed-text-v1.5"));
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

    /// Test error handling for unknown texts
    #[tokio::test]
    async fn test_error_handling_unknown_text() {
        let provider = FixtureBasedMockProvider::nomic();

        let unknown_text = "This text is not in fixtures";

        // This should either return a default embedding or fail gracefully
        // The test documents the expected behavior
        let result = provider.embed(unknown_text).await;

        match result {
            Ok(response) => {
                // If successful, should have consistent dimensions
                assert_eq!(response.dimensions, 768);
                assert_eq!(response.model, "nomic-embed-text-v1.5");
            }
            Err(_) => {
                // Error is acceptable for unknown texts
                // This allows the implementation to be strict about fixture coverage
            }
        }
    }

    /// Test error handling for batch with unknown texts
    #[tokio::test]
    async fn test_error_handling_batch_with_unknown_texts() {
        let provider = FixtureBasedMockProvider::nomic();

        let mixed_texts = vec![
            "Hello, world!".to_string(), // Known
            "Unknown text".to_string(),  // Unknown
            "This is a test document".to_string(), // Known
        ];

        let result = provider.embed_batch(mixed_texts).await;

        match result {
            Ok(responses) => {
                assert_eq!(responses.len(), 3);
                // Known texts should return fixture data
                assert_eq!(responses[0].model, "nomic-embed-text-v1.5");
                assert_eq!(responses[2].model, "nomic-embed-text-v1.5");
            }
            Err(_) => {
                // Batch should fail if any text is unknown (strict mode)
            }
        }
    }

    /// Test health check functionality
    #[tokio::test]
    async fn test_health_check() {
        let provider = FixtureBasedMockProvider::nomic();

        let is_healthy = provider.health_check().await.unwrap();
        assert!(is_healthy, "Fixture-based provider should always be healthy");
    }

    /// Test Unicode and special character handling
    #[tokio::test]
    async fn test_unicode_and_special_characters() {
        let provider = FixtureBasedMockProvider::nomic();

        let unicode_text = "Unicode test: 🦀 Rust is awesome!";
        let result = provider.embed(unicode_text).await.unwrap();

        let fixtures = EmbeddingFixtures::load();
        let expected_embedding = fixtures.get_embedding(unicode_text).expect("Unicode embedding should exist in fixtures");

        assert_eq!(result.embedding, *expected_embedding);
        assert_eq!(result.model, "nomic-embed-text-v1.5");
        assert_eq!(result.dimensions, 768);
    }

    /// Test empty string handling
    #[tokio::test]
    async fn test_empty_string_handling() {
        let provider = FixtureBasedMockProvider::nomic();

        let empty_text = "";
        let result = provider.embed(empty_text).await.unwrap();

        let fixtures = EmbeddingFixtures::load();
        let expected_embedding = fixtures.get_embedding("Empty string").expect("Empty string embedding should exist in fixtures");

        assert_eq!(result.embedding, *expected_embedding);
        assert_eq!(result.model, "nomic-embed-text-v1.5");
        assert_eq!(result.dimensions, 768);
        assert_eq!(result.tokens, Some(0)); // Empty string has 0 tokens
    }

    /// Test large batch processing
    #[tokio::test]
    async fn test_large_batch_processing() {
        let provider = FixtureBasedMockProvider::nomic();

        // Create a batch of 50 texts
        let large_batch: Vec<String> = (0..50)
            .map(|i| format!("Test document {}", i))
            .collect();

        let start_time = std::time::Instant::now();
        let results = provider.embed_batch(large_batch).await.unwrap();
        let elapsed = start_time.elapsed();

        assert_eq!(results.len(), 50);
        assert!(elapsed.as_millis() < 1000, "Batch processing should be fast");

        // All results should have consistent model and dimensions
        for result in &results {
            assert_eq!(result.model, "nomic-embed-text-v1.5");
            assert_eq!(result.dimensions, 768);
        }
    }

    /// Test concurrent access
    #[tokio::test]
    async fn test_concurrent_access() {
        let provider = std::sync::Arc::new(FixtureBasedMockProvider::nomic());

        let mut handles = Vec::new();

        // Spawn 10 concurrent tasks
        for i in 0..10 {
            let provider_clone = provider.clone();
            let handle = tokio::spawn(async move {
                let text = format!("Concurrent test {}", i);
                provider_clone.embed(&text).await
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());

            let response = result.unwrap();
            assert_eq!(response.model, "nomic-embed-text-v1.5");
            assert_eq!(response.dimensions, 768);
        }
    }

    /// Test fixture data integrity
    #[test]
    fn test_fixture_data_integrity() {
        let fixtures = EmbeddingFixtures::load();

        // Verify we have the expected fixtures
        assert!(fixtures.embeddings.contains_key("Hello, world!"));
        assert!(fixtures.embeddings.contains_key("This is a test document"));
        assert!(fixtures.embeddings.contains_key("Search query example"));
        assert!(fixtures.embeddings.contains_key("Unicode test: 🦀 Rust is awesome!"));

        // Verify model info fixtures
        assert!(fixtures.model_info.contains_key("nomic-embed-text-v1.5"));
        assert!(fixtures.model_info.contains_key("text-embedding-3-small"));

        // Verify model dimensions
        assert_eq!(fixtures.model_dimensions.get("nomic-embed-text-v1.5"), Some(&768));
        assert_eq!(fixtures.model_dimensions.get("text-embedding-3-small"), Some(&1536));

        // Verify batch fixtures
        assert!(!fixtures.batch_embeddings.is_empty());
        for (texts, responses) in &fixtures.batch_embeddings {
            assert_eq!(texts.len(), responses.len());
        }
    }

    /// Test integration with crucible-config system
    #[tokio::test]
    async fn test_integration_with_crucible_config() {
        // This test verifies that MockEmbeddingProvider can be created
        // from crucible-config configurations

        let config = EmbeddingConfig::ollama(
            Some("https://mock.example.com".to_string()),
            Some("nomic-embed-text-v1.5".to_string()),
        );

        let model_name = config.model.clone();
        let expected_dimensions = config.expected_dimensions();
        let provider = FixtureBasedMockProvider::new(model_name.clone());

        // Provider should reflect config settings
        assert_eq!(provider.model_name(), model_name);
        assert_eq!(provider.dimensions(), expected_dimensions);

        // Should be able to generate embeddings consistent with config
        let result = provider.embed("Test integration").await.unwrap();
        assert_eq!(result.model, model_name);
        assert_eq!(result.dimensions, expected_dimensions);
    }
}