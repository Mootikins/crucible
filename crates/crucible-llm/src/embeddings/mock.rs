//! Mock embedding provider for testing

use crate::embeddings::{EmbeddingProvider, EmbeddingResponse, EmbeddingResult};
use async_trait::async_trait;
use std::collections::HashMap;

/// Mock embedding provider for testing
///
/// Returns deterministic embeddings based on text hash, useful for unit tests
/// without requiring external services.
pub struct MockEmbeddingProvider {
    dimensions: usize,
    model_name: String,
    cache: std::sync::Mutex<HashMap<String, Vec<f32>>>,
}

impl MockEmbeddingProvider {
    /// Create a new mock provider with default dimensions (768)
    pub fn new() -> Self {
        Self {
            dimensions: 768,
            model_name: "mock-test-model".to_string(),
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Create a mock provider with custom dimensions
    pub fn with_dimensions(dimensions: usize) -> Self {
        Self {
            dimensions,
            model_name: "mock-test-model".to_string(),
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Create a mock provider with custom model name
    pub fn with_model(model_name: String) -> Self {
        Self {
            dimensions: 768,
            model_name,
            cache: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Generate deterministic embedding from text
    fn generate_embedding(&self, text: &str) -> Vec<f32> {
        // Check cache first
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.get(text) {
                return cached.clone();
            }
        }

        // Generate deterministic embedding based on text hash
        let hash = self.hash_text(text);
        let mut embedding = Vec::with_capacity(self.dimensions);

        for i in 0..self.dimensions {
            let value = ((hash as f32 + i as f32).sin() * 0.5 + 0.5) * 2.0 - 1.0;
            embedding.push(value);
        }

        // Cache the result
        {
            let mut cache = self.cache.lock().unwrap();
            cache.insert(text.to_string(), embedding.clone());
        }

        embedding
    }

    /// Simple hash function for deterministic results
    fn hash_text(&self, text: &str) -> u32 {
        text.chars().fold(0u32, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u32))
    }
}

impl Default for MockEmbeddingProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        let embedding = self.generate_embedding(text);

        Ok(EmbeddingResponse {
            embedding,
            model: self.model_name.clone(),
            dimensions: self.dimensions,
            tokens: Some(text.split_whitespace().count()),
            metadata: None,
        })
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            let response = self.embed(&text).await?;
            results.push(response);
        }

        Ok(results)
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<crate::embeddings::provider::ModelInfo>> {
        use crate::embeddings::provider::{ModelFamily, ModelInfo, ParameterSize};

        // Return a hardcoded list of test models
        Ok(vec![
            ModelInfo::builder()
                .name("mock-test-model")
                .display_name("Mock Test Model")
                .dimensions(768)
                .family(ModelFamily::Bert)
                .parameter_size(ParameterSize::new(137, true)) // 137M
                .recommended(true)
                .build(),
            ModelInfo::builder()
                .name("mock-small-model")
                .display_name("Mock Small Model")
                .dimensions(384)
                .family(ModelFamily::Bert)
                .parameter_size(ParameterSize::new(50, true)) // 50M
                .build(),
            ModelInfo::builder()
                .name("mock-large-model")
                .display_name("Mock Large Model")
                .dimensions(1536)
                .family(ModelFamily::Gpt)
                .parameter_size(ParameterSize::new(1, false)) // 1B
                .build(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_basic() {
        let provider = MockEmbeddingProvider::new();
        let result = provider.embed("test text").await.unwrap();

        assert_eq!(result.embedding.len(), 768);
        assert_eq!(result.model, "mock-test-model");
    }

    #[tokio::test]
    async fn test_mock_provider_custom_dimensions() {
        let provider = MockEmbeddingProvider::with_dimensions(512);
        let result = provider.embed("test text").await.unwrap();

        assert_eq!(result.embedding.len(), 512);
    }

    #[tokio::test]
    async fn test_mock_provider_deterministic() {
        let provider = MockEmbeddingProvider::new();
        let text = "deterministic test";

        let result1 = provider.embed(text).await.unwrap();
        let result2 = provider.embed(text).await.unwrap();

        assert_eq!(result1.embedding, result2.embedding);
    }

    #[tokio::test]
    async fn test_mock_provider_different_texts() {
        let provider = MockEmbeddingProvider::new();

        let result1 = provider.embed("text1").await.unwrap();
        let result2 = provider.embed("text2").await.unwrap();

        assert_ne!(result1.embedding, result2.embedding);
    }

    #[tokio::test]
    async fn test_mock_provider_batch() {
        let provider = MockEmbeddingProvider::new();
        let texts = vec!["text1".to_string(), "text2".to_string(), "text3".to_string()];

        let results = provider.embed_batch(texts).await.unwrap();

        assert_eq!(results.len(), 3);
        for result in results {
            assert_eq!(result.embedding.len(), 768);
        }
    }
}