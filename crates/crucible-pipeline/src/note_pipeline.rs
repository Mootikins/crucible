//! Note Processing Pipeline Orchestrator
//!
//! This module implements the pipeline for processing notes in Crucible.
//!
//! ## Pipeline Architecture
//!
//! 1. **Quick Filter**: Check file state (date modified + BLAKE3 hash) to skip unchanged files
//! 2. **Parse**: Transform markdown to AST using crucible-parser
//! 3. **Enrich**: Generate embeddings and metadata using crucible-enrichment
//! 4. **Store**: Persist all changes using storage layer
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
use crucible_parser::{traits::MarkdownParser, CrucibleParser};
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, info};

/// Parser backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParserBackend {
    /// Use CrucibleParser (default, regex-based extraction)
    #[default]
    Default,
    /// Use markdown-it-rust based parser (AST-based, requires feature flag)
    #[cfg(feature = "markdown-it-parser")]
    MarkdownIt,
}

/// Configuration for pipeline behavior
#[derive(Debug, Clone, Default)]
pub struct NotePipelineConfig {
    /// Which markdown parser to use
    pub parser: ParserBackend,
    /// Skip enrichment phase (useful for testing or when embeddings not needed)
    pub skip_enrichment: bool,
    /// Force full reprocessing even if file hash matches
    pub force_reprocess: bool,
}

/// The main pipeline orchestrator
///
/// Coordinates all phases of note processing. This is the single
/// entry point for all note processing operations across all frontends
/// (CLI, Desktop, MCP, Obsidian plugin, etc.).
///
/// # Architecture
///
/// ```text
/// NotePipeline (orchestration)
///   ├─> ChangeDetectionStore (Phase 1: skip checks)
///   ├─> crucible-parser (Phase 2: AST)
///   ├─> EnrichmentService (Phase 3: embeddings)
///   └─> Storage (Phase 4: persistence)
/// ```
///
pub struct NotePipeline {
    /// Markdown parser (Phase 2) - supports multiple backends
    parser: Arc<dyn MarkdownParser>,

    /// Storage for file state tracking (Phase 1)
    change_detector: Arc<dyn ChangeDetectionStore>,

    /// Enrichment service for embeddings and metadata (Phase 3)
    enrichment_service: Arc<dyn EnrichmentService>,

    /// Storage for enriched notes (Phase 4)
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
        enrichment_service: Arc<dyn EnrichmentService>,
        storage: Arc<dyn EnrichedNoteStore>,
    ) -> Self {
        let config = NotePipelineConfig::default();
        let parser = Self::create_parser(config.parser);

        Self {
            parser,
            change_detector,
            enrichment_service,
            storage,
            config,
        }
    }

    /// Create a new pipeline with custom configuration
    pub fn with_config(
        change_detector: Arc<dyn ChangeDetectionStore>,
        enrichment_service: Arc<dyn EnrichmentService>,
        storage: Arc<dyn EnrichedNoteStore>,
        config: NotePipelineConfig,
    ) -> Self {
        let parser = Self::create_parser(config.parser);

        Self {
            parser,
            change_detector,
            enrichment_service,
            storage,
            config,
        }
    }

    /// Process a note through all phases
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
    /// 3. **Enrich**: Generate embeddings for all blocks
    /// 4. **Store**: Persist changes to database
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

        let path_str = path.to_string_lossy();

        // Phase 3: Enrichment (if enabled)
        let phase3_start = std::time::Instant::now();
        let enriched = if !self.config.skip_enrichment {
            debug!("Phase 3: Enriching note");

            // Enrich all blocks (empty changed_blocks means embed all)
            self.enrichment_service
                .enrich(parsed.clone(), Vec::new())
                .await
                .with_context(|| format!("Phase 3: Failed to enrich note '{}'", path.display()))?
        } else {
            debug!("Phase 3: Enrichment skipped (disabled in config)");

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
        let phase3_duration = phase3_start.elapsed().as_millis() as u64;
        debug!(
            "Phase 3: Generated {} embeddings, {} relations",
            enriched.embeddings.len(),
            enriched.inferred_relations.len()
        );

        // Phase 4: Storage
        let phase4_start = std::time::Instant::now();

        // Store enriched note (includes parsed content, embeddings, metadata)
        self.storage
            .store_enriched(&enriched, &path_str)
            .await
            .with_context(|| {
                format!(
                    "Phase 4: Failed to store enriched note for '{}'",
                    path.display()
                )
            })?;

        // Update file state tracking
        self.update_file_state(path).await.with_context(|| {
            format!(
                "Phase 4: Failed to update file state for '{}'",
                path.display()
            )
        })?;

        let phase4_duration = phase4_start.elapsed().as_millis() as u64;

        let total_duration = start.elapsed().as_millis() as u64;

        info!(
            "Completed processing in {}ms (P1:{}, P2:{}, P3:{}, P4:{})",
            total_duration, phase1_duration, phase2_duration, phase3_duration, phase4_duration
        );

        // Count of blocks enriched (embeddings generated)
        let blocks_enriched = enriched.embeddings.len();

        Ok(ProcessingResult::success(
            blocks_enriched,
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
    use crucible_core::processing::InMemoryChangeDetectionStore;
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    // TODO: Add tests once we have mock EnrichmentService

    #[tokio::test]
    async fn test_pipeline_creation() {
        let _change_detector = Arc::new(InMemoryChangeDetectionStore::new());

        // For now, we can't test without a mock EnrichmentService
        // This will be added once we wire up the full implementation
    }

    /// Test that Phase 2 parse errors include file path in error message
    #[tokio::test]
    async fn test_parse_error_includes_file_path() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "# Test").unwrap();
        let _file_path = temp_file.path();

        // TODO: Implement with mock parser that returns errors
    }

    /// Test that Phase 3 enrichment errors include file path and phase info
    #[tokio::test]
    async fn test_enrichment_error_includes_context() {
        // TODO: Implement with mock enrichment service that returns errors
    }

    /// Test that Phase 4 storage errors include file path and what failed
    #[tokio::test]
    async fn test_storage_error_includes_operation_context() {
        // TODO: Implement with mock storage that returns errors
    }
}
