//! Enrichment service factory - creates DefaultEnrichmentService
//! Phase 5: Uses public factory function instead of importing concrete service.

use crate::config::CliConfig;
use anyhow::Result;
use crucible_core::enrichment::EnrichmentService;
use std::sync::Arc;

/// Create DefaultEnrichmentService with embedding provider
///
/// Phase 5: Uses public factory function from crucible-enrichment instead of
/// constructing DefaultEnrichmentService directly.
pub async fn create_default_enrichment_service(
    config: &CliConfig,
) -> Result<Arc<dyn EnrichmentService>> {
    // Create embedding provider from composite config
    let embedding_provider = {
        let embedding_config = config.embedding.to_provider_config();
        // Create llm provider using factory function
        let llm_provider = crucible_llm::embeddings::create_provider(embedding_config).await?;
        // Wrap in adapter to implement core trait
        let core_provider = crucible_llm::embeddings::CoreProviderAdapter::new(llm_provider);
        Some(Arc::new(core_provider) as Arc<dyn crucible_core::enrichment::EmbeddingProvider>)
    };

    // Use public factory function from crucible-enrichment
    crucible_enrichment::create_default_enrichment_service(embedding_provider)
}
