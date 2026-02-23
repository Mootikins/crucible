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
use crucible_core::storage::{NoteRecord, NoteStore};
use crucible_core::EnrichmentService;
use crucible_parser::{traits::MarkdownParser, CrucibleParser};
use std::collections::HashMap;
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
///   └─> NoteStore (Phase 4: persistence)
/// ```
///
pub struct NotePipeline {
    /// Markdown parser (Phase 2) - supports multiple backends
    parser: Arc<dyn MarkdownParser>,

    /// Storage for file state tracking (Phase 1)
    change_detector: Arc<dyn ChangeDetectionStore>,

    /// Enrichment service for embeddings and metadata (Phase 3)
    enrichment_service: Arc<dyn EnrichmentService>,

    /// Storage for notes (Phase 4) - backend-agnostic via NoteStore trait
    note_store: Arc<dyn NoteStore>,

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
        note_store: Arc<dyn NoteStore>,
    ) -> Self {
        let config = NotePipelineConfig::default();
        let parser = Self::create_parser(config.parser);

        Self {
            parser,
            change_detector,
            enrichment_service,
            note_store,
            config,
        }
    }

    /// Create a new pipeline with custom configuration
    pub fn with_config(
        change_detector: Arc<dyn ChangeDetectionStore>,
        enrichment_service: Arc<dyn EnrichmentService>,
        note_store: Arc<dyn NoteStore>,
        config: NotePipelineConfig,
    ) -> Self {
        let parser = Self::create_parser(config.parser);

        Self {
            parser,
            change_detector,
            enrichment_service,
            note_store,
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

        // Convert EnrichedNote to NoteRecord for storage
        let note_record = self.enriched_to_record(&enriched, &path_str)?;

        // Store via NoteStore trait (works with any backend)
        self.note_store
            .upsert(note_record)
            .await
            .map_err(|e| anyhow::anyhow!("Storage error: {}", e))
            .with_context(|| format!("Phase 4: Failed to store note for '{}'", path.display()))?;

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

    /// Convert an EnrichedNote to a NoteRecord for storage
    ///
    /// This bridges the enrichment domain model to the storage domain model,
    /// extracting the key fields needed for indexing and search.
    fn enriched_to_record(
        &self,
        enriched: &crucible_core::enrichment::EnrichedNote,
        relative_path: &str,
    ) -> Result<NoteRecord> {
        use crucible_core::parser::BlockHash;

        let parsed = &enriched.parsed;

        // Use content hash from parsed note (BLAKE3 hash of file content)
        let content_hash =
            BlockHash::from_hex(&parsed.content_hash).unwrap_or_else(|_| BlockHash::zero());

        // Get embedding: use first block embedding or average if multiple
        let embedding = if enriched.embeddings.is_empty() {
            None
        } else if enriched.embeddings.len() == 1 {
            Some(enriched.embeddings[0].vector.clone())
        } else {
            // Average all embeddings for document-level vector
            let dim = enriched.embeddings[0].vector.len();
            let mut avg = vec![0.0f32; dim];
            for emb in &enriched.embeddings {
                for (i, v) in emb.vector.iter().enumerate() {
                    if i < dim {
                        avg[i] += v;
                    }
                }
            }
            let count = enriched.embeddings.len() as f32;
            for v in &mut avg {
                *v /= count;
            }
            Some(avg)
        };

        // Extract links from wikilinks
        let links_to: Vec<String> = parsed.wikilinks.iter().map(|w| w.target.clone()).collect();

        // Extract tags (Tag.name is the string value)
        let tags: Vec<String> = parsed.tags.iter().map(|t| t.name.clone()).collect();

        // Extract properties from frontmatter
        let properties: HashMap<String, serde_json::Value> = parsed
            .frontmatter
            .as_ref()
            .map(|fm| fm.properties().clone())
            .unwrap_or_default();

        Ok(NoteRecord {
            path: relative_path.to_string(),
            content_hash,
            embedding,
            title: parsed.title(),
            tags,
            links_to,
            properties,
            updated_at: chrono::Utc::now(),
        })
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
    use crucible_core::enrichment::EnrichmentMetadata;
    use crucible_core::enrichment::{EnrichedNote, EnrichmentService, InferredRelation};
    use crucible_core::events::SessionEvent;
    use crucible_core::parser::{BlockHash, ParsedNote};
    use crucible_core::processing::InMemoryChangeDetectionStore;
    use crucible_core::storage::{Filter, NoteRecord, SearchResult, StorageError};
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    // -- Mock EnrichmentService --

    struct MockEnrichmentService {
        should_fail: bool,
    }

    impl MockEnrichmentService {
        fn new() -> Self {
            Self { should_fail: false }
        }

        fn failing() -> Self {
            Self { should_fail: true }
        }
    }

    #[async_trait::async_trait]
    impl EnrichmentService for MockEnrichmentService {
        async fn enrich(
            &self,
            parsed: ParsedNote,
            _changed_block_ids: Vec<String>,
        ) -> Result<EnrichedNote> {
            if self.should_fail {
                anyhow::bail!("mock enrichment failure");
            }
            Ok(EnrichedNote::new(
                parsed,
                Vec::new(),
                EnrichmentMetadata::default(),
                Vec::new(),
            ))
        }

        async fn enrich_with_tree(
            &self,
            parsed: ParsedNote,
            changed_block_ids: Vec<String>,
        ) -> Result<EnrichedNote> {
            self.enrich(parsed, changed_block_ids).await
        }

        async fn infer_relations(
            &self,
            _enriched: &EnrichedNote,
            _threshold: f64,
        ) -> Result<Vec<InferredRelation>> {
            Ok(Vec::new())
        }

        fn min_words_for_embedding(&self) -> usize {
            5
        }
        fn max_batch_size(&self) -> usize {
            10
        }
        fn has_embedding_provider(&self) -> bool {
            false
        }
    }

    // -- Mock NoteStore --

    struct MockNoteStore {
        should_fail: bool,
    }

    impl MockNoteStore {
        fn new() -> Self {
            Self { should_fail: false }
        }

        fn failing() -> Self {
            Self { should_fail: true }
        }
    }

    #[async_trait::async_trait]
    impl NoteStore for MockNoteStore {
        async fn upsert(
            &self,
            _note: NoteRecord,
        ) -> std::result::Result<Vec<SessionEvent>, StorageError> {
            if self.should_fail {
                return Err(StorageError::Backend("mock storage failure".into()));
            }
            Ok(vec![])
        }

        async fn get(&self, _path: &str) -> std::result::Result<Option<NoteRecord>, StorageError> {
            Ok(None)
        }

        async fn delete(&self, _path: &str) -> std::result::Result<SessionEvent, StorageError> {
            Ok(SessionEvent::NoteDeleted {
                path: std::path::PathBuf::new(),
                existed: false,
            })
        }

        async fn list(&self) -> std::result::Result<Vec<NoteRecord>, StorageError> {
            Ok(vec![])
        }

        async fn get_by_hash(
            &self,
            _hash: &BlockHash,
        ) -> std::result::Result<Option<NoteRecord>, StorageError> {
            Ok(None)
        }

        async fn search(
            &self,
            _embedding: &[f32],
            _k: usize,
            _filter: Option<Filter>,
        ) -> std::result::Result<Vec<SearchResult>, StorageError> {
            Ok(vec![])
        }
    }

    fn create_pipeline(
        enrichment: Arc<dyn EnrichmentService>,
        note_store: Arc<dyn NoteStore>,
    ) -> NotePipeline {
        let change_detector = Arc::new(InMemoryChangeDetectionStore::new());
        let config = NotePipelineConfig {
            skip_enrichment: false,
            force_reprocess: true,
            ..Default::default()
        };
        NotePipeline::with_config(change_detector, enrichment, note_store, config)
    }

    fn write_temp_note(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "{}", content).unwrap();
        f
    }

    #[tokio::test]
    async fn pipeline_processes_markdown_file_successfully() {
        let enrichment = Arc::new(MockEnrichmentService::new());
        let store = Arc::new(MockNoteStore::new());
        let pipeline = create_pipeline(enrichment, store);

        let tmp = write_temp_note("# Hello World\n\nSome content here.\n");
        let result = pipeline.process(tmp.path()).await.unwrap();
        assert!(
            !result.is_skipped(),
            "newly created file should not be skipped"
        );
    }

    #[tokio::test]
    async fn pipeline_skips_unchanged_file_on_second_pass() {
        let change_detector = Arc::new(InMemoryChangeDetectionStore::new());
        let enrichment: Arc<dyn EnrichmentService> = Arc::new(MockEnrichmentService::new());
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());

        // force_reprocess=false so the change detector is consulted
        let config = NotePipelineConfig {
            skip_enrichment: true,
            force_reprocess: false,
            ..Default::default()
        };
        let pipeline = NotePipeline::with_config(change_detector, enrichment, store, config);

        let tmp = write_temp_note("# Test\n\nParagraph.\n");

        // First pass should process
        let r1 = pipeline.process(tmp.path()).await.unwrap();
        assert!(!r1.is_skipped());

        // Second pass with same content should skip
        let r2 = pipeline.process(tmp.path()).await.unwrap();
        assert!(
            r2.is_skipped(),
            "unchanged file should be skipped on second pass"
        );
    }

    #[tokio::test]
    async fn pipeline_enrichment_error_propagates_with_context() {
        let enrichment = Arc::new(MockEnrichmentService::failing());
        let store = Arc::new(MockNoteStore::new());
        let pipeline = create_pipeline(enrichment, store);

        let tmp = write_temp_note("# Oops\n\nThis should fail enrichment.\n");
        let err = pipeline.process(tmp.path()).await.unwrap_err();
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("Phase 3") || msg.contains("enrich"),
            "error should mention enrichment phase, got: {msg}"
        );
    }

    #[tokio::test]
    async fn pipeline_storage_error_propagates_with_context() {
        let enrichment = Arc::new(MockEnrichmentService::new());
        let store = Arc::new(MockNoteStore::failing());
        let pipeline = create_pipeline(enrichment, store);

        let tmp = write_temp_note("# Store fail\n\nContent.\n");
        let err = pipeline.process(tmp.path()).await.unwrap_err();
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("Phase 4")
                || msg.contains("store")
                || msg.contains("storage")
                || msg.contains("Storage"),
            "error should mention storage phase, got: {msg}"
        );
    }

    #[tokio::test]
    async fn pipeline_skip_enrichment_config_bypasses_enrichment() {
        let change_detector = Arc::new(InMemoryChangeDetectionStore::new());
        let enrichment: Arc<dyn EnrichmentService> = Arc::new(MockEnrichmentService::failing());
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());

        let config = NotePipelineConfig {
            skip_enrichment: true,
            force_reprocess: true,
            ..Default::default()
        };
        let pipeline = NotePipeline::with_config(change_detector, enrichment, store, config);

        let tmp = write_temp_note("# Skip enrichment\n\nBody text.\n");
        // Should succeed even though enrichment would fail,
        // because enrichment is skipped
        let result = pipeline.process(tmp.path()).await.unwrap();
        assert!(!result.is_skipped());
    }

    #[tokio::test]
    async fn pipeline_nonexistent_file_returns_error() {
        let enrichment = Arc::new(MockEnrichmentService::new());
        let store = Arc::new(MockNoteStore::new());
        let pipeline = create_pipeline(enrichment, store);

        let result = pipeline
            .process(std::path::Path::new("/nonexistent/file.md"))
            .await;
        assert!(result.is_err(), "nonexistent file should error");
    }

    #[test]
    fn parser_backend_default_is_default() {
        let backend = ParserBackend::default();
        assert_eq!(backend, ParserBackend::Default);
    }

    #[test]
    fn pipeline_config_default_values() {
        let config = NotePipelineConfig::default();
        assert!(!config.skip_enrichment);
        assert!(!config.force_reprocess);
        assert_eq!(config.parser, ParserBackend::Default);
    }
}
