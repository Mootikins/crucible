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

/// Provider trait and common functionality.
pub mod provider;

// Make mock available for both internal tests and external integration tests
#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

pub use config::{EmbeddingConfig, ProviderType};
pub use error::{EmbeddingError, EmbeddingResult};
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
pub use provider::{EmbeddingProvider, EmbeddingResponse};

use std::sync::Arc;

/// Create an embedding provider from configuration
pub async fn create_provider(config: EmbeddingConfig) -> EmbeddingResult<Arc<dyn EmbeddingProvider>> {
    match config.provider {
        ProviderType::Ollama => {
            let provider = ollama::OllamaProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        ProviderType::OpenAI => {
            let provider = openai::OpenAIProvider::new(config)?;
            Ok(Arc::new(provider))
        }
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
        // Test that we can create basic configs
        let config = EmbeddingConfig {
            provider: ProviderType::Ollama,
            endpoint: "https://llama.terminal.krohnos.io".to_string(),
            api_key: None,
            model: "nomic-embed-text".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 10,
        };
        
        assert_eq!(config.provider, ProviderType::Ollama);
        assert_eq!(config.model, "nomic-embed-text");
    }
}
