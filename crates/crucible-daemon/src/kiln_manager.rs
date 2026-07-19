//! Multi-kiln connection manager
//!
//! Manages connections to multiple kilns on-demand with idle timeout.
//! Supports SQLite backend via feature flags.

use anyhow::Result;
use crucible_core::config::read_kiln_config;
use crucible_core::events::InternalSessionEvent;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

use crate::pipeline::{NotePipeline, NotePipelineConfig};
use crate::watch::{EventFilter, WatchManager, WatchManagerConfig};
use crucible_core::processing::InMemoryChangeDetectionStore;
use crucible_core::storage::note_store::NoteRecord;
use crucible_core::storage::VectorStore;
use crucible_core::traits::{KnowledgeRepository, NoteInfo};
use crucible_core::EXCLUDED_DIRS;

use crate::embedding::get_or_create_embedding_provider;
use crate::file_watch_bridge::create_event_bridge;
use crate::protocol::SessionEventMessage;

use crucible_core::config::EmbeddingProviderConfig;

/// Canonicalize a path, falling back to the path as-given if it cannot be
/// resolved (e.g. the file was deleted, or lives on a filesystem that does not
/// support canonicalization).
fn canonical_or_self(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Normalize a file path to be relative to the kiln root.
///
/// Strips the kiln prefix (canonical or as-given) and normalizes
/// separators to forward slashes. Returns `None` if the path is not
/// inside the kiln.
pub fn normalize_note_path(file_path: &Path, kiln_path: &Path) -> Option<String> {
    let canonical = canonical_or_self(kiln_path);
    let relative = file_path
        .strip_prefix(&canonical)
        .or_else(|_| file_path.strip_prefix(kiln_path))
        .ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

// Backend-specific imports
use crate::storage::lance::vector_index::DEFAULT_EMBEDDING_DIM;
use crate::storage::lance::LanceVectorIndex;
use crate::storage::sqlite::{adapters as sqlite_adapters, SqliteClientHandle, SqliteConfig};

// ===========================================================================
// Constants
// ===========================================================================

// ===========================================================================
// Backend Abstraction
// ===========================================================================

/// Per-kiln storage. SQLite owns metadata, properties, and the
/// `KnowledgeRepository` surface; LanceDB owns the vector index. Both
/// always present; there is no single-backend mode.
#[derive(Clone)]
pub struct StorageHandle {
    pub sqlite: SqliteClientHandle,
    pub vectors: Arc<LanceVectorIndex>,
}

impl StorageHandle {
    /// Stable label for diagnostic logs.
    pub fn backend_name(&self) -> &'static str {
        "sqlite+lance"
    }

    /// Note metadata store (SQLite).
    pub fn as_note_store(&self) -> Arc<dyn crucible_core::storage::NoteStore> {
        self.sqlite.as_note_store()
    }

    /// Property/EAV store (SQLite).
    pub fn as_property_store(&self) -> Arc<dyn crucible_core::storage::PropertyStore> {
        self.sqlite.as_property_store()
    }

    /// Vector similarity search.
    ///
    /// Returns (document_id, score) pairs sorted by similarity descending.
    /// Vector index lives in LanceDB; the document_id is the note path
    /// stored at upsert time and looked up in SQLite if metadata hydration
    /// is required.
    ///
    /// Scope-aware vector search.
    ///
    /// Over-fetches the Lance index (~2x `limit`, capped at 4x) so the
    /// SQLite scope post-filter doesn't return fewer than `limit` results
    /// when the filter rejects some hits. Lance is the similarity oracle;
    /// SQLite is the scope oracle. Both layers must approve for a hit to
    /// reach the caller.
    pub async fn search_vectors(
        &self,
        vector: Vec<f32>,
        limit: usize,
        authority: &crucible_core::storage::Scope,
    ) -> Result<Vec<(String, f64)>> {
        let fetch = limit.max(1).saturating_mul(2).min(limit.max(1) * 4);
        let matches = self.vectors.search(&vector, fetch).await?;

        // Post-filter: hydrate each match through the SQLite note store
        // (scoped read). If the record is missing or out-of-scope, drop it.
        // `NoteStore::get` is already authority-aware so we get both
        // existence and visibility in one call.
        let note_store = self.sqlite.as_note_store();
        let mut filtered: Vec<(String, f64)> = Vec::with_capacity(matches.len());
        for m in matches {
            let visible = note_store
                .get(&m.id, authority)
                .await
                .map(|opt| opt.is_some())
                .unwrap_or(false);
            if visible {
                filtered.push((m.id, m.similarity as f64));
                if filtered.len() >= limit {
                    break;
                }
            }
        }
        Ok(filtered)
    }

    /// List notes by metadata filter. Always reads from SQLite.
    ///
    /// `authority` is the request authority — see [`crucible_core::storage::Scope`].
    /// Records whose stored scope is outside the caller's authority are
    /// filtered out at the SQL layer.
    pub async fn list_notes(
        &self,
        path_filter: Option<&str>,
        authority: &crucible_core::storage::Scope,
    ) -> Result<Vec<NoteInfo>> {
        let records = self.sqlite.as_note_store().list(authority).await?;
        Ok(records
            .into_iter()
            .filter(|r| path_filter.is_none_or(|p| r.path.contains(p)))
            .map(|r| NoteInfo {
                name: std::path::Path::new(&r.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&r.path)
                    .to_string(),
                path: r.path,
                title: Some(r.title),
                tags: r.tags,
                created_at: None,
                updated_at: Some(r.updated_at),
            })
            .collect())
    }

    /// Case-insensitive fuzzy lookup by path or title.
    ///
    /// `authority` is the request authority — see [`crucible_core::storage::Scope`].
    pub async fn get_note_by_name(
        &self,
        name: &str,
        authority: &crucible_core::storage::Scope,
    ) -> Result<Option<NoteRecord>> {
        let records = self.sqlite.as_note_store().list(authority).await?;
        let name_lower = name.to_lowercase();
        Ok(records.into_iter().find(|r| {
            r.path.to_lowercase().contains(&name_lower)
                || r.title.to_lowercase().contains(&name_lower)
        }))
    }

    /// Resolve a note by name and collect the notes that wikilink to it.
    ///
    /// Returns `None` if `name` resolves to no note. The second element is
    /// the backlink sources as [`NoteInfo`], sorted by path for stable output.
    ///
    /// `authority` is the request authority — see [`crucible_core::storage::Scope`].
    pub async fn get_backlinks(
        &self,
        name: &str,
        authority: &crucible_core::storage::Scope,
    ) -> Result<Option<(NoteRecord, Vec<NoteInfo>)>> {
        let records = self.sqlite.as_note_store().list(authority).await?;
        let name_lower = name.to_lowercase();
        let Some(target) = records
            .iter()
            .find(|r| {
                r.path.to_lowercase().contains(&name_lower)
                    || r.title.to_lowercase().contains(&name_lower)
            })
            .cloned()
        else {
            return Ok(None);
        };

        // Deterministic backlinks from the resolved-link index — the same
        // resolver the rename rewrite uses, so what backlinks show is exactly
        // what a rename would rewrite. (The old fuzzy candidate matching made
        // [[async]] structurally match EVERY note whose stem is `async`.)
        let sources = self.sqlite.as_note_store().backlinks(&target.path).await?;
        let source_set: std::collections::HashSet<&str> =
            sources.iter().map(String::as_str).collect();
        let mut backlinks: Vec<NoteInfo> = records
            .into_iter()
            .filter(|r| r.path != target.path)
            .filter(|r| source_set.contains(r.path.as_str()))
            .map(|r| NoteInfo {
                name: Path::new(&r.path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&r.path)
                    .to_string(),
                path: r.path,
                title: Some(r.title),
                tags: r.tags,
                created_at: None,
                updated_at: Some(r.updated_at),
            })
            .collect();
        backlinks.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Some((target, backlinks)))
    }

    /// Knowledge repository trait surface (SQLite-backed).
    pub fn as_knowledge_repository(&self) -> Arc<dyn KnowledgeRepository> {
        self.sqlite.as_knowledge_repository()
    }
}

/// Rebuild the resolved-link index for every note in a kiln by re-parsing
/// the files on disk. Runs once after the note_links v1→v2 migration (the
/// old rows carried raw text without spans and were unrecoverable in place).
/// Best-effort: unreadable/unparseable files are skipped with a warning.
async fn relink_kiln(root: &Path, store: &dyn crucible_core::storage::NoteStore) {
    use crucible_core::parser::{traits::MarkdownParser, CrucibleParser};
    use crucible_core::storage::{LinkOccurrence, Scope};

    let authority = Scope::workspace_unchecked(root);
    let records = match store.list(&authority).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "link relink: listing notes failed; index stays empty until notes are re-processed");
            return;
        }
    };
    let parser = CrucibleParser::new();
    let mut relinked = 0usize;
    for rec in &records {
        let file = root.join(&rec.path);
        let parsed = match parser.parse_file(&file).await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(path = %rec.path, error = %e, "link relink: parse failed, skipping");
                continue;
            }
        };
        let links: Vec<LinkOccurrence> = parsed
            .wikilinks
            .iter()
            .map(|w| LinkOccurrence {
                raw_target: w.target.clone(),
                span_start: parsed.body_offset + w.target_span.0,
                span_end: parsed.body_offset + w.target_span.1,
                is_embed: w.is_embed,
            })
            .collect();
        if let Err(e) = store.reindex_links(&rec.path, &links).await {
            tracing::warn!(path = %rec.path, error = %e, "link relink: write failed");
            continue;
        }
        relinked += 1;
    }
    info!(
        notes = relinked,
        "Resolved-link index rebuilt (note_links v2 migration)"
    );
}

// ===========================================================================
// KilnConnection and KilnManager
// ===========================================================================

/// Connection to a single kiln
pub struct KilnConnection {
    pub handle: StorageHandle,
    pub pipeline: NotePipeline,
    pub name: Option<String>,
    pub last_access: Instant,
    watch_manager: Option<WatchManager>,
}

/// Manages connections to multiple kilns
pub struct KilnManager {
    connections: RwLock<HashMap<PathBuf, KilnConnection>>,
    event_tx: Option<broadcast::Sender<SessionEventMessage>>,
    enrichment_config: Option<EmbeddingProviderConfig>,
    max_precognition_chars: usize,
}

impl KilnManager {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            event_tx: None,
            enrichment_config: None,
            max_precognition_chars: crucible_core::config::default_max_precognition_chars(),
        }
    }

    pub fn with_event_tx(
        event_tx: broadcast::Sender<SessionEventMessage>,
        enrichment_config: Option<EmbeddingProviderConfig>,
        max_precognition_chars: usize,
    ) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            event_tx: Some(event_tx),
            enrichment_config,
            max_precognition_chars,
        }
    }

    pub fn enrichment_config(&self) -> Option<&EmbeddingProviderConfig> {
        self.enrichment_config.as_ref()
    }

    pub fn max_precognition_chars(&self) -> usize {
        self.max_precognition_chars
    }

    /// Open a connection to a kiln (or return existing)
    pub async fn open(&self, kiln_path: &Path) -> Result<()> {
        let canonical = canonical_or_self(kiln_path);

        {
            let conns = self.connections.read().await;
            if conns.contains_key(&canonical) {
                return Ok(()); // Already open
            }
        }

        let db_path = canonical.join(".crucible").join("crucible-sqlite.db");

        info!("Opening kiln at {:?}", db_path);

        let handle =
            create_storage_handle(&db_path, &canonical, self.enrichment_config.as_ref()).await?;
        info!(
            "Kiln opened with {} backend at {:?}",
            handle.backend_name(),
            db_path
        );

        // Phase-3 one-time migration: note_links v1 stored raw text and was
        // dropped on upgrade; rebuild the resolved-link index by re-parsing
        // links from disk (parse only — embeddings are untouched).
        {
            let store = handle.as_note_store();
            if store.needs_link_reindex() {
                relink_kiln(&canonical, store.as_ref()).await;
            }
        }

        let mut pipeline = create_pipeline(&handle, self.enrichment_config.as_ref()).await?;
        pipeline.set_kiln_root(canonical.clone());
        info!("Pipeline created for kiln at {:?}", canonical);

        let name = read_kiln_name(&canonical);

        let watch_manager = self.start_watch_manager(&canonical).await;

        let mut conns = self.connections.write().await;
        conns.insert(
            canonical.clone(),
            KilnConnection {
                handle,
                pipeline,
                name,
                last_access: Instant::now(),
                watch_manager,
            },
        );
        // Drop the write lock before checking classification
        drop(conns);

        // Check if workspace has a data classification configured.
        // If not, emit ClassificationRequired so clients can prompt the user.
        let classification =
            crate::trust_resolution::resolve_kiln_classification(&canonical, &canonical);
        if classification.is_none() {
            if let Some(ref tx) = self.event_tx {
                let event = SessionEventMessage::new(
                    "system",
                    "classification_required",
                    serde_json::json!({ "kiln_path": canonical.to_string_lossy() }),
                );
                crate::event_emitter::emit_event(tx, event);
            }
        }

        Ok(())
    }

    /// Open a kiln and process all markdown files through the pipeline.
    ///
    /// Returns (discovered_count, processed_count, skipped_count, errors).
    /// If the kiln is already open, still runs processing.
    pub async fn open_and_process(
        &self,
        kiln_path: &Path,
        force: bool,
    ) -> Result<(usize, usize, usize, Vec<(PathBuf, String)>)> {
        // Ensure kiln is open
        self.open(kiln_path).await?;

        // Discover files
        let files = discover_markdown_files(kiln_path);
        let discovered = files.len();

        if files.is_empty() {
            info!("No markdown files found in {:?}", kiln_path);
            return Ok((0, 0, 0, Vec::new()));
        }

        info!(
            "Discovered {} markdown files in {:?}",
            discovered, kiln_path
        );

        let (processed, skipped, errors) = self.process_batch(kiln_path, &files, force).await?;
        Ok((discovered, processed, skipped, errors))
    }

    /// Close a kiln connection
    pub async fn close(&self, kiln_path: &Path) -> Result<()> {
        let canonical = canonical_or_self(kiln_path);
        let mut conns = self.connections.write().await;
        if let Some(mut conn) = conns.remove(&canonical) {
            if let Some(ref mut wm) = conn.watch_manager {
                if let Err(e) = wm.shutdown().await {
                    warn!(
                        "Failed to shutdown watch manager for {:?}: {}",
                        canonical, e
                    );
                }
            }
            info!("Closed kiln at {:?}", canonical);
        }
        Ok(())
    }

    /// List all open kilns
    pub async fn list(&self) -> Vec<(PathBuf, Option<String>, Instant)> {
        let conns = self.connections.read().await;
        conns
            .iter()
            .map(|(path, conn)| (path.clone(), conn.name.clone(), conn.last_access))
            .collect()
    }

    /// Get handle for a kiln if it's already open (does not open if closed)
    #[allow(dead_code)] // peek-without-open API, exercised by tests
    pub async fn get(&self, kiln_path: &Path) -> Option<StorageHandle> {
        let canonical = canonical_or_self(kiln_path);

        let mut conns = self.connections.write().await;
        if let Some(conn) = conns.get_mut(&canonical) {
            conn.last_access = Instant::now();
            Some(conn.handle.clone())
        } else {
            None
        }
    }

    /// Process a file through the kiln's pipeline
    ///
    /// Opens the kiln if not already open, then processes the file.
    /// Returns Ok(true) if file was processed, Ok(false) if skipped (unchanged).
    pub async fn process_file(&self, kiln_path: &Path, file_path: &Path) -> Result<bool> {
        // Ensure kiln is open
        self.open(kiln_path).await?;

        let canonical = canonical_or_self(kiln_path);

        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;

        conn.last_access = Instant::now();

        // Process file through pipeline
        use crate::pipeline::ProcessingResult;
        match conn.pipeline.process(file_path).await {
            Ok(ProcessingResult::Success { .. }) => Ok(true),
            Ok(ProcessingResult::Skipped) => Ok(false),
            Ok(ProcessingResult::NoChanges) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Like [`Self::process_file`] but bypassing change detection for this
    /// one call. A renamed note can land on a path whose change-detection
    /// state is stale-but-matching (A→B→A round trip), which would skip the
    /// reindex the rename depends on. Toggling force is safe: the connections
    /// write-lock is held across the process call, so no other pipeline use
    /// can observe the flag.
    pub async fn process_file_forced(&self, kiln_path: &Path, file_path: &Path) -> Result<bool> {
        self.open(kiln_path).await?;
        let canonical = canonical_or_self(kiln_path);
        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;
        conn.last_access = Instant::now();

        use crate::pipeline::ProcessingResult;
        conn.pipeline.set_force_reprocess(true);
        let result = conn.pipeline.process(file_path).await;
        conn.pipeline.set_force_reprocess(false);
        match result {
            Ok(ProcessingResult::Success { .. }) => Ok(true),
            Ok(_) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn handle_file_deleted(&self, kiln_path: &Path, file_path: &Path) -> Result<bool> {
        use crucible_core::events::SessionEvent;

        if !is_markdown_file(file_path) {
            return Ok(false);
        }

        self.open(kiln_path).await?;

        let canonical = canonical_or_self(kiln_path);

        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;

        conn.last_access = Instant::now();

        let relative_path = match normalize_note_path(file_path, kiln_path) {
            Some(p) => p,
            None => return Ok(false),
        };
        let event = conn.handle.as_note_store().delete(&relative_path).await?;

        // Drop the note's vector too, or it orphans in the Lance index and can
        // eat the similarity over-fetch window for later searches.
        if let Err(e) = conn.handle.vectors.delete(&relative_path).await {
            tracing::warn!(path = %relative_path, ?e, "failed to remove deleted note from vector index");
        }

        match event {
            SessionEvent::Internal(inner) => {
                if let InternalSessionEvent::NoteDeleted { existed, .. } = inner.as_ref() {
                    Ok(*existed)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    /// Process multiple files through the kiln's pipeline
    ///
    /// Returns (processed_count, skipped_count, errors)
    pub async fn process_batch(
        &self,
        kiln_path: &Path,
        file_paths: &[PathBuf],
        force: bool,
    ) -> Result<(usize, usize, Vec<(PathBuf, String)>)> {
        use crate::pipeline::ProcessingResult;

        // Ensure kiln is open
        self.open(kiln_path).await?;

        let canonical = canonical_or_self(kiln_path);

        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;

        conn.last_access = Instant::now();

        // Apply force flag to pipeline config for this batch
        conn.pipeline.set_force_reprocess(force);

        let mut processed = 0;
        let mut skipped = 0;
        let mut errors = Vec::new();

        for path in file_paths {
            match conn.pipeline.process(path).await {
                Ok(ProcessingResult::Success { warnings, .. }) => {
                    processed += 1;

                    if !warnings.is_empty() {
                        for warning in warnings {
                            warn!("Parse warning for {}: {}", path.display(), warning);
                        }
                    }
                }
                Ok(ProcessingResult::Skipped) | Ok(ProcessingResult::NoChanges) => {
                    skipped += 1;
                }
                Err(e) => {
                    errors.push((path.clone(), e.to_string()));
                }
            }
        }

        Ok((processed, skipped, errors))
    }

    pub async fn get_or_open(&self, kiln_path: &Path) -> Result<StorageHandle> {
        let canonical = canonical_or_self(kiln_path);

        // Try to get existing and update last_access
        {
            let mut conns = self.connections.write().await;
            if let Some(conn) = conns.get_mut(&canonical) {
                conn.last_access = Instant::now();
                return Ok(conn.handle.clone());
            }
        }

        // Open new connection
        self.open(kiln_path).await?;

        let conns = self.connections.read().await;
        conns
            .get(&canonical)
            .map(|c| c.handle.clone())
            .ok_or_else(|| anyhow::anyhow!("Failed to get connection after opening"))
    }

    /// Find which open kiln contains the given file path.
    ///
    /// Both the incoming `file_path` and stored kiln keys are canonicalized
    /// (kiln keys are canonicalized at `open()` time). If `file_path` cannot
    /// be canonicalized (e.g., file was deleted between event and lookup),
    /// we fall back to the raw path which may still match if the kiln key
    /// also wasn't canonicalized (defensive).
    pub async fn find_kiln_for_path(&self, file_path: &Path) -> Option<PathBuf> {
        let canonical = canonical_or_self(file_path);
        let conns = self.connections.read().await;
        conns
            .keys()
            .filter(|kiln_path| canonical.starts_with(kiln_path))
            .max_by_key(|p| p.components().count())
            .cloned()
    }

    /// Open kilns by name from a registry. Returns names of successfully opened kilns.
    /// Logs warnings for names not found in the registry or that fail to open.
    pub async fn open_named_kilns(
        &self,
        registry: &HashMap<String, crucible_core::config::KilnEntry>,
        names: &[String],
    ) -> Vec<String> {
        let mut opened = Vec::new();
        for name in names {
            if let Some(entry) = registry.get(name) {
                if entry.lazy() {
                    tracing::debug!(kiln = %name, "Skipping lazy kiln");
                    continue;
                }
                let raw_path = entry.path();
                let path = expand_tilde_path(&raw_path);
                match self.open(&path).await {
                    Ok(()) => {
                        info!(kiln = %name, path = %path.display(), "Opened project kiln");
                        opened.push(name.clone());
                    }
                    Err(e) => {
                        warn!(kiln = %name, error = %e, "Failed to open project kiln");
                    }
                }
            } else {
                warn!(kiln = %name, "Kiln not found in registry");
            }
        }
        opened
    }

    async fn start_watch_manager(&self, kiln_path: &Path) -> Option<WatchManager> {
        let event_tx = self.event_tx.as_ref()?;

        let bridge = create_event_bridge(event_tx.clone());
        let config = WatchManagerConfig {
            enable_default_handlers: true,
            queue_capacity: 1000,
            debounce_delay: std::time::Duration::from_millis(500),
            ..Default::default()
        };

        let mut wm = match WatchManager::with_emitter(config, bridge).await {
            Ok(wm) => wm,
            Err(e) => {
                warn!("Failed to create watch manager for {:?}: {}", kiln_path, e);
                return None;
            }
        };

        if let Err(e) = wm.start().await {
            warn!("Failed to start watch manager for {:?}: {}", kiln_path, e);
            return None;
        }

        let filter = EXCLUDED_DIRS
            .iter()
            .fold(EventFilter::new().with_extension("md"), |f, dir| {
                f.exclude_dir(kiln_path.join(dir))
            });

        let watch_config =
            crate::watch::traits::WatchConfig::new(format!("kiln-{}", kiln_path.display()))
                .with_filter(filter)
                .with_debounce(crate::watch::traits::DebounceConfig::new(500));

        if let Err(e) = wm.add_watch(kiln_path.to_path_buf(), watch_config).await {
            warn!("Failed to add watch for {:?}: {}", kiln_path, e);
            let _ = wm.shutdown().await;
            return None;
        }

        info!("File watcher started for kiln at {:?}", kiln_path);
        Some(wm)
    }
}

impl Default for KilnManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Backend Factory
// ===========================================================================

/// Create a NotePipeline for daemon-side file processing
///
/// Creates a pipeline with:
/// - In-memory change detection
/// - NoteStore from the storage handle
fn pipeline_config(enrichment_config: Option<&EmbeddingProviderConfig>) -> NotePipelineConfig {
    NotePipelineConfig {
        skip_enrichment: enrichment_config.is_none(),
        force_reprocess: false,
    }
}

async fn create_pipeline(
    handle: &StorageHandle,
    enrichment_config: Option<&EmbeddingProviderConfig>,
) -> Result<NotePipeline> {
    // Change detection (in-memory)
    let change_detector = Arc::new(InMemoryChangeDetectionStore::new());

    let embedding_provider = if let Some(config) = enrichment_config {
        match get_or_create_embedding_provider(config).await {
            Ok(provider) => {
                info!("Kiln enrichment active: embedding provider configured");
                Some(provider)
            }
            Err(e) => {
                tracing::warn!("Embedding provider unavailable, semantic search disabled: {e}");
                None
            }
        }
    } else {
        info!("Kiln enrichment skipped (no config)");
        None
    };
    let enricher = Arc::new(crate::enrichment::Enricher::from_optional_provider(
        embedding_provider,
    ));

    // Get NoteStore from handle
    let note_store = handle.as_note_store();

    let config = pipeline_config(enrichment_config);

    let pipeline = NotePipeline::with_config(change_detector, enricher, note_store, config)
        .with_vector_store(handle.vectors.clone());

    Ok(pipeline)
}

/// Create a storage handle for the given database path.
/// Open both backends for a kiln. SQLite for metadata + properties at
/// `<kiln>/.crucible/crucible-sqlite.db`; LanceDB vector index at
/// `<kiln>/.crucible/crucible-vectors.lance/`.
///
/// `kiln_path` is the kiln root (canonicalized by `open()`); the SQLite
/// handle is bound to it so `as_knowledge_repository()` enforces
/// `Scope::Workspace(kiln_path)` authority on reads.
async fn create_storage_handle(
    sqlite_db_path: &Path,
    kiln_path: &Path,
    enrichment_config: Option<&EmbeddingProviderConfig>,
) -> Result<StorageHandle> {
    let sqlite_config = SqliteConfig::new(sqlite_db_path);
    let sqlite = sqlite_adapters::create_sqlite_client(sqlite_config)
        .await?
        .with_kiln_path(kiln_path.to_path_buf());

    let lance_dir = sqlite_db_path
        .parent()
        .map(|p| p.join("crucible-vectors.lance"))
        .unwrap_or_else(|| PathBuf::from("crucible-vectors.lance"));

    // The Lance index dimension is fixed at open time and must match the
    // configured embedding model, or every upsert fails the length check and
    // semantic search silently returns nothing (the default fastembed model is
    // 384-dim, OpenAI 1536 — not the old hardcoded 768).
    let dimension = enrichment_config
        .and_then(|c| c.dimensions())
        .map(|d| d as usize)
        .unwrap_or(DEFAULT_EMBEDDING_DIM);
    let vectors = Arc::new(
        LanceVectorIndex::open_with_dimension(lance_dir.to_string_lossy().as_ref(), dimension)
            .await?,
    );

    Ok(StorageHandle { sqlite, vectors })
}

// ===========================================================================
// File Discovery
// ===========================================================================

/// Check if a path is a markdown file
fn is_markdown_file(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("md")
}

/// Check if a directory should be excluded from file discovery
fn is_excluded_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| EXCLUDED_DIRS.contains(&name))
        .unwrap_or(false)
}

/// Discover markdown files in a kiln directory
fn discover_markdown_files(kiln_path: &Path) -> Vec<PathBuf> {
    use walkdir::WalkDir;

    WalkDir::new(kiln_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_excluded_dir(e.path()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file() && is_markdown_file(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Expand a leading `~/` to the user's home directory.
fn expand_tilde_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if s.starts_with("~/") || s == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.join(&s[2..]);
        }
    }
    path.to_path_buf()
}

fn read_kiln_name(kiln_path: &Path) -> Option<String> {
    let config = read_kiln_config(kiln_path)?;
    let trimmed = config.kiln.name.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests;
