#[cfg(test)]
mod tests {
    use crucible_llm::embeddings::create_provider;
    use crucible_config::{EmbeddingProviderConfig, BurnEmbedConfig, BurnBackendConfig};

    #[tokio::test]
    async fn test_create_burn_provider() {
        // Create a Burn configuration
        let config = EmbeddingProviderConfig::Burn(BurnEmbedConfig {
            model: "test-model".to_string(),
            backend: BurnBackendConfig::Cpu { num_threads: 4 },
            dimensions: 384,
            ..Default::default()
        });

        // Create provider through factory
        let provider = create_provider(config).await.expect("Failed to create Burn provider");

        // Verify provider properties
        assert_eq!(provider.provider_name(), "Burn");
        assert_eq!(provider.model_name(), "test-model");
        assert_eq!(provider.dimensions(), 384);

        // Test embedding generation
        let text = "Hello, world!";
        let response = provider.embed(text).await.expect("Failed to generate embedding");

        assert_eq!(response.embedding.len(), 384);
        assert_eq!(response.model, "test-model");
        assert_eq!(response.dimensions, 384);
    }

    #[tokio::test]
    async fn test_burn_provider_batch_embeddings() {
        let config = EmbeddingProviderConfig::Burn(BurnEmbedConfig {
            model: "test-model".to_string(),
            backend: BurnBackendConfig::Cpu { num_threads: 4 },
            dimensions: 768,
            ..Default::default()
        });

        let provider = create_provider(config).await.expect("Failed to create Burn provider");

        // Test batch embedding
        let texts = vec!["First text".to_string(), "Second text".to_string(), "Third text".to_string()];
        let responses = provider.embed_batch(texts).await.expect("Failed to generate batch embeddings");

        assert_eq!(responses.len(), 3);
        for response in responses {
            assert_eq!(response.embedding.len(), 768);
            assert_eq!(response.model, "test-model");
        }
    }

    #[tokio::test]
    async fn test_burn_provider_gpu_backend() {
        // Test with Vulkan backend (will fall back to mock in tests)
        let config = EmbeddingProviderConfig::Burn(BurnEmbedConfig {
            model: "test-model".to_string(),
            backend: BurnBackendConfig::Vulkan { device_id: 0 },
            dimensions: 512,
            ..Default::default()
        });

        let provider = create_provider(config).await.expect("Failed to create Burn provider");
        assert_eq!(provider.provider_name(), "Burn");
        assert_eq!(provider.dimensions(), 512);

        // Even with Vulkan backend, should work (mocked)
        let response = provider.embed("Test").await.expect("Failed to generate embedding");
        assert_eq!(response.embedding.len(), 512);
    }

    #[tokio::test]
    async fn test_burn_provider_list_models() {
        let config = EmbeddingProviderConfig::Burn(BurnEmbedConfig {
            model: "test-model".to_string(),
            backend: BurnBackendConfig::Auto,
            dimensions: 768,
            ..Default::default()
        });

        let provider = create_provider(config).await.expect("Failed to create Burn provider");

        let models = provider.list_models().await.expect("Failed to list models");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "test-model");
        assert_eq!(models[0].dimensions, Some(768));
    }
}