//! Enrichment service factory - creates DefaultEnrichmentService
//! Phase 5: Uses public factory function instead of importing concrete service.
//! Includes caching for embedding providers to avoid repeated initialization.

use crate::config::CliConfig;
use anyhow::Result;
use crucible_core::enrichment::{EmbeddingProvider, EnrichmentService};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::trace;

/// Cache for embedding providers keyed by configuration hash
/// This avoids recreating embedding providers (which can be expensive for
/// FastEmbed model loading or remote API connections)
static EMBEDDING_PROVIDER_CACHE: Lazy<Mutex<HashMap<String, Arc<dyn EmbeddingProvider>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Generate a cache key from embedding configuration
fn embedding_config_cache_key(config: &CliConfig) -> String {
    let ec = &config.embedding;
    format!(
        "{:?}|{}|{}|{}",
        ec.provider,
        ec.model.as_deref().unwrap_or("default"),
        ec.api_url.as_deref().unwrap_or("default"),
        ec.batch_size
    )
}

/// Get or create an embedding provider (cached)
///
/// This function caches embedding providers to avoid expensive repeated
/// initialization. FastEmbed requires loading model weights, and remote
/// providers may need connection setup.
pub async fn get_or_create_embedding_provider(
    config: &CliConfig,
) -> Result<Arc<dyn EmbeddingProvider>> {
    let cache_key = embedding_config_cache_key(config);

    // Check cache first
    {
        let cache = EMBEDDING_PROVIDER_CACHE.lock().unwrap();
        if let Some(cached) = cache.get(&cache_key) {
            trace!("Using cached embedding provider for key: {}", cache_key);
            return Ok(cached.clone());
        }
    }

    // Create new provider
    trace!("Creating new embedding provider for key: {}", cache_key);
    let embedding_config = config.embedding.to_provider_config();
    let llm_provider = crucible_llm::embeddings::create_provider(embedding_config).await?;
    let core_provider = crucible_llm::embeddings::CoreProviderAdapter::new(llm_provider);
    let provider: Arc<dyn EmbeddingProvider> = Arc::new(core_provider);

    // Cache it
    {
        let mut cache = EMBEDDING_PROVIDER_CACHE.lock().unwrap();
        cache.insert(cache_key, provider.clone());
    }

    Ok(provider)
}

/// Create DefaultEnrichmentService with embedding provider
///
/// Phase 5: Uses public factory function from crucible-enrichment instead of
/// constructing DefaultEnrichmentService directly.
/// Uses cached embedding provider for faster repeated calls.
pub async fn create_default_enrichment_service(
    config: &CliConfig,
) -> Result<Arc<dyn EnrichmentService>> {
    // Use cached embedding provider
    let embedding_provider = get_or_create_embedding_provider(config).await?;

    // Use public factory function from crucible-enrichment
    crucible_enrichment::create_default_enrichment_service(Some(embedding_provider))
}

/// Clear the embedding provider cache (useful for testing)
#[allow(dead_code)]
pub fn clear_embedding_provider_cache() {
    EMBEDDING_PROVIDER_CACHE.lock().unwrap().clear();
}
