//! Burn ML Framework Adapter for crucible-llm EmbeddingProvider trait
//!
//! This module provides an adapter that wraps `BurnEmbeddingProvider` from crucible-burn
//! and implements the crucible-llm `EmbeddingProvider` trait, enabling GPU-accelerated
//! embeddings to be used through the standard embedding factory.

use async_trait::async_trait;
use tracing::{debug, info};

use crate::embeddings::error::{EmbeddingError, EmbeddingResult};
use crate::embeddings::provider::{EmbeddingProvider, EmbeddingResponse, ModelInfo};

use crucible_burn::config::BurnConfig;
use crucible_burn::hardware::{BackendType, HardwareInfo};
use crucible_burn::models::{ModelInfo as BurnModelInfo, ModelRegistry, ModelType};
use crucible_burn::providers::BurnEmbeddingProvider;
use crucible_config::{BurnBackendConfig, BurnEmbedConfig};
use std::path::PathBuf;

/// Adapter that wraps BurnEmbeddingProvider for use with crucible-llm
///
/// This adapter bridges the gap between:
/// - `crucible_core::enrichment::EmbeddingProvider` (implemented by BurnEmbeddingProvider)
/// - `crucible_llm::embeddings::EmbeddingProvider` (rich trait with metadata)
pub struct BurnProviderAdapter {
    inner: BurnEmbeddingProvider,
    model_name: String,
    dimensions: usize,
    model_info: BurnModelInfo,
}

impl BurnProviderAdapter {
    /// Create a new Burn provider adapter from configuration
    pub async fn new(config: BurnEmbedConfig) -> EmbeddingResult<Self> {
        info!(
            "Creating Burn provider adapter for model: {}",
            config.model
        );

        // Get search paths and expand them
        let search_paths: Vec<PathBuf> = config
            .all_search_paths()
            .iter()
            .map(|p| PathBuf::from(shellexpand::tilde(p).to_string()))
            .collect();
        debug!("Model search paths: {:?}", search_paths);

        // Create model registry (auto-scans on creation)
        let registry = ModelRegistry::new(search_paths.clone()).await.map_err(|e| {
            EmbeddingError::ConfigError(format!("Failed to scan for models: {}", e))
        })?;
        debug!("Model registry initialized");

        // Find the requested model
        let model_info = registry
            .find_model(&config.model)
            .await
            .map_err(|e| {
                EmbeddingError::ModelNotFound(format!(
                    "Model '{}' not found in search paths {:?}: {}",
                    config.model, search_paths, e
                ))
            })?;

        // Verify it's an embedding model
        if model_info.model_type != ModelType::Embedding {
            return Err(EmbeddingError::ConfigError(format!(
                "Model '{}' is not an embedding model (type: {:?})",
                config.model, model_info.model_type
            )));
        }

        // Determine backend
        let backend = Self::resolve_backend(&config.backend).await?;
        info!("Using backend: {}", backend);

        // Create Burn config
        let burn_config = BurnConfig::default();

        // Create the underlying provider
        let inner = BurnEmbeddingProvider::new(model_info.clone(), backend, &burn_config)
            .await
            .map_err(|e| EmbeddingError::ConfigError(format!("Failed to create Burn provider: {}", e)))?;

        // Get dimensions (from config, model info, or provider)
        let dimensions = if config.dimensions > 0 {
            config.dimensions as usize
        } else {
            // Use crucible_core trait to get dimensions
            use crucible_core::enrichment::EmbeddingProvider as CoreProvider;
            inner.dimensions()
        };

        Ok(Self {
            inner,
            model_name: model_info.name.clone(),
            dimensions,
            model_info,
        })
    }

    /// Resolve backend configuration to a BackendType
    async fn resolve_backend(config: &BurnBackendConfig) -> EmbeddingResult<BackendType> {
        match config {
            BurnBackendConfig::Auto => {
                // Auto-detect best backend
                let hw_info = HardwareInfo::detect().await.map_err(|e| {
                    EmbeddingError::ConfigError(format!("Failed to detect hardware: {}", e))
                })?;
                Ok(hw_info.recommended_backend)
            }
            BurnBackendConfig::Vulkan { device_id } => Ok(BackendType::Vulkan {
                device_id: *device_id,
            }),
            BurnBackendConfig::Rocm { device_id } => Ok(BackendType::Rocm {
                device_id: *device_id,
            }),
            BurnBackendConfig::Cpu { num_threads } => Ok(BackendType::Cpu {
                num_threads: *num_threads,
            }),
        }
    }

    /// Convert Burn ModelInfo to LLM ModelInfo
    fn to_llm_model_info(burn_info: &BurnModelInfo) -> ModelInfo {
        ModelInfo::builder()
            .name(&burn_info.name)
            .dimensions(burn_info.dimensions.unwrap_or(384))
            .format(burn_info.format.to_string())
            .size_bytes(burn_info.file_size_bytes.unwrap_or(0))
            .build()
    }
}

#[async_trait]
impl EmbeddingProvider for BurnProviderAdapter {
    /// Generate an embedding for a single text input
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        use crucible_core::enrichment::EmbeddingProvider as CoreProvider;

        let embedding = self.inner.embed(text).await.map_err(|e| {
            EmbeddingError::InferenceFailed(format!("Burn embedding failed: {}", e))
        })?;

        Ok(EmbeddingResponse::new(embedding, self.model_name.clone()))
    }

    /// Generate embeddings for multiple texts in a single batch
    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        use crucible_core::enrichment::EmbeddingProvider as CoreProvider;

        // Convert Vec<String> to Vec<&str> for the core trait
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        let embeddings = self.inner.embed_batch(&text_refs).await.map_err(|e| {
            EmbeddingError::InferenceFailed(format!("Burn batch embedding failed: {}", e))
        })?;

        Ok(embeddings
            .into_iter()
            .map(|embedding| EmbeddingResponse::new(embedding, self.model_name.clone()))
            .collect())
    }

    /// Get the name of the model being used
    fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Get the dimensionality of embeddings produced by this provider
    fn dimensions(&self) -> usize {
        self.dimensions
    }

    /// Get the name of the embedding provider
    fn provider_name(&self) -> &str {
        "Burn"
    }

    /// List available embedding models
    async fn list_models(&self) -> EmbeddingResult<Vec<ModelInfo>> {
        // Return just the current model since we don't have a registry reference
        Ok(vec![Self::to_llm_model_info(&self.model_info)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_config_variants() {
        // Verify the backend config enum matches what we expect
        let auto = BurnBackendConfig::Auto;
        let vulkan = BurnBackendConfig::Vulkan { device_id: 0 };
        let rocm = BurnBackendConfig::Rocm { device_id: 0 };
        let cpu = BurnBackendConfig::Cpu { num_threads: 4 };

        // These should all be valid variants
        assert!(matches!(auto, BurnBackendConfig::Auto));
        assert!(matches!(vulkan, BurnBackendConfig::Vulkan { .. }));
        assert!(matches!(rocm, BurnBackendConfig::Rocm { .. }));
        assert!(matches!(cpu, BurnBackendConfig::Cpu { .. }));
    }
}
