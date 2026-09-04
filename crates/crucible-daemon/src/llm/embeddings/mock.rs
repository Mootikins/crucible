//! Fixture embedding provider: deterministic vectors, no network.
//!
//! `crate::test_support::MockEmbeddingProvider` is the configurable test
//! double. This one backs `BackendType::Mock` at runtime.

use async_trait::async_trait;
use crucible_core::enrichment::EmbeddingProvider;
use std::collections::HashMap;

/// Fixture embedding provider.
///
/// Returns deterministic embeddings based on text hash, useful for unit tests
/// without requiring external services.
pub struct FixtureEmbeddingProvider {
    dimensions: usize,
    model_name: String,
    cache: std::sync::Mutex<HashMap<String, Vec<f32>>>,
}

impl FixtureEmbeddingProvider {
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
        text.chars()
            .fold(0u32, |acc, c| acc.wrapping_mul(31).wrapping_add(c as u32))
    }
}

impl Default for FixtureEmbeddingProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmbeddingProvider for FixtureEmbeddingProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.generate_embedding(text))
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.generate_embedding(t)).collect())
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn provider_kind(&self) -> &'static str {
        "mock"
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![
            "mock-test-model".to_string(),
            "mock-small-model".to_string(),
            "mock-large-model".to_string(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_basic() {
        let provider = FixtureEmbeddingProvider::new();
        let embedding = provider.embed("test text").await.unwrap();

        assert_eq!(embedding.len(), 768);
    }

    #[tokio::test]
    async fn test_mock_provider_custom_dimensions() {
        let provider = FixtureEmbeddingProvider::with_dimensions(512);
        let embedding = provider.embed("test text").await.unwrap();

        assert_eq!(embedding.len(), 512);
    }

    #[tokio::test]
    async fn test_mock_provider_deterministic() {
        let provider = FixtureEmbeddingProvider::new();
        let text = "deterministic test";

        let result1 = provider.embed(text).await.unwrap();
        let result2 = provider.embed(text).await.unwrap();

        assert_eq!(result1, result2);
    }

    #[tokio::test]
    async fn test_mock_provider_different_texts() {
        let provider = FixtureEmbeddingProvider::new();

        let result1 = provider.embed("text1").await.unwrap();
        let result2 = provider.embed("text2").await.unwrap();

        assert_ne!(result1, result2);
    }

    #[tokio::test]
    async fn test_mock_provider_batch() {
        let provider = FixtureEmbeddingProvider::new();
        let texts: Vec<&str> = vec!["text1", "text2", "text3"];

        let results = provider.embed_batch(&texts).await.unwrap();

        assert_eq!(results.len(), 3);
        for result in results {
            assert_eq!(result.len(), 768);
        }
    }
}
