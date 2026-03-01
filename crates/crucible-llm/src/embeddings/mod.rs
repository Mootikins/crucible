//! Embedding provider abstraction for semantic search and vector operations
//!
//! This module provides a unified interface for generating text embeddings
//! from multiple providers (Ollama, OpenAI, etc.) with built-in resilience
//! patterns including retry logic, circuit breakers, and timeout management.

/// Configuration structures for embedding providers.
pub mod config;

/// Error types for embedding operations.
pub mod error;

/// Ollama provider implementation.
pub mod ollama;

/// OpenAI provider implementation.
pub mod openai;

/// FastEmbed local provider implementation.
#[cfg(feature = "fastembed")]
pub mod fastembed;

/// GGUF model loading for embedding models.
pub mod gguf_model;

/// Inference backend abstraction for pluggable model execution.
pub mod inference;

/// Provider trait and common functionality.
pub mod provider;

/// Mock provider for testing
pub mod mock;

pub use config::{BackendType, EmbeddingConfig, ProviderType};
pub use crucible_core::enrichment::EmbeddingProvider;
pub use error::{EmbeddingError, EmbeddingResult};
#[cfg(feature = "fastembed")]
pub use fastembed::FastEmbedProvider;
pub use mock::MockEmbeddingProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
pub use provider::EmbeddingResponse;

use std::sync::Arc;

/// Create an embedding provider from configuration
pub async fn create_provider(
    config: EmbeddingConfig,
) -> EmbeddingResult<Arc<dyn EmbeddingProvider>> {
    // Validate configuration before creating provider (From impl handles error conversion)
    config.validate()?;

    let provider_type = config.provider_type();
    match provider_type {
        BackendType::Ollama => {
            let provider = ollama::OllamaProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        BackendType::OpenAI => {
            let provider = openai::OpenAIProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        #[cfg(feature = "fastembed")]
        BackendType::FastEmbed => {
            let provider = fastembed::FastEmbedProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        #[cfg(not(feature = "fastembed"))]
        BackendType::FastEmbed => Err(EmbeddingError::ConfigError(
            "FastEmbed provider requires the 'fastembed' feature to be enabled".to_string(),
        )),
        BackendType::Burn => Err(EmbeddingError::ConfigError(
            "Burn provider is no longer included in crucible-llm".to_string(),
        )),
        BackendType::Mock => {
            let dimensions = config.dimensions().unwrap_or(768) as usize;
            let provider = mock::MockEmbeddingProvider::with_dimensions(dimensions);
            Ok(Arc::new(provider))
        }
        _ => Err(EmbeddingError::ConfigError(format!(
            "Unsupported provider type: {:?}",
            provider_type
        ))),
    }
}

/// Create a mock embedding provider for testing
#[cfg(any(test, feature = "test-utils"))]
pub fn create_mock_provider(dimensions: usize) -> Arc<dyn EmbeddingProvider> {
    Arc::new(mock::MockEmbeddingProvider::with_dimensions(dimensions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation_requires_valid_config() {
        // Test that we can create basic configs using the canonical API
        let config = EmbeddingConfig::ollama(
            Some("http://localhost:11434".to_string()),
            Some("nomic-embed-text".to_string()),
        );

        assert_eq!(config.provider_type(), BackendType::Ollama);
        assert_eq!(config.model_name(), "nomic-embed-text");
    }
}
