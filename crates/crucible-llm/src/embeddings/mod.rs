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
#[cfg(feature = "fastembed")]
pub mod fastembed;

/// Burn ML framework provider implementation.
pub mod burn;

/// Burn model loading and inference helpers.
#[cfg(feature = "burn")]
pub mod burn_model;

/// GGUF model loading for embedding models.
pub mod gguf_model;

/// Inference backend abstraction for pluggable model execution.
pub mod inference;

/// llama.cpp backend for GGUF model inference.
#[cfg(feature = "llama-cpp")]
pub mod llama_cpp_backend;

/// Provider trait and common functionality.
pub mod provider;

/// Mock provider for testing
pub mod mock;

pub use burn::BurnProvider;
pub use config::{EmbeddingConfig, EmbeddingProviderType, ProviderType};
pub use core_adapter::CoreProviderAdapter;
pub use error::{EmbeddingError, EmbeddingResult};
#[cfg(feature = "fastembed")]
pub use fastembed::FastEmbedProvider;
pub use mock::MockEmbeddingProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAIProvider;
pub use provider::{EmbeddingProvider, EmbeddingResponse};

#[cfg(feature = "llama-cpp")]
pub use llama_cpp_backend::{AvailableDevice, LlamaCppBackend};

use std::sync::Arc;

/// Create an embedding provider from configuration
pub async fn create_provider(
    config: EmbeddingConfig,
) -> EmbeddingResult<Arc<dyn EmbeddingProvider>> {
    // Validate configuration before creating provider (From impl handles error conversion)
    config.validate()?;

    match config.provider_type() {
        EmbeddingProviderType::Ollama => {
            let provider = ollama::OllamaProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        EmbeddingProviderType::OpenAI => {
            let provider = openai::OpenAIProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        #[cfg(feature = "fastembed")]
        EmbeddingProviderType::FastEmbed => {
            let provider = fastembed::FastEmbedProvider::new(config)?;
            Ok(Arc::new(provider))
        }
        #[cfg(not(feature = "fastembed"))]
        EmbeddingProviderType::FastEmbed => Err(EmbeddingError::ConfigError(
            "FastEmbed provider requires the 'fastembed' feature to be enabled".to_string(),
        )),
        EmbeddingProviderType::Burn => {
            // Extract Burn config from the embedding config
            if let crucible_config::EmbeddingProviderConfig::Burn(burn_config) = config {
                let provider = burn::BurnProvider::new(&burn_config)?;
                Ok(Arc::new(provider))
            } else {
                Err(EmbeddingError::ConfigError(
                    "Burn provider type requires Burn configuration".to_string(),
                ))
            }
        }
        #[cfg(feature = "llama-cpp")]
        EmbeddingProviderType::LlamaCpp => {
            // Extract LlamaCpp config from the embedding config
            if let crucible_config::EmbeddingProviderConfig::LlamaCpp(llama_config) = config {
                use crate::embeddings::inference::{BackendConfig, DeviceType};
                use std::path::PathBuf;

                // Parse device type from config
                let device = match llama_config.device.to_lowercase().as_str() {
                    "cpu" => DeviceType::Cpu,
                    "vulkan" => DeviceType::Vulkan,
                    "cuda" => DeviceType::Cuda,
                    "metal" => DeviceType::Metal,
                    "rocm" => DeviceType::Rocm,
                    _ => DeviceType::Auto, // "auto" or any other value
                };

                // Build backend config from llama config
                let backend_config = BackendConfig {
                    device: device.clone(),
                    gpu_layers: llama_config.gpu_layers,
                    threads: None, // Use default
                    context_size: llama_config.context_size,
                    batch_size: llama_config.batch_size,
                    use_mmap: true,
                };

                let model_path = PathBuf::from(&llama_config.model_path);
                let provider = llama_cpp_backend::LlamaCppBackend::new_with_model_and_config(
                    model_path,
                    device,
                    backend_config,
                )?;
                Ok(Arc::new(provider))
            } else {
                Err(EmbeddingError::ConfigError(
                    "LlamaCpp provider type requires LlamaCpp configuration".to_string(),
                ))
            }
        }
        #[cfg(not(feature = "llama-cpp"))]
        EmbeddingProviderType::LlamaCpp => Err(EmbeddingError::ConfigError(
            "LlamaCpp provider requires the 'llama-cpp' feature to be enabled".to_string(),
        )),
        EmbeddingProviderType::Mock => {
            let dimensions = config.dimensions().unwrap_or(768) as usize;
            let provider = mock::MockEmbeddingProvider::with_dimensions(dimensions);
            Ok(Arc::new(provider))
        }
        _ => Err(EmbeddingError::ConfigError(format!(
            "Unsupported provider type: {:?}",
            config.provider_type()
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

        assert_eq!(config.provider_type(), EmbeddingProviderType::Ollama);
        assert_eq!(config.model_name(), "nomic-embed-text");
    }
}
