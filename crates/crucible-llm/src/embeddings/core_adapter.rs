//! Adapter to bridge crucible-llm providers to core enrichment trait
//!
//! This module provides adapters that allow existing crucible-llm embedding
//! providers to work with the core enrichment layer's EmbeddingProvider trait.
//!
//! ## Architecture
//!
//! The adapter pattern enables dependency inversion:
//! - Core layer defines minimal trait (crucible_core::enrichment::EmbeddingProvider)
//! - Infrastructure layer implements concrete providers (crucible_llm)
//! - Adapter implements core trait by delegating to infrastructure providers
//!
//! ## Usage
//!
//! Create a crucible-llm provider using `create_provider()`, then wrap it with
//! `CoreProviderAdapter::new()` to use with the core enrichment layer.

use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;

use super::provider::EmbeddingProvider as LlmEmbeddingProvider;
use crucible_core::enrichment::EmbeddingProvider as CoreEmbeddingProvider;

/// Adapter that wraps a crucible-llm EmbeddingProvider to implement the core trait
///
/// This adapter allows any provider from the crucible-llm crate (FastEmbed,
/// Ollama, OpenAI, etc.) to work with the core enrichment layer.
pub struct CoreProviderAdapter {
    /// The wrapped provider from crucible-llm
    inner: Arc<dyn LlmEmbeddingProvider>,
}

impl CoreProviderAdapter {
    /// Create a new adapter wrapping a crucible-llm provider
    ///
    /// # Arguments
    ///
    /// * `provider` - Any provider implementing crucible_llm::embeddings::EmbeddingProvider
    pub fn new(provider: Arc<dyn LlmEmbeddingProvider>) -> Self {
        Self { inner: provider }
    }

    /// Get a reference to the underlying provider
    pub fn inner(&self) -> &Arc<dyn LlmEmbeddingProvider> {
        &self.inner
    }
}

#[async_trait]
impl CoreEmbeddingProvider for CoreProviderAdapter {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Call the crucible-llm provider and extract the embedding vector
        let response = self
            .inner
            .embed(text)
            .await
            .map_err(|e| anyhow::anyhow!("Embedding failed: {}", e))?;

        Ok(response.embedding)
    }

    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // Convert &[&str] to Vec<String> for crucible-llm API
        let texts_owned: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

        // Call the crucible-llm provider
        let responses = self
            .inner
            .embed_batch(texts_owned)
            .await
            .map_err(|e| anyhow::anyhow!("Batch embedding failed: {}", e))?;

        // Extract just the embedding vectors
        let embeddings = responses.into_iter().map(|r| r.embedding).collect();

        Ok(embeddings)
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn dimensions(&self) -> usize {
        self.inner.dimensions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::{create_provider, EmbeddingConfig};

    #[tokio::test]
    async fn test_adapter_single_embedding() {
        // Create a FastEmbed provider
        let config = EmbeddingConfig::fastembed(
            Some("all-MiniLM-L6-v2".to_string()),
            Some("/tmp/fastembed_cache".to_string()),
            None,
        );
        let llm_provider = create_provider(config).await.unwrap();

        // Wrap in adapter
        let adapter = CoreProviderAdapter::new(llm_provider);

        // Test as core provider
        let embedding = adapter.embed("Hello, world!").await.unwrap();
        assert_eq!(embedding.len(), 384);

        // Verify all values are finite
        for &value in &embedding {
            assert!(value.is_finite());
        }
    }

    #[tokio::test]
    async fn test_adapter_batch_embedding() {
        let config =
            EmbeddingConfig::fastembed(None, Some("/tmp/fastembed_cache".to_string()), None);
        let llm_provider = create_provider(config).await.unwrap();
        let adapter = CoreProviderAdapter::new(llm_provider);

        let texts = vec!["First text", "Second text", "Third text"];
        let embeddings = adapter.embed_batch(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 3);
        for embedding in embeddings {
            assert_eq!(embedding.len(), 384); // BGE-small default
        }
    }

    #[tokio::test]
    async fn test_adapter_metadata() {
        let config = EmbeddingConfig::fastembed(None, None, None);
        let llm_provider = create_provider(config).await.unwrap();
        let adapter = CoreProviderAdapter::new(llm_provider);

        assert_eq!(adapter.model_name(), "BAAI/bge-small-en-v1.5");
        assert_eq!(adapter.dimensions(), 384);
    }
}
