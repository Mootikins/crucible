//! Pipeline factory - assembles NotePipeline with all dependencies
//!
//! This factory module is responsible for creating a fully configured
//! NotePipeline by wiring together all necessary dependencies. This is
//! the composition root for the pipeline orchestration layer.

use crate::config::CliConfig;
use anyhow::Result;
use crucible_core::processing::InMemoryChangeDetectionStore;
use crucible_pipeline::{NotePipeline, NotePipelineConfig, ParserBackend};
use std::sync::Arc;

/// Create NotePipeline with all dependencies wired together
///
/// This factory assembles a complete NotePipeline by creating and connecting:
/// 1. Change detection (SurrealDB-backed for persistence)
/// 2. Merkle tree storage (SurrealDB-backed)
/// 3. Enrichment service (with optional embeddings)
/// 4. Enriched note storage (SurrealDB-backed)
/// 5. Pipeline configuration
///
/// All dependencies are created as trait objects (`Arc<dyn Trait>`), following
/// the Dependency Inversion Principle. This allows easy swapping of implementations
/// without changing the pipeline code.
///
/// # Arguments
///
/// * `storage_client` - SurrealDB client for database operations
/// * `config` - CLI configuration containing paths and settings
/// * `force` - Whether to force reprocessing of all files
///
/// # Returns
///
/// A fully configured `NotePipeline` ready to process notes
///
/// # Example
///
/// ```no_run
/// use crucible_cli::factories;
/// use crucible_cli::config::CliConfig;
///
/// # async fn example() -> anyhow::Result<()> {
/// # let config = CliConfig::default();
/// let storage_client = factories::create_surrealdb_storage(&config).await?;
/// let pipeline = factories::create_pipeline(
///     storage_client,
///     &config,
///     false  // don't force reprocess
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_pipeline(
    storage_client: crucible_surrealdb::adapters::SurrealClientHandle,
    config: &CliConfig,
    force: bool,
) -> Result<NotePipeline> {
    // 1. Change detection (in-memory for now)
    // NOTE: Phase 4 cleanup - SurrealDB change detection store was removed.
    // Using in-memory store until NoteStore-based change detection is implemented.
    let change_detector = Arc::new(InMemoryChangeDetectionStore::new());

    // 2. Merkle store (SurrealDB-backed)
    let merkle_store = super::create_surrealdb_merkle_store(storage_client.clone());

    // 3. Enrichment service (with optional embeddings)
    let enrichment_service = super::create_default_enrichment_service(config).await?;

    // 4. Enriched note store (SurrealDB-backed)
    let note_store = super::create_surrealdb_enriched_note_store(storage_client);

    // 5. Pipeline configuration
    let pipeline_config = NotePipelineConfig {
        parser: ParserBackend::default(),
        skip_enrichment: false,
        force_reprocess: force,
    };

    // 6. Assemble pipeline (all trait objects)
    Ok(NotePipeline::with_config(
        change_detector,
        merkle_store,
        enrichment_service,
        note_store,
        pipeline_config,
    ))
}
