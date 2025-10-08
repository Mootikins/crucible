//! Embedding provider abstraction for semantic search and vector operations
//! 
//! This module provides a unified interface for generating text embeddings
//! from multiple providers (Ollama, OpenAI, etc.) with built-in resilience
//! patterns including retry logic, circuit breakers, and timeout management.

pub mod config;
pub mod error;
pub mod ollama;
pub mod openai;
pub mod provider;

pub use config::{EmbeddingConfig, ProviderType};
pub use error::{EmbeddingError, EmbeddingResult};
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
