use anyhow::Result;
use std::sync::Arc;
use tracing::{debug, info};

use crate::config::BurnConfig;
use crate::hardware::BackendType;
use crate::models::ModelInfo;

pub mod embed;
pub mod base;

// Re-export provider types
pub use embed::BurnEmbeddingProvider;

/// Create a Burn embedding provider
pub async fn create_embedding_provider(
    model_info: ModelInfo,
    backend: BackendType,
    config: &BurnConfig,
) -> Result<Arc<dyn crucible_core::enrichment::EmbeddingProvider>> {
    info!("Creating Burn embedding provider for model: {}", model_info.name);
    debug!("Backend: {:?}", backend);

    let provider = embed::BurnEmbeddingProvider::new(model_info, backend, config).await?;
    Ok(Arc::new(provider))
}