//! Note Processing Pipeline Orchestrator
//!
//! This module implements the pipeline for processing notes in Crucible.
//!
//! ## Pipeline Architecture
//!
//! 1. **Quick Filter**: Check file state (date modified + BLAKE3 hash) to skip unchanged files
//! 2. **Parse**: Transform markdown to AST using crucible-parser
//! 3. **Enrich**: Generate embeddings and metadata using the enrichment module
//! 4. **Store**: Persist all changes using storage layer
//!
//! ## Design Principles
//!
//! - **Orchestration Only**: This crate coordinates, it doesn't implement business logic
//! - **Dependency Injection**: All services injected via constructor (testable, flexible)
//! - **Clear Boundaries**: Each phase has explicit input/output types
//! - **Error Recovery**: Graceful handling of failures at each phase
//! - **Single Responsibility**: Pipeline coordinates; infrastructure crates provide capabilities

use crate::enrichment::Enricher;
use anyhow::{Context, Result};
use crucible_core::parser::{traits::MarkdownParser, CrucibleParser};
use crucible_core::processing::{ChangeDetectionStore, FileState, ProcessingResult};
use crucible_core::storage::{NoteRecord, NoteStore, VectorStore};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, info};

/// Configuration for pipeline behavior
#[derive(Debug, Clone, Default)]
pub struct NotePipelineConfig {
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
///   ├─> Enricher (Phase 3: embeddings)
///   └─> NoteStore (Phase 4: persistence)
/// ```
///
pub struct NotePipeline {
    /// Markdown parser (Phase 2) - supports multiple backends
    parser: Arc<dyn MarkdownParser>,

    /// Storage for file state tracking (Phase 1)
    change_detector: Arc<dyn ChangeDetectionStore>,

    /// Enricher for embeddings and metadata (Phase 3)
    enricher: Arc<Enricher>,

    /// Storage for notes (Phase 4) - backend-agnostic via NoteStore trait
    note_store: Arc<dyn NoteStore>,

    /// Vector index for embeddings (Phase 4 — Lance-backed). When set,
    /// the pipeline writes the per-note embedding here keyed by note path.
    /// `None` in tests that don't exercise vector search.
    vector_store: Option<Arc<dyn VectorStore>>,

    /// Configuration
    config: NotePipelineConfig,

    /// Kiln root path for normalizing stored paths to kiln-relative form
    kiln_root: Option<std::path::PathBuf>,
}

impl NotePipeline {
    /// Create a new pipeline with dependencies (uses default config)
    pub fn new(
        change_detector: Arc<dyn ChangeDetectionStore>,
        enricher: Arc<Enricher>,
        note_store: Arc<dyn NoteStore>,
    ) -> Self {
        let config = NotePipelineConfig::default();
        let parser = Arc::new(CrucibleParser::new()) as Arc<dyn MarkdownParser>;

        Self {
            parser,
            change_detector,
            enricher,
            note_store,
            vector_store: None,
            config,
            kiln_root: None,
        }
    }

    /// Create a new pipeline with custom configuration
    pub fn with_config(
        change_detector: Arc<dyn ChangeDetectionStore>,
        enricher: Arc<Enricher>,
        note_store: Arc<dyn NoteStore>,
        config: NotePipelineConfig,
    ) -> Self {
        let parser = Arc::new(CrucibleParser::new()) as Arc<dyn MarkdownParser>;

        Self {
            parser,
            change_detector,
            enricher,
            note_store,
            vector_store: None,
            config,
            kiln_root: None,
        }
    }

    /// Attach a Lance-backed vector index. The pipeline writes each note's
    /// embedding to this index after the SQLite metadata upsert succeeds.
    pub fn with_vector_store(mut self, vectors: Arc<dyn VectorStore>) -> Self {
        self.vector_store = Some(vectors);
        self
    }

    /// Set the kiln root path for normalizing stored note paths.
    pub fn set_kiln_root(&mut self, root: std::path::PathBuf) {
        self.kiln_root = Some(root);
    }

    /// Set whether to force reprocessing of all files, bypassing change detection.
    pub fn set_force_reprocess(&mut self, force: bool) {
        self.config.force_reprocess = force;
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
        let warnings = parsed
            .parse_errors
            .iter()
            .map(|error| {
                format!(
                    "{:?} [{}:{}] (offset {}): {}",
                    error.error_type, error.line, error.column, error.offset, error.message
                )
            })
            .collect::<Vec<_>>();
        debug!("Phase 2: Parsed note successfully");

        let path_str = if let Some(ref kiln_root) = self.kiln_root {
            std::borrow::Cow::Owned(
                crate::kiln_manager::normalize_note_path(path, kiln_root)
                    .unwrap_or_else(|| path.to_string_lossy().to_string()),
            )
        } else {
            path.to_string_lossy()
        };

        // Phase 3: Enrichment (if enabled)
        let phase3_start = std::time::Instant::now();
        let enriched = if !self.config.skip_enrichment {
            debug!("Phase 3: Enriching note");

            // Enrich all blocks (empty changed_blocks means embed all)
            self.enricher
                .enrich(parsed.clone(), Vec::new())
                .await
                .with_context(|| format!("Phase 3: Failed to enrich note '{}'", path.display()))?
        } else {
            debug!("Phase 3: Enrichment skipped (disabled in config)");

            // Create minimal enriched note without embeddings
            use crucible_core::enrichment::{EnrichedNote, EnrichmentMetadata};
            EnrichedNote::new(parsed.clone(), Vec::new(), EnrichmentMetadata::default())
        };

        let embeddings_generated = !enriched.embeddings.is_empty();
        let phase3_duration = phase3_start.elapsed().as_millis() as u64;
        debug!(
            "Phase 3: Generated {} embeddings",
            enriched.embeddings.len()
        );

        // Phase 4: Storage
        let phase4_start = std::time::Instant::now();

        // Convert EnrichedNote to NoteRecord for storage
        let note_record = self.enriched_to_record(&enriched, &path_str)?;

        // Capture the embedding before move so we can also write it to the
        // Lance vector index. Cheap clone — this is the only copy made.
        let embedding_for_vectors = note_record.embedding.clone();

        // Store via NoteStore trait (works with any backend)
        self.note_store
            .upsert(note_record)
            .await
            .map_err(|e| anyhow::anyhow!("Storage error: {}", e))
            .with_context(|| format!("Phase 4: Failed to store note for '{}'", path.display()))?;

        // Mirror the embedding into the Lance vector index keyed by note
        // path. Lance is the source of truth for similarity search; the
        // SQLite copy persists for now but is no longer queried.
        if let (Some(vectors), Some(embedding)) =
            (self.vector_store.as_ref(), embedding_for_vectors)
        {
            if let Err(e) = vectors.upsert(&path_str, embedding).await {
                // Surfaced at error level: a persistent upsert failure (e.g. an
                // embedding/index dimension mismatch) silently voids semantic
                // search for this note, so it must not hide in warn noise.
                tracing::error!(
                    path = %path_str,
                    ?e,
                    "vector index upsert failed; metadata persisted but search will miss this note"
                );
            }
        }

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

        Ok(ProcessingResult::success_with_warnings(
            blocks_enriched,
            embeddings_generated,
            warnings,
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
        storage_path: &str,
    ) -> Result<NoteRecord> {
        use crucible_core::parser::BlockHash;

        let parsed = &enriched.parsed;

        // Use content hash from parsed note (BLAKE3 hash of file content)
        let content_hash =
            BlockHash::from_hex(&parsed.content_hash).unwrap_or_else(|_| BlockHash::zero());

        // Get embedding: use first block embedding or average if multiple
        let (embedding, embedding_model, embedding_dimensions) = if enriched.embeddings.is_empty() {
            (None, None, None)
        } else if enriched.embeddings.len() == 1 {
            let emb = &enriched.embeddings[0];
            (
                Some(emb.vector.clone()),
                Some(emb.model.clone()),
                Some(emb.dimensions as u32),
            )
        } else {
            // Average all embeddings for document-level vector
            let first = &enriched.embeddings[0];
            let dim = first.vector.len();
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
            (
                Some(avg),
                Some(first.model.clone()),
                Some(first.dimensions as u32),
            )
        };

        // Extract links from wikilinks
        let links_to: Vec<String> = parsed.wikilinks.iter().map(|w| w.target.clone()).collect();

        // Extract tags (Tag.name is the string value)
        let tags: Vec<String> = parsed.tags.iter().map(|t| t.name.clone()).collect();

        // Extract properties from frontmatter
        let mut properties: HashMap<String, serde_json::Value> = parsed
            .frontmatter
            .as_ref()
            .map(|fm| fm.properties().clone())
            .unwrap_or_default();

        // Stamp `properties.scope` so the storage-layer scope filter has a
        // value to match against. Precedence:
        //   1. Explicit frontmatter `scope:` (already in `properties`)
        //   2. Derived default = `Workspace { path: kiln_root }`
        //
        // For (1), we ALSO normalize an unbound `workspace` placeholder
        // (frontmatter said `scope: workspace` with no path) by binding it
        // to the kiln root. We never mutate the markdown file on disk —
        // this stamping only affects the in-memory NoteRecord that becomes
        // the SQLite row.
        stamp_scope_on_properties(&mut properties, self.kiln_root.as_deref());

        Ok(NoteRecord {
            path: storage_path.to_string(),
            content_hash,
            embedding,
            embedding_model,
            embedding_dimensions,
            title: parsed.title(),
            tags,
            links_to,
            properties,
            updated_at: chrono::Utc::now(),
        })
    }
}

/// Ensure `properties["scope"]` is present and bound. See
/// [`NotePipeline::enriched_to_record`] for precedence rules.
///
/// - If `properties["scope"]` is missing or unparseable → stamp `workspace`
///   derived from `kiln_root` (canonicalized if possible).
/// - If `properties["scope"]` is an unbound workspace placeholder (the
///   frontmatter said `scope: workspace` with no path) → bind it to
///   `kiln_root`.
/// - Otherwise leave the property as-is.
///
/// If `kiln_root` is `None` (rare, test-only) the property is stamped with
/// an unbound workspace placeholder — the post-prune storage layer treats
/// an unbound scope as invisible to every authority, which fails closed.
///
/// Public-in-crate so tests can exercise the migration logic directly
/// without spinning up a full pipeline.
pub(crate) fn stamp_scope_on_properties(
    properties: &mut HashMap<String, serde_json::Value>,
    kiln_root: Option<&std::path::Path>,
) {
    use crucible_core::storage::note_store::SCOPE_PROPERTY_KEY;
    use crucible_core::storage::Scope;

    // Pre-prune `from_property_value` returned `Option<Scope>`. It now
    // returns `Option<Result<Scope, ScopeError>>` because legacy `global`
    // and `user:*` kinds are refused. Treat any error as "missing" so a
    // stale frontmatter `scope: global` flips back to the kiln workspace.
    let existing = properties
        .get(SCOPE_PROPERTY_KEY)
        .and_then(|v| Scope::from_property_value(v).and_then(Result::ok));

    let bound = match existing {
        Some(scope) if scope.is_unbound_workspace() => match kiln_root {
            Some(root) => scope.bind_to_workspace(root),
            None => scope,
        },
        Some(scope) => scope,
        None => match kiln_root {
            Some(root) => {
                Scope::workspace(root).unwrap_or_else(|_| Scope::workspace_unchecked(root))
            }
            None => Scope::workspace_unchecked(std::path::PathBuf::new()),
        },
    };

    properties.insert(SCOPE_PROPERTY_KEY.to_string(), bound.to_property_value());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::enrichment::EmbeddingProvider;
    use crucible_core::events::{InternalSessionEvent, SessionEvent};
    use crucible_core::parser::BlockHash;
    use crucible_core::processing::InMemoryChangeDetectionStore;
    use crucible_core::storage::{Filter, NoteRecord, SearchResult, StorageError};
    use std::io::Write;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    /// Embedding provider whose batch call always fails; used to make the
    /// enricher propagate errors through Phase 3.
    struct FailingEmbeddingProvider;

    #[async_trait::async_trait]
    impl EmbeddingProvider for FailingEmbeddingProvider {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            anyhow::bail!("intentional embed failure")
        }

        async fn embed_batch(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>> {
            anyhow::bail!("intentional embed failure")
        }

        fn model_name(&self) -> &str {
            "failing-mock"
        }

        fn dimensions(&self) -> usize {
            3
        }

        fn provider_name(&self) -> &str {
            "failing-mock"
        }

        async fn list_models(&self) -> Result<Vec<String>> {
            Ok(vec!["failing-mock".to_string()])
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

        async fn get(
            &self,
            _path: &str,
            _authority: &crucible_core::storage::Scope,
        ) -> std::result::Result<Option<NoteRecord>, StorageError> {
            Ok(None)
        }

        async fn delete(&self, _path: &str) -> std::result::Result<SessionEvent, StorageError> {
            Ok(SessionEvent::internal(InternalSessionEvent::NoteDeleted {
                path: std::path::PathBuf::new(),
                existed: false,
            }))
        }

        async fn list(
            &self,
            _authority: &crucible_core::storage::Scope,
        ) -> std::result::Result<Vec<NoteRecord>, StorageError> {
            Ok(vec![])
        }

        async fn get_by_hash(
            &self,
            _hash: &BlockHash,
            _authority: &crucible_core::storage::Scope,
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

    fn create_pipeline(enricher: Arc<Enricher>, note_store: Arc<dyn NoteStore>) -> NotePipeline {
        let change_detector = Arc::new(InMemoryChangeDetectionStore::new());
        let config = NotePipelineConfig {
            skip_enrichment: false,
            force_reprocess: true,
        };
        NotePipeline::with_config(change_detector, enricher, note_store, config)
    }

    fn passing_enricher() -> Arc<Enricher> {
        Arc::new(Enricher::without_embeddings())
    }

    fn failing_enricher() -> Arc<Enricher> {
        Arc::new(Enricher::new(Arc::new(FailingEmbeddingProvider)))
    }

    fn write_temp_note(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "{}", content).unwrap();
        f
    }

    #[tokio::test]
    async fn pipeline_processes_markdown_file_successfully() {
        let store = Arc::new(MockNoteStore::new());
        let pipeline = create_pipeline(passing_enricher(), store);

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
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());

        // force_reprocess=false so the change detector is consulted
        let config = NotePipelineConfig {
            skip_enrichment: true,
            force_reprocess: false,
        };
        let pipeline =
            NotePipeline::with_config(change_detector, passing_enricher(), store, config);

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
        let store = Arc::new(MockNoteStore::new());
        let pipeline = create_pipeline(failing_enricher(), store);

        // Paragraph is long enough to trigger an embed call, which then fails.
        let tmp = write_temp_note(
            "# Oops\n\nThis paragraph has more than five words so embed_batch runs.\n",
        );
        let err = pipeline.process(tmp.path()).await.unwrap_err();
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("Phase 3") || msg.contains("enrich"),
            "error should mention enrichment phase, got: {msg}"
        );
    }

    #[tokio::test]
    async fn pipeline_storage_error_propagates_with_context() {
        let store = Arc::new(MockNoteStore::failing());
        let pipeline = create_pipeline(passing_enricher(), store);

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
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());

        let config = NotePipelineConfig {
            skip_enrichment: true,
            force_reprocess: true,
        };
        // Enrichment is configured to fail, but skip_enrichment=true means it
        // never runs — the pipeline should still succeed.
        let pipeline =
            NotePipeline::with_config(change_detector, failing_enricher(), store, config);

        let tmp = write_temp_note("# Skip enrichment\n\nBody text.\n");
        let result = pipeline.process(tmp.path()).await.unwrap();
        assert!(!result.is_skipped());
    }

    #[tokio::test]
    async fn pipeline_nonexistent_file_returns_error() {
        let store = Arc::new(MockNoteStore::new());
        let pipeline = create_pipeline(passing_enricher(), store);

        let result = pipeline
            .process(std::path::Path::new("/nonexistent/file.md"))
            .await;
        assert!(result.is_err(), "nonexistent file should error");
    }

    #[test]
    fn pipeline_config_default_values() {
        let config = NotePipelineConfig::default();
        assert!(!config.skip_enrichment);
        assert!(!config.force_reprocess);
    }

    #[tokio::test]
    async fn force_reprocess_overrides_skip() {
        let change_detector = Arc::new(InMemoryChangeDetectionStore::new());
        let store: Arc<dyn NoteStore> = Arc::new(MockNoteStore::new());

        let config = NotePipelineConfig {
            skip_enrichment: true,
            force_reprocess: true,
        };
        let pipeline =
            NotePipeline::with_config(change_detector, passing_enricher(), store, config);

        let tmp = write_temp_note("# Force Test\n\nContent here.\n");

        // First pass
        let r1 = pipeline.process(tmp.path()).await.unwrap();
        assert!(!r1.is_skipped(), "first pass should process");

        // Second pass with force_reprocess=true should also process
        let r2 = pipeline.process(tmp.path()).await.unwrap();
        assert!(
            !r2.is_skipped(),
            "force_reprocess should override skip for same file"
        );
    }

    #[tokio::test]
    async fn empty_markdown_processes_successfully() {
        let store = Arc::new(MockNoteStore::new());
        let pipeline = create_pipeline(passing_enricher(), store);

        let tmp = write_temp_note("");
        let result = pipeline.process(tmp.path()).await.unwrap();
        assert!(
            !result.is_skipped(),
            "empty markdown should process, not skip"
        );
    }

    #[tokio::test]
    async fn malformed_frontmatter_returns_success_with_warnings() {
        let store = Arc::new(MockNoteStore::new());
        let pipeline = create_pipeline(passing_enricher(), store);

        let tmp = write_temp_note("---\ntitle: [unterminated\n---\n\n# Heading\n");
        let result = pipeline.process(tmp.path()).await.unwrap();

        match result {
            ProcessingResult::Success { warnings, .. } => {
                assert!(
                    !warnings.is_empty(),
                    "malformed frontmatter should produce parse warnings"
                );
            }
            other => panic!("expected success result, got: {other:?}"),
        }
    }
}
