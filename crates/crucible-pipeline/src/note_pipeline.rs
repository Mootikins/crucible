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
use crucible_core::processing::{
    ChangeDetectionStore, FileState, NotePipelineOrchestrator, PipelineMetrics, ProcessingResult,
};
use crucible_core::{EnrichedNoteStore, EnrichmentService};
use crucible_merkle::{HybridMerkleTree, MerkleStore};
use crucible_parser::{traits::MarkdownParser, CrucibleParser};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, info};

/// Parser backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserBackend {
    /// Use CrucibleParser (default, regex-based extraction)
    Default,
    /// Use markdown-it-rust based parser (AST-based, requires feature flag)
    #[cfg(feature = "markdown-it-parser")]
    MarkdownIt,
}

impl Default for ParserBackend {
    fn default() -> Self {
        Self::Default
    }
}

/// Configuration for pipeline behavior
#[derive(Debug, Clone)]
pub struct NotePipelineConfig {
    /// Which markdown parser to use
    pub parser: ParserBackend,
    /// Skip enrichment phase (useful for testing or when embeddings not needed)
    pub skip_enrichment: bool,
    /// Force full reprocessing even if file hash matches
    pub force_reprocess: bool,
}

impl Default for NotePipelineConfig {
    fn default() -> Self {
        Self {
            parser: ParserBackend::default(),
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
pub struct NotePipeline {
    /// Markdown parser (Phase 2) - supports multiple backends
    parser: Arc<dyn MarkdownParser>,

    /// Storage for file state tracking (Phase 1)
    change_detector: Arc<dyn ChangeDetectionStore>,

    /// Storage for Merkle trees (Phase 3)
    merkle_store: Arc<dyn MerkleStore>,

    /// Enrichment service for embeddings and metadata (Phase 4)
    enrichment_service: Arc<dyn EnrichmentService>,

    /// Storage for enriched notes (Phase 5)
    storage: Arc<dyn EnrichedNoteStore>,

    /// Configuration
    config: NotePipelineConfig,
}

impl NotePipeline {
    /// Create a parser instance based on the configured backend
    fn create_parser(backend: ParserBackend) -> Arc<dyn MarkdownParser> {
        match backend {
            ParserBackend::Default => Arc::new(CrucibleParser::new()),
            #[cfg(feature = "markdown-it-parser")]
            ParserBackend::MarkdownIt => {
                use crucible_parser::MarkdownItParser;
                Arc::new(MarkdownItParser::new())
            }
        }
    }

    /// Create a new pipeline with dependencies (uses default config)
    pub fn new(
        change_detector: Arc<dyn ChangeDetectionStore>,
        merkle_store: Arc<dyn MerkleStore>,
        enrichment_service: Arc<dyn EnrichmentService>,
        storage: Arc<dyn EnrichedNoteStore>,
    ) -> Self {
        let config = NotePipelineConfig::default();
        let parser = Self::create_parser(config.parser);

        Self {
            parser,
            change_detector,
            merkle_store,
            enrichment_service,
            storage,
            config,
        }
    }

    /// Create a new pipeline with custom configuration
    pub fn with_config(
        change_detector: Arc<dyn ChangeDetectionStore>,
        merkle_store: Arc<dyn MerkleStore>,
        enrichment_service: Arc<dyn EnrichmentService>,
        storage: Arc<dyn EnrichedNoteStore>,
        config: NotePipelineConfig,
    ) -> Self {
        let parser = Self::create_parser(config.parser);

        Self {
            parser,
            change_detector,
            merkle_store,
            enrichment_service,
            storage,
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
        let parsed = self.parser.parse_file(path).await.with_context(|| {
            format!(
                "Phase 2: Failed to parse markdown file '{}'",
                path.display()
            )
        })?;
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

        // If no changes and not forcing reprocess, update file state and return
        if diff.changed_sections.is_empty() && old_tree.is_some() && !self.config.force_reprocess {
            debug!("Phase 3: No content changes detected");
            self.update_file_state(path).await.with_context(|| {
                format!(
                    "Failed to update file state for '{}' after detecting no changes",
                    path.display()
                )
            })?;
            self.merkle_store
                .store(&path_str, &new_tree)
                .await
                .with_context(|| {
                    format!(
                        "Failed to update Merkle tree for '{}' after detecting no changes",
                        path.display()
                    )
                })?;
            return Ok(ProcessingResult::NoChanges);
        }

        // Extract changed block IDs from diff
        // Note: The diff provides section-level changes. For now, we treat entire sections
        // as changed blocks. Future enhancement: block-level granularity.

        // Count all types of changes: modified, added, and removed sections
        let changed_count =
            diff.changed_sections.len() + diff.added_sections + diff.removed_sections;

        // Build IDs for all affected sections
        let mut changed_block_ids = Vec::with_capacity(changed_count);

        // Add modified sections
        for (_idx, section) in diff.changed_sections.iter().enumerate() {
            changed_block_ids.push(format!("modified_section_{}", section.section_index));
        }

        // Add newly added sections
        for idx in 0..diff.added_sections {
            changed_block_ids.push(format!("added_section_{}", idx));
        }

        // Add removed sections (for tracking purposes)
        for idx in 0..diff.removed_sections {
            changed_block_ids.push(format!("removed_section_{}", idx));
        }

        debug!(
            "Phase 3: {} sections changed (modified: {}, added: {}, removed: {})",
            changed_count,
            diff.changed_sections.len(),
            diff.added_sections,
            diff.removed_sections
        );

        // Phase 4: Enrichment (if enabled)
        let phase4_start = std::time::Instant::now();
        let enriched = if !self.config.skip_enrichment {
            debug!(
                "Phase 4: Enriching note with {} changed blocks",
                changed_count
            );

            // Call enrichment service with Merkle tree (avoids recomputation)
            self.enrichment_service
                .enrich_with_tree(parsed.clone(), changed_block_ids.clone())
                .await
                .with_context(|| {
                    format!(
                        "Phase 4: Failed to enrich note '{}' (processing {} changed blocks)",
                        path.display(),
                        changed_count
                    )
                })?
        } else {
            debug!("Phase 4: Enrichment skipped (disabled in config)");

            // Create minimal enriched note without embeddings
            use crucible_core::enrichment::{EnrichedNote, EnrichmentMetadata};
            EnrichedNote::new(
                parsed.clone(),
                Vec::new(), // No embeddings
                EnrichmentMetadata::default(),
                Vec::new(), // No inferred relations
            )
        };

        let embeddings_generated = !enriched.embeddings.is_empty();
        let phase4_duration = phase4_start.elapsed().as_millis() as u64;
        debug!(
            "Phase 4: Generated {} embeddings, {} relations",
            enriched.embeddings.len(),
            enriched.inferred_relations.len()
        );

        // Phase 5: Storage
        let phase5_start = std::time::Instant::now();

        // Store enriched note (includes parsed content, Merkle tree, embeddings, metadata)
        self.storage
            .store_enriched(&enriched, &path_str)
            .await
            .with_context(|| {
                format!(
                    "Phase 5: Failed to store enriched note for '{}'",
                    path.display()
                )
            })?;

        // Store Merkle tree separately for future diffs
        self.merkle_store
            .store(&path_str, &new_tree)
            .await
            .with_context(|| {
                format!(
                    "Phase 5: Failed to store Merkle tree for '{}'",
                    path.display()
                )
            })?;

        // Update file state tracking
        self.update_file_state(path).await.with_context(|| {
            format!(
                "Phase 5: Failed to update file state for '{}'",
                path.display()
            )
        })?;

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

        Ok(ProcessingResult::success(
            changed_count,
            embeddings_generated,
        ))
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
                && stored.file_size == current_state.file_size
            {
                debug!(
                    "File unchanged (hash: {}, size: {})",
                    &current_state.file_hash[..8],
                    current_state.file_size
                );
                return Ok(Some(ProcessingResult::skipped()));
            }
        }

        Ok(None)
    }

    /// Compute current file state (hash, modified time, size)
    async fn compute_file_state(&self, path: &Path) -> Result<FileState> {
        let metadata = tokio::fs::metadata(path)
            .await
            .context("Failed to read file metadata")?;

        let content = tokio::fs::read(path)
            .await
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
        self.change_detector
            .store_file_state(path, state)
            .await
            .context("Failed to store file state")?;
        Ok(())
    }
}

// Implement the NotePipelineOrchestrator trait
#[async_trait::async_trait]
impl NotePipelineOrchestrator for NotePipeline {
    async fn process(&self, path: &Path) -> Result<ProcessingResult> {
        // Delegate to the existing process implementation
        NotePipeline::process(self, path).await
    }

    async fn process_with_metrics(
        &self,
        path: &Path,
    ) -> Result<(ProcessingResult, PipelineMetrics)> {
        // TODO: Collect detailed metrics during processing
        // For now, just call process and return empty metrics
        let result = self.process(path).await?;
        Ok((result, PipelineMetrics::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::processing::InMemoryChangeDetectionStore;
    use crucible_merkle::InMemoryMerkleStore;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    // TODO: Add tests once we have mock EnrichmentService

    #[tokio::test]
    async fn test_pipeline_creation() {
        let _change_detector = Arc::new(InMemoryChangeDetectionStore::new());
        let _merkle_store = Arc::new(InMemoryMerkleStore::new());

        // For now, we can't test without a mock EnrichmentService
        // This will be added once we wire up the full implementation
    }

    /// Bug #3 RED: Test that Phase 2 parse errors include file path in error message
    ///
    /// When parsing fails, the error should clearly identify:
    /// - The file that failed
    /// - The phase that failed (Phase 2: Parse)
    /// - The underlying error details
    #[tokio::test]
    async fn test_parse_error_includes_file_path() {
        // This test will fail initially because current error messages lack file context

        // Create a file with invalid markdown that will cause parse failure
        let mut temp_file = NamedTempFile::new().unwrap();
        // Write some content that might cause issues (actual failure will depend on parser)
        writeln!(temp_file, "# Test").unwrap();
        let file_path = temp_file.path();

        // When we implement this test properly with mocks, we'll verify:
        // - Error contains file path: assert!(error_msg.contains(file_path.display()))
        // - Error contains phase info: assert!(error_msg.contains("Phase 2"))
        // - Error contains specific details: assert!(error_msg.contains("Failed to parse"))

        // For now, this is a placeholder that documents what we need to test
        // TODO: Implement with mock parser that returns errors
    }

    /// Bug #3 RED: Test that Phase 4 enrichment errors include file path and phase info
    ///
    /// When enrichment fails, the error should clearly identify:
    /// - The file being enriched
    /// - The phase that failed (Phase 4: Enrich)
    /// - How many blocks were being enriched
    /// - The underlying error from the enrichment service
    #[tokio::test]
    async fn test_enrichment_error_includes_context() {
        // This test will fail initially because current error at line 268 lacks detail

        // When we implement this test properly with mocks, we'll verify error message contains:
        // - File path: "Failed to enrich /path/to/note.md"
        // - Phase info: "Phase 4: Enrich"
        // - Block count: "while processing 5 changed blocks"
        // - Underlying error: actual error from enrichment service

        // TODO: Implement with mock enrichment service that returns errors
    }

    /// Bug #3 RED: Test that Phase 5 storage errors include file path and what failed
    ///
    /// When storage fails, the error should clearly identify:
    /// - The file being stored
    /// - The phase that failed (Phase 5: Store)
    /// - Which storage operation failed (enriched note vs Merkle tree vs file state)
    /// - The underlying storage error
    #[tokio::test]
    async fn test_storage_error_includes_operation_context() {
        // This test will fail initially because errors at lines 297, 301, 305 lack detail

        // When we implement this test properly with mocks, we'll verify:
        // Storage of enriched note:
        //   - "Phase 5: Failed to store enriched note for /path/to/note.md"
        // Merkle tree storage:
        //   - "Phase 5: Failed to store Merkle tree for /path/to/note.md"
        // File state update:
        //   - "Phase 5: Failed to update file state for /path/to/note.md"

        // Each error should include the underlying storage error details

        // TODO: Implement with mock storage that returns errors
    }
}
