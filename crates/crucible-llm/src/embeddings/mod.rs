//! Embedding provider abstraction for semantic search and vector operations
//!
//! This module provides a unified interface for generating text embeddings
//! from multiple providers (Ollama, OpenAI, etc.) with built-in resilience
//! patterns including retry logic, circuit breakers, and timeout management.

/// Configuration structures for embedding providers.
pub mod config;

/// Core adapter for enrichment layer integration.
pub mod core_adapter;

/// Error types for embedding operations.
pub mod error;

/// Ollama provider implementation.
pub mod ollama;

/// OpenAI provider implementation.
pub mod openai;

/// FastEmbed local provider implementation.
pub mod fastembed;

/// Provider trait and common functionality.
pub mod provider;

/// Mock provider for testing
pub mod mock;

pub use config::{EmbeddingConfig, EmbeddingProviderType, ProviderType};
pub use core_adapter::CoreProviderAdapter;
pub use error::{EmbeddingError, EmbeddingResult};
pub use fastembed::FastEmbedProvider;
pub use mock::MockEmbeddingProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
pub use provider::{EmbeddingProvider, EmbeddingResponse};

use std::sync::Arc;

/// Create an embedding provider from configuration
pub async fn create_provider(
    config: EmbeddingConfig,
) -> EmbeddingResult<Arc<dyn EmbeddingProvider>> {
    // Validate configuration before creating provider
    config.validate()
        .map_err(|e| EmbeddingError::ConfigError(e.to_string()))?;

    match config.provider_type {
        EmbeddingProviderType::Ollama => {
            let provider = ollama::OllamaProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        EmbeddingProviderType::OpenAI => {
            let provider = openai::OpenAIProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        EmbeddingProviderType::FastEmbed => {
            let provider = fastembed::FastEmbedProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        EmbeddingProviderType::Mock => {
            let dimensions = config.model.dimensions.unwrap_or(768) as usize;
            let provider = mock::MockEmbeddingProvider::with_dimensions(dimensions);
            Ok(Arc::new(provider))
        }
        _ => Err(EmbeddingError::ConfigError(format!(
            "Unsupported provider type: {:?}",
            config.provider_type
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
            Some("https://llama.terminal.krohnos.io".to_string()),
            Some("nomic-embed-text".to_string()),
        );

        assert_eq!(config.provider_type, EmbeddingProviderType::Ollama);
        assert_eq!(config.model_name(), "nomic-embed-text");
    }
}
