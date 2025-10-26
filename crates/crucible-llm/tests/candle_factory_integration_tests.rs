//! Integration tests for Candle provider factory registration
//!
//! These tests verify that the Candle provider is properly registered in the factory
//! system and can be created through the create_provider function with proper model
//! name tracking and configuration passing.

use crucible_llm::embeddings::{create_provider, EmbeddingConfig, ProviderType};

#[tokio::test]
async fn test_factory_creates_candle_provider() {
    let config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    let provider = create_provider(config).await;

    assert!(
        provider.is_ok(),
        "Factory should successfully create Candle provider"
    );

    let provider = provider.unwrap();
    assert_eq!(provider.provider_name(), "Candle");
    assert_eq!(provider.model_name(), "all-MiniLM-L6-v2");
    assert_eq!(provider.dimensions(), 384);
}

#[tokio::test]
async fn test_factory_candle_different_models() {
    let test_cases = vec![
        ("all-MiniLM-L6-v2", 384),
        ("nomic-embed-text-v1.5", 768),
        ("jina-embeddings-v2-base-en", 768),
        ("bge-small-en-v1.5", 384),
        ("unknown-model", 768), // Should use default
    ];

    for (model_name, expected_dims) in test_cases {
        let config = EmbeddingConfig::candle(None, Some(model_name.to_string()));
        let provider = create_provider(config).await;

        assert!(
            provider.is_ok(),
            "Factory should create provider for model: {}",
            model_name
        );

        let provider = provider.unwrap();
        assert_eq!(provider.model_name(), model_name);
        assert_eq!(provider.dimensions(), expected_dims);
    }
}

#[tokio::test]
async fn test_factory_candle_provider_functionality() {
    let config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    let provider = create_provider(config).await.unwrap();

    // Test single embedding
    let response = provider.embed("Hello, factory!").await;
    assert!(
        response.is_ok(),
        "Candle provider should generate embeddings"
    );

    let response = response.unwrap();
    assert_eq!(response.model, "all-MiniLM-L6-v2");
    assert_eq!(response.dimensions, 384);
    assert_eq!(response.embedding.len(), 384);

    // Test batch embedding
    let texts = vec!["Text 1".to_string(), "Text 2".to_string()];
    let responses = provider.embed_batch(texts).await;
    assert!(
        responses.is_ok(),
        "Candle provider should handle batch embedding"
    );

    let responses = responses.unwrap();
    assert_eq!(responses.len(), 2);
    for response in responses {
        assert_eq!(response.model, "all-MiniLM-L6-v2");
        assert_eq!(response.dimensions, 384);
    }
}

#[tokio::test]
async fn test_factory_candle_provider_health_check() {
    let config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    let provider = create_provider(config).await.unwrap();

    let health = provider.health_check().await;
    assert!(health.is_ok(), "Health check should complete successfully");
    assert!(health.unwrap(), "Candle provider should be healthy");
}

#[tokio::test]
async fn test_factory_candle_provider_list_models() {
    let config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    let provider = create_provider(config).await.unwrap();

    let models = provider.list_models().await;
    assert!(models.is_ok(), "Should list available models");

    let models = models.unwrap();
    assert!(!models.is_empty(), "Should have available models");

    // Check that expected models are present
    let model_names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    assert!(model_names.contains(&"all-MiniLM-L6-v2"));
    assert!(model_names.contains(&"nomic-embed-text-v1.5"));
    assert!(model_names.contains(&"jina-embeddings-v2-base-en"));
    assert!(model_names.contains(&"bge-small-en-v1.5"));
}

#[tokio::test]
async fn test_factory_candle_config_validation() {
    // Test valid configuration
    let config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    assert!(
        config.validate().is_ok(),
        "Valid Candle config should pass validation"
    );

    let provider = create_provider(config).await;
    assert!(provider.is_ok(), "Valid config should create provider");

    // Test invalid configuration (empty model name)
    let mut invalid_config = EmbeddingConfig::candle(None, None);
    invalid_config.model = String::new();
    assert!(
        invalid_config.validate().is_err(),
        "Invalid config should fail validation"
    );

    let provider = create_provider(invalid_config).await;
    assert!(
        provider.is_err(),
        "Invalid config should not create provider"
    );
}

#[tokio::test]
async fn test_factory_candle_from_env() {
    // Set environment variables for Candle provider
    std::env::set_var("EMBEDDING_PROVIDER", "candle");
    std::env::set_var("EMBEDDING_MODEL", "nomic-embed-text-v1.5");

    let config = EmbeddingConfig::from_env();
    assert!(config.is_ok(), "Should create config from environment");

    let config = config.unwrap();
    assert_eq!(config.provider, ProviderType::Candle);
    assert_eq!(config.model, "nomic-embed-text-v1.5");

    let provider = create_provider(config).await;
    assert!(provider.is_ok(), "Should create provider from env config");

    let provider = provider.unwrap();
    assert_eq!(provider.provider_name(), "Candle");
    assert_eq!(provider.model_name(), "nomic-embed-text-v1.5");
    assert_eq!(provider.dimensions(), 768);

    // Clean up environment
    std::env::remove_var("EMBEDDING_PROVIDER");
    std::env::remove_var("EMBEDDING_MODEL");
}

#[tokio::test]
async fn test_factory_candle_vs_other_providers() {
    // Test that factory correctly routes to different providers

    let candle_config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    let candle_provider = create_provider(candle_config).await.unwrap();
    assert_eq!(candle_provider.provider_name(), "Candle");

    let ollama_config = EmbeddingConfig::ollama(None, Some("nomic-embed-text".to_string()));
    let ollama_provider = create_provider(ollama_config).await.unwrap();
    assert_eq!(ollama_provider.provider_name(), "Ollama");

    // Verify they have different characteristics
    assert_ne!(
        candle_provider.provider_name(),
        ollama_provider.provider_name()
    );
    assert_eq!(candle_provider.dimensions(), 384); // all-MiniLM-L6-v2
    assert_eq!(ollama_provider.dimensions(), 768); // nomic-embed-text
}

#[tokio::test]
async fn test_factory_candle_error_handling() {
    let config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    let provider = create_provider(config).await.unwrap();

    // Test empty text error
    let result = provider.embed("").await;
    assert!(result.is_err(), "Empty text should return error");

    // Test whitespace-only text error
    let result = provider.embed("   ").await;
    assert!(result.is_err(), "Whitespace-only text should return error");

    // Test empty batch (should succeed with empty result)
    let result = provider.embed_batch(vec![]).await;
    assert!(result.is_ok(), "Empty batch should succeed");
    assert!(
        result.unwrap().is_empty(),
        "Empty batch should return empty vector"
    );
}

#[tokio::test]
async fn test_factory_candle_deterministic_embeddings() {
    let config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    let provider = create_provider(config).await.unwrap();

    let text = "Deterministic test text";

    // Generate multiple embeddings
    let embedding1 = provider.embed(text).await.unwrap();
    let embedding2 = provider.embed(text).await.unwrap();
    let embedding3 = provider.embed(text).await.unwrap();

    // All should be identical
    assert_eq!(embedding1.embedding, embedding2.embedding);
    assert_eq!(embedding2.embedding, embedding3.embedding);
    assert_eq!(embedding1.model, "all-MiniLM-L6-v2");
    assert_eq!(embedding1.dimensions, 384);
}

#[tokio::test]
async fn test_factory_candle_performance() {
    let config = EmbeddingConfig::candle(None, Some("all-MiniLM-L6-v2".to_string()));
    let provider = create_provider(config).await.unwrap();

    let text = "Performance test";
    let start_time = std::time::Instant::now();

    let response = provider.embed(text).await;
    let duration = start_time.elapsed();

    assert!(response.is_ok(), "Embedding should succeed");
    // Mock implementation should be very fast (< 10ms)
    assert!(
        duration.as_millis() < 10,
        "Mock embedding should be fast, took {}ms",
        duration.as_millis()
    );
}
