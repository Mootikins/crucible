//! Note Processing Pipeline Orchestrator
//!
//! This module implements the 5-phase pipeline for processing notes in Crucible.
//!
//! ## Five-Phase Architecture
//!
//! 1. **Quick Filter**: Check file state (date modified + BLAKE3 hash) to skip unchanged files
//! 2. **Parse**: Transform markdown to AST using crucible-parser
//! 3. **Merkle Diff**: Build Merkle tree and compare with stored version to identify changed blocks
//! 4. **Enrich**: Generate embeddings and metadata for changed blocks using crucible-enrichment
//! 5. **Store**: Persist all changes using storage layer
//!
//! ## Design Principles
//!
//! - **Orchestration Only**: This crate coordinates, it doesn't implement business logic
//! - **Dependency Injection**: All services injected via constructor (testable, flexible)
//! - **Clear Boundaries**: Each phase has explicit input/output types
//! - **Error Recovery**: Graceful handling of failures at each phase
//! - **Single Responsibility**: Pipeline coordinates; infrastructure crates provide capabilities

use anyhow::{Context, Result};
use crucible_core::processing::{ChangeDetectionStore, FileState};
use crucible_core::EnrichmentService;
use crucible_merkle::{HybridMerkleTree, MerkleStore};
use crucible_parser::{CrucibleParser, traits::MarkdownParserImplementation};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, info, warn};

/// Result of processing a note through the pipeline
#[derive(Debug, Clone)]
pub enum ProcessingResult {
    /// Note was processed successfully with changes
    Success {
        /// Number of blocks that were changed
        changed_blocks: usize,
        /// Whether embeddings were generated
        embeddings_generated: bool,
    },
    /// Note was skipped (unchanged file hash)
    Skipped,
    /// Note had no content changes (same Merkle tree)
    NoChanges,
}

/// Metrics collected during pipeline execution
#[derive(Debug, Clone, Default)]
pub struct PipelineMetrics {
    /// Time spent in Phase 1 (quick filter)
    pub phase1_duration_ms: u64,
    /// Time spent in Phase 2 (parse)
    pub phase2_duration_ms: u64,
    /// Time spent in Phase 3 (Merkle diff)
    pub phase3_duration_ms: u64,
    /// Time spent in Phase 4 (enrichment)
    pub phase4_duration_ms: u64,
    /// Time spent in Phase 5 (storage)
    pub phase5_duration_ms: u64,
    /// Total pipeline execution time
    pub total_duration_ms: u64,
}

/// Configuration for pipeline behavior
#[derive(Debug, Clone)]
pub struct NotePipelineConfig {
    /// Skip enrichment phase (useful for testing or when embeddings not needed)
    pub skip_enrichment: bool,
    /// Force full reprocessing even if file hash matches
    pub force_reprocess: bool,
}

impl Default for NotePipelineConfig {
    fn default() -> Self {
        Self {
            skip_enrichment: false,
            force_reprocess: false,
        }
    }
}

/// The main pipeline orchestrator
///
/// Coordinates all five phases of note processing. This is the single
/// entry point for all note processing operations across all frontends
/// (CLI, Desktop, MCP, Obsidian plugin, etc.).
///
/// # Architecture
///
/// ```text
/// NotePipeline (orchestration)
///   ├─> ChangeDetectionStore (Phase 1: skip checks)
///   ├─> crucible-parser (Phase 2: AST)
///   ├─> MerkleStore (Phase 3: diff)
///   ├─> EnrichmentService (Phase 4: embeddings)
///   └─> Storage (Phase 5: persistence)
/// ```
///
/// # Example
///
/// ```rust,ignore
/// use crucible_pipeline::{NotePipeline, NotePipelineConfig};
///
/// // Create pipeline with all dependencies injected
/// let pipeline = NotePipeline::builder()
///     .change_detector(change_store)
///     .merkle_store(merkle_store)
///     .enrichment_service(enrichment_service)
///     .storage(storage)
///     .build();
///
/// // Process a note
/// let result = pipeline.process(path).await?;
/// match result {
///     ProcessingResult::Success { changed_blocks, .. } => {
///         println!("Processed {} changed blocks", changed_blocks);
///     }
///     ProcessingResult::Skipped => {
///         println!("Note unchanged, skipped processing");
///     }
///     ProcessingResult::NoChanges => {
///         println!("No content changes detected");
///     }
/// }
/// ```
pub struct NotePipeline {
    /// Markdown parser (Phase 2)
    parser: CrucibleParser,

    /// Storage for file state tracking (Phase 1)
    change_detector: Arc<dyn ChangeDetectionStore>,

    /// Storage for Merkle trees (Phase 3)
    merkle_store: Arc<dyn MerkleStore>,

    /// Enrichment service for embeddings and metadata (Phase 4)
    enrichment_service: Arc<dyn EnrichmentService>,

    /// Configuration
    config: NotePipelineConfig,
}

impl NotePipeline {
    /// Create a new pipeline with dependencies
    pub fn new(
        change_detector: Arc<dyn ChangeDetectionStore>,
        merkle_store: Arc<dyn MerkleStore>,
        enrichment_service: Arc<dyn EnrichmentService>,
    ) -> Self {
        Self {
            parser: CrucibleParser::new(),
            change_detector,
            merkle_store,
            enrichment_service,
            config: NotePipelineConfig::default(),
        }
    }

    /// Create a new pipeline with custom configuration
    pub fn with_config(
        change_detector: Arc<dyn ChangeDetectionStore>,
        merkle_store: Arc<dyn MerkleStore>,
        enrichment_service: Arc<dyn EnrichmentService>,
        config: NotePipelineConfig,
    ) -> Self {
        Self {
            parser: CrucibleParser::new(),
            change_detector,
            merkle_store,
            enrichment_service,
            config,
        }
    }

    /// Process a note through all five phases
    ///
    /// This is the main entry point for note processing. It coordinates
    /// all phases and handles errors gracefully.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the markdown file to process
    ///
    /// # Returns
    ///
    /// - `Ok(ProcessingResult)` on success or skip
    /// - `Err(...)` only for unrecoverable errors
    ///
    /// # Phases
    ///
    /// 1. **Quick Filter**: Check if file hash changed
    /// 2. **Parse**: Convert markdown to AST
    /// 3. **Merkle Diff**: Identify changed blocks
    /// 4. **Enrich**: Generate embeddings for changed blocks
    /// 5. **Store**: Persist changes to database
    pub async fn process(&self, path: &Path) -> Result<ProcessingResult> {
        let start = std::time::Instant::now();

        info!("Processing note: {}", path.display());

        // Phase 1: Quick Filter (skip check)
        let phase1_start = std::time::Instant::now();
        if let Some(skip_result) = self.phase1_quick_filter(path).await? {
            debug!("Phase 1: File unchanged, skipping");
            return Ok(skip_result);
        }
        let phase1_duration = phase1_start.elapsed().as_millis() as u64;

        // Phase 2: Parse to AST
        let phase2_start = std::time::Instant::now();
        let parsed = self.parser.parse_file(path).await
            .context("Phase 2: Failed to parse markdown file")?;
        let phase2_duration = phase2_start.elapsed().as_millis() as u64;
        debug!("Phase 2: Parsed note successfully");

        // Phase 3: Build Merkle tree and diff
        let phase3_start = std::time::Instant::now();
        let new_tree = HybridMerkleTree::from_document(&parsed);

        // Get old tree from storage
        let path_str = path.to_string_lossy();
        let old_tree = self.merkle_store.retrieve(&path_str).await.ok();

        // Compute diff
        let diff = if let Some(ref old) = old_tree {
            new_tree.diff(old)
        } else {
            // No old tree means all blocks are "changed"
            new_tree.diff(&HybridMerkleTree::default())
        };

        let phase3_duration = phase3_start.elapsed().as_millis() as u64;

        // If no changes, update file state and return
        if diff.changed_sections.is_empty() && old_tree.is_some() {
            debug!("Phase 3: No content changes detected");
            self.update_file_state(path).await?;
            self.merkle_store.store(&path_str, &new_tree).await
                .context("Failed to update Merkle tree")?;
            return Ok(ProcessingResult::NoChanges);
        }

        // Count changed sections as a proxy for changed blocks
        // TODO: Enhance diff to provide block-level granularity
        let changed_section_count = diff.changed_sections.len();
        debug!("Phase 3: {} sections changed", changed_section_count);

        // Phase 4: Enrichment (if enabled)
        let phase4_start = std::time::Instant::now();
        let embeddings_generated = if !self.config.skip_enrichment {
            // TODO: Call enrichment service with parsed note and changed sections
            // For now, this is a placeholder until we integrate with EnrichmentService
            warn!("Phase 4: Enrichment not yet implemented in pipeline");
            false
        } else {
            debug!("Phase 4: Enrichment skipped (disabled in config)");
            false
        };
        let phase4_duration = phase4_start.elapsed().as_millis() as u64;

        // Phase 5: Storage
        let phase5_start = std::time::Instant::now();

        // Store Merkle tree
        self.merkle_store.store(&path_str, &new_tree).await
            .context("Phase 5: Failed to store Merkle tree")?;

        // Update file state
        self.update_file_state(path).await
            .context("Phase 5: Failed to update file state")?;

        // TODO: Store enriched note data
        // This will use NoteIngestor::ingest_enriched() once we wire it up

        let phase5_duration = phase5_start.elapsed().as_millis() as u64;

        let total_duration = start.elapsed().as_millis() as u64;

        info!(
            "Completed processing in {}ms (P1:{}, P2:{}, P3:{}, P4:{}, P5:{})",
            total_duration,
            phase1_duration,
            phase2_duration,
            phase3_duration,
            phase4_duration,
            phase5_duration
        );

        Ok(ProcessingResult::Success {
            changed_blocks: changed_section_count,
            embeddings_generated,
        })
    }

    /// Phase 1: Quick filter check
    ///
    /// Checks if the file has changed since last processing by comparing
    /// file hash and modification time. Returns `Some(ProcessingResult::Skipped)`
    /// if the file is unchanged, or `None` if processing should continue.
    async fn phase1_quick_filter(&self, path: &Path) -> Result<Option<ProcessingResult>> {
        if self.config.force_reprocess {
            debug!("Force reprocess enabled, skipping quick filter");
            return Ok(None);
        }

        // Get stored file state
        let stored_state = self.change_detector.get_file_state(path).await?;

        // Compute current file state
        let current_state = self.compute_file_state(path).await?;

        // Compare states
        if let Some(stored) = stored_state {
            if stored.file_hash == current_state.file_hash
                && stored.file_size == current_state.file_size {
                debug!(
                    "File unchanged (hash: {}, size: {})",
                    &current_state.file_hash[..8],
                    current_state.file_size
                );
                return Ok(Some(ProcessingResult::Skipped));
            }
        }

        Ok(None)
    }

    /// Compute current file state (hash, modified time, size)
    async fn compute_file_state(&self, path: &Path) -> Result<FileState> {
        let metadata = tokio::fs::metadata(path).await
            .context("Failed to read file metadata")?;

        let content = tokio::fs::read(path).await
            .context("Failed to read file content")?;

        let hash = blake3::hash(&content);

        Ok(FileState {
            file_hash: hash.to_hex().to_string(),
            modified_time: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            file_size: metadata.len(),
        })
    }

    /// Update stored file state after successful processing
    async fn update_file_state(&self, path: &Path) -> Result<()> {
        let state = self.compute_file_state(path).await?;
        self.change_detector.store_file_state(path, state).await
            .context("Failed to store file state")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::processing::InMemoryChangeDetectionStore;
    use crucible_merkle::InMemoryMerkleStore;
    use std::sync::Arc;
    use tempfile::NamedTempFile;
    use std::io::Write;

    // TODO: Add tests once we have mock EnrichmentService

    #[tokio::test]
    async fn test_pipeline_creation() {
        let change_detector = Arc::new(InMemoryChangeDetectionStore::new());
        let merkle_store = Arc::new(InMemoryMerkleStore::new());

        // For now, we can't test without a mock EnrichmentService
        // This will be added once we wire up the full implementation
    }
}
