//! Multi-kiln connection manager
//!
//! Manages connections to multiple kilns on-demand with idle timeout.
//! Supports SQLite backend via feature flags.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

use crate::pipeline::{NotePipeline, NotePipelineConfig, ParserBackend};
use crucible_core::processing::InMemoryChangeDetectionStore;
use crucible_core::storage::note_store::NoteRecord;
use crucible_core::traits::{KnowledgeRepository, NoteInfo};
use crucible_watch::{EventFilter, WatchManager, WatchManagerConfig};

use crate::file_watch_bridge::create_event_bridge;
use crate::embedding::get_or_create_embedding_provider;
use crate::protocol::SessionEventMessage;

use crucible_config::EmbeddingProviderConfig;

// Backend-specific imports
#[cfg(feature = "storage-sqlite")]
use crucible_sqlite::{adapters as sqlite_adapters, SqliteClientHandle, SqliteConfig};

// ===========================================================================
// Constants
// ===========================================================================

/// Directories to exclude from file discovery and watching
pub const EXCLUDED_DIRS: &[&str] = &[".crucible", ".git", ".obsidian", "node_modules", ".trash"];


// ===========================================================================
// Backend Abstraction
// ===========================================================================

/// Storage backend handle that wraps the SQLite client
#[derive(Clone)]
#[allow(dead_code)] // Variants may appear unused depending on feature flags
pub enum StorageHandle {
    #[cfg(feature = "storage-sqlite")]
    Sqlite(SqliteClientHandle),
}

impl StorageHandle {
    /// Get the backend name for logging
    pub fn backend_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(_) => "sqlite",
        }
    }

    /// Get a NoteStore trait object for this storage backend
    pub fn as_note_store(&self) -> std::sync::Arc<dyn crucible_core::storage::NoteStore> {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(client) => client.as_note_store(),
        }
    }

    /// Search for similar vectors - backend-agnostic VSS
    ///
    /// Returns (document_id, score) pairs sorted by similarity descending.
    pub async fn search_vectors(
        &self,
        vector: Vec<f32>,
        limit: usize,
    ) -> Result<Vec<(String, f64)>> {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(client) => {
                use crucible_core::storage::NoteStore;
                let store = client.as_note_store();
                let results = store.search(&vector, limit, None).await?;
                Ok(results
                    .into_iter()
                    .map(|r| (r.note.path, r.score as f64))
                    .collect())
            }
        }
    }

    /// List notes in the kiln - backend-agnostic
    ///
    /// # Arguments
    ///
    /// * `path_filter` - Optional substring to filter note paths (case-sensitive).
    ///   Notes are included if their path contains this substring.
    ///
    /// # Returns
    ///
    /// A list of notes with metadata. The `name` field is extracted from the file
    /// stem of the path (e.g., "notes/daily.md" → "daily"), falling back to the
    /// full path if stem extraction fails.
    pub async fn list_notes(&self, path_filter: Option<&str>) -> Result<Vec<NoteInfo>> {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(client) => {
                use crucible_core::storage::NoteStore;
                let store = client.as_note_store();
                let records = store.list().await?;
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
        }
    }

    /// Get a note by name - backend-agnostic
    ///
    /// Performs a case-insensitive fuzzy search, returning the first note whose
    /// path or title contains the given name.
    ///
    /// # Performance
    ///
    /// Currently does a linear scan over all notes (O(n)). For large kilns with
    /// 10k+ notes, consider adding backend-specific indexed queries (e.g., SQL
    /// LIKE with index).
    pub async fn get_note_by_name(&self, name: &str) -> Result<Option<NoteRecord>> {
        use crucible_core::storage::NoteStore;

        let records: Vec<NoteRecord> = match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(client) => client.as_note_store().list().await?,
        };

        let name_lower = name.to_lowercase();
        Ok(records.into_iter().find(|r| {
            r.path.to_lowercase().contains(&name_lower)
                || r.title.to_lowercase().contains(&name_lower)
        }))
    }

    /// Get a KnowledgeRepository trait object for this storage backend
    pub fn as_knowledge_repository(&self) -> Arc<dyn KnowledgeRepository> {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(client) => client.as_knowledge_repository(),
        }
    }
}

// ===========================================================================
// KilnConnection and KilnManager
// ===========================================================================

/// Connection to a single kiln
pub struct KilnConnection {
    pub handle: StorageHandle,
    pub pipeline: NotePipeline,
    pub last_access: Instant,
    watch_manager: Option<WatchManager>,
    pub enrichment_config: Option<EmbeddingProviderConfig>,
}

/// Manages connections to multiple kilns
pub struct KilnManager {
    connections: RwLock<HashMap<PathBuf, KilnConnection>>,
    event_tx: Option<broadcast::Sender<SessionEventMessage>>,
}

impl KilnManager {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            event_tx: None,
        }
    }

    pub fn with_event_tx(event_tx: broadcast::Sender<SessionEventMessage>) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            event_tx: Some(event_tx),
        }
    }

    /// Open a connection to a kiln (or return existing)
    pub async fn open(&self, kiln_path: &Path) -> Result<()> {
        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

        {
            let conns = self.connections.read().await;
            if conns.contains_key(&canonical) {
                return Ok(()); // Already open
            }
        }

        // Use backend-specific database names
        #[cfg(feature = "storage-sqlite")]
        let db_path = canonical.join(".crucible").join("crucible-sqlite.db");

        info!("Opening kiln at {:?}", db_path);

        let handle = create_storage_handle(&db_path).await?;
        info!(
            "Kiln opened with {} backend at {:?}",
            handle.backend_name(),
            db_path
        );

        // Try to load enrichment config from kiln's crucible.toml
        let enrichment_config = load_enrichment_config(&canonical).await;

        let pipeline = create_pipeline(&handle, enrichment_config.as_ref()).await?;
        info!("Pipeline created for kiln at {:?}", canonical);

        let watch_manager = self.start_watch_manager(&canonical).await;

        let mut conns = self.connections.write().await;
        conns.insert(
            canonical.clone(),
            KilnConnection {
                handle,
                pipeline,
                last_access: Instant::now(),
                watch_manager,
                enrichment_config,
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
    /// Returns (processed_count, skipped_count, errors).
    /// If the kiln is already open, still runs processing.
    pub async fn open_and_process(
        &self,
        kiln_path: &Path,
        force: bool,
    ) -> Result<(usize, usize, Vec<(PathBuf, String)>)> {
        // Ensure kiln is open
        self.open(kiln_path).await?;

        // Discover files
        let files = discover_markdown_files(kiln_path);

        if files.is_empty() {
            info!("No markdown files found in {:?}", kiln_path);
            return Ok((0, 0, Vec::new()));
        }

        info!(
            "Discovered {} markdown files in {:?}",
            files.len(),
            kiln_path
        );

        // TODO: `force` flag will be wired when pipeline supports skipping change detection
        if force {
            warn!("--force flag accepted but not yet forwarded to pipeline");
        }

        self.process_batch(kiln_path, &files).await
    }

    /// Close a kiln connection
    pub async fn close(&self, kiln_path: &Path) -> Result<()> {
        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());
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
    pub async fn list(&self) -> Vec<(PathBuf, Instant)> {
        let conns = self.connections.read().await;
        conns
            .iter()
            .map(|(path, conn)| (path.clone(), conn.last_access))
            .collect()
    }

    /// Get handle for a kiln if it's already open (does not open if closed)
    #[allow(dead_code)]
    pub async fn get(&self, kiln_path: &Path) -> Option<StorageHandle> {
        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

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

        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

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

    pub async fn handle_file_deleted(&self, kiln_path: &Path, file_path: &Path) -> Result<bool> {
        use crucible_core::events::SessionEvent;
        use crucible_core::storage::NoteStore;

        if !is_markdown_file(file_path) {
            return Ok(false);
        }

        self.open(kiln_path).await?;

        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;

        conn.last_access = Instant::now();

        let relative_path = match file_path
            .strip_prefix(&canonical)
            .or_else(|_| file_path.strip_prefix(kiln_path))
        {
            Ok(path) => path,
            Err(_) => return Ok(false),
        };

        let relative_path = relative_path.to_string_lossy().replace('\\', "/");
        let event = conn.handle.as_note_store().delete(&relative_path).await?;

        match event {
            SessionEvent::NoteDeleted { existed, .. } => Ok(existed),
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
    ) -> Result<(usize, usize, Vec<(PathBuf, String)>)> {
        use crate::pipeline::ProcessingResult;

        // Ensure kiln is open
        self.open(kiln_path).await?;

        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

        let mut conns = self.connections.write().await;
        let conn = conns
            .get_mut(&canonical)
            .ok_or_else(|| anyhow::anyhow!("Kiln not found after opening"))?;

        conn.last_access = Instant::now();

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

    /// Get handle for a kiln, opening if needed
    /// Get enrichment configuration for a kiln if it's already open
    pub async fn get_enrichment_config(&self, kiln_path: &Path) -> Option<EmbeddingProviderConfig> {
        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

        let conns = self.connections.read().await;
        conns
            .get(&canonical)
            .and_then(|conn| conn.enrichment_config.clone())
    }

    pub async fn get_or_open(&self, kiln_path: &Path) -> Result<StorageHandle> {
        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

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
        let canonical = file_path
            .canonicalize()
            .unwrap_or_else(|_| file_path.to_path_buf());
        let conns = self.connections.read().await;
        conns
            .keys()
            .filter(|kiln_path| canonical.starts_with(kiln_path))
            .max_by_key(|p| p.components().count())
            .cloned()
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

        let filter = EventFilter::new()
            .with_extension("md")
            .exclude_dir(kiln_path.join(".crucible"))
            .exclude_dir(kiln_path.join(".git"))
            .exclude_dir(kiln_path.join(".obsidian"))
            .exclude_dir(kiln_path.join("node_modules"))
            .exclude_dir(kiln_path.join(".trash"));

        let watch_config =
            crucible_watch::traits::WatchConfig::new(format!("kiln-{}", kiln_path.display()))
                .with_filter(filter)
                .with_debounce(crucible_watch::traits::DebounceConfig::new(500));

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

// ===========================================================================
// Enrichment Configuration Loading
// ===========================================================================

/// Load enrichment configuration from a kiln's crucible.toml
///
/// Attempts to read `{kiln_path}/crucible.toml` and extract the enrichment
/// provider configuration. Returns None if the file doesn't exist or has no
/// enrichment section.
async fn load_enrichment_config(kiln_path: &Path) -> Option<EmbeddingProviderConfig> {
    let config_path = kiln_path.join("crucible.toml");

    // Try to read the config file
    let config_content = match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => content,
        Err(_) => {
            // File doesn't exist or can't be read - this is fine, enrichment is optional
            return None;
        }
    };

    // Parse as TOML
    let config: crucible_config::Config = match toml::from_str(&config_content) {
        Ok(cfg) => cfg,
        Err(e) => {
            warn!("Failed to parse crucible.toml at {:?}: {}", config_path, e);
            return None;
        }
    };

    // Extract enrichment provider config
    config.enrichment.map(|enrichment| enrichment.provider)
}

/// Create a NotePipeline for daemon-side file processing
///
/// Creates a pipeline with:
/// - In-memory change detection
/// - NoteStore from the storage handle
fn pipeline_config(enrichment_config: Option<&EmbeddingProviderConfig>) -> NotePipelineConfig {
    NotePipelineConfig {
        parser: ParserBackend::default(),
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
        info!("Kiln enrichment active: embedding provider configured");
        Some(get_or_create_embedding_provider(config).await?)
    } else {
        info!("Kiln enrichment skipped (no config)");
        None
    };
    let enrichment_service =
        crate::enrichment::create_default_enrichment_service(embedding_provider)?;

    // Get NoteStore from handle
    let note_store = handle.as_note_store();

    let config = pipeline_config(enrichment_config);

    Ok(NotePipeline::with_config(
        change_detector,
        enrichment_service,
        note_store,
        config,
    ))
}

/// Create a storage handle for the given database path.
/// Uses SQLite as the default backend.
#[allow(clippy::needless_return)] // Returns needed for cfg-gated branches
async fn create_storage_handle(db_path: &Path) -> Result<StorageHandle> {
    // SQLite is the default backend
    #[cfg(feature = "storage-sqlite")]
    {
        let config = SqliteConfig::new(db_path);
        let client = sqlite_adapters::create_sqlite_client(config).await?;
        return Ok(StorageHandle::Sqlite(client));
    }

    // If neither feature is enabled, compilation will fail here
    #[cfg(not(feature = "storage-sqlite"))]
    {
        compile_error!("At least one storage backend must be enabled");
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_config::EmbeddingProviderConfig;
    use tempfile::TempDir;

    /// Helper to get a path that doesn't exist and works cross-platform
    fn nonexistent_path() -> PathBuf {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();
        drop(tmp); // Remove the temp dir
        base.join("nonexistent").join("path")
    }

    #[test]
    fn test_excluded_dirs_constant() {
        // Verify the constant contains exactly the 5 expected directories
        assert_eq!(EXCLUDED_DIRS.len(), 5);
        assert!(EXCLUDED_DIRS.contains(&".crucible"));
        assert!(EXCLUDED_DIRS.contains(&".git"));
        assert!(EXCLUDED_DIRS.contains(&".obsidian"));
        assert!(EXCLUDED_DIRS.contains(&"node_modules"));
        assert!(EXCLUDED_DIRS.contains(&".trash"));
    }

    #[test]
    fn pipeline_config_enables_enrichment_when_provider_configured() {
        let config = pipeline_config(Some(&EmbeddingProviderConfig::mock(Some(384))));
        assert!(!config.skip_enrichment);
    }

    #[test]
    fn pipeline_config_skips_enrichment_when_provider_missing() {
        let config = pipeline_config(None);
        assert!(config.skip_enrichment);
    }

    #[tokio::test]
    async fn enrichment_config_wiring_no_config_skips_enrichment() {
        // Scenario 1: No crucible.toml → enrichment is skipped gracefully
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path();

        // No crucible.toml exists in the temp dir
        let enrichment = load_enrichment_config(kiln_path).await;
        assert!(enrichment.is_none(), "Expected None when no crucible.toml exists");

        // Pipeline config should skip enrichment
        let config = pipeline_config(enrichment.as_ref());
        assert!(
            config.skip_enrichment,
            "skip_enrichment should be true when no config is present"
        );
    }

    #[tokio::test]
    async fn enrichment_config_wiring_with_config_enables_enrichment() {
        // Scenario 2: crucible.toml with [enrichment.provider] → enrichment enabled
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path();

        // Write a minimal crucible.toml with a mock enrichment provider
        let config_content = r#"
[enrichment.provider]
type = "mock"
model = "test-model"
dimensions = 384

[enrichment.pipeline]
batch_size = 16
"#;
        std::fs::write(kiln_path.join("crucible.toml"), config_content).unwrap();

        // load_enrichment_config should find and parse the provider
        let enrichment = load_enrichment_config(kiln_path).await;
        assert!(
            enrichment.is_some(),
            "Expected Some(EmbeddingProviderConfig) when crucible.toml has enrichment section"
        );

        // Verify it's the mock provider we configured
        let provider = enrichment.as_ref().unwrap();
        assert!(matches!(provider, EmbeddingProviderConfig::Mock(_)));
        assert_eq!(provider.model(), "test-model");
        assert_eq!(provider.dimensions(), Some(384));

        // Pipeline config should enable enrichment
        let config = pipeline_config(enrichment.as_ref());
        assert!(
            !config.skip_enrichment,
            "skip_enrichment should be false when enrichment config is present"
        );
    }

    #[tokio::test]
    async fn enrichment_config_wiring_malformed_toml_skips_gracefully() {
        // Edge case: malformed crucible.toml should not crash, just skip enrichment
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path();

        std::fs::write(kiln_path.join("crucible.toml"), "not valid { toml ]").unwrap();

        let enrichment = load_enrichment_config(kiln_path).await;
        assert!(
            enrichment.is_none(),
            "Malformed TOML should return None, not panic"
        );

        let config = pipeline_config(enrichment.as_ref());
        assert!(config.skip_enrichment);
    }

    #[tokio::test]
    async fn enrichment_config_wiring_toml_without_enrichment_section_skips() {
        // Edge case: valid crucible.toml but no enrichment section
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path();

        // Valid TOML but no enrichment section
        std::fs::write(kiln_path.join("crucible.toml"), "[storage]\npath = \"/tmp\"").unwrap();

        let enrichment = load_enrichment_config(kiln_path).await;
        assert!(
            enrichment.is_none(),
            "Config without enrichment section should return None"
        );

        let config = pipeline_config(enrichment.as_ref());
        assert!(config.skip_enrichment);
    }

    #[tokio::test]
    async fn test_kiln_manager_new() {
        let km = KilnManager::new();
        let list = km.list().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_open_creates_kiln_if_needed() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        // Open should succeed (creates new kiln)
        let result = km.open(&kiln_path).await;
        assert!(result.is_ok());

        // Should now be in the list
        let list = km.list().await;
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_close_unopened_kiln_succeeds() {
        let km = KilnManager::new();
        let path = nonexistent_path();
        // Closing a kiln that was never opened should succeed (no-op)
        let result = km.close(&path).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_list_empty_initially() {
        let km = KilnManager::new();
        let list = km.list().await;
        assert_eq!(list.len(), 0);
    }

    #[tokio::test]
    async fn test_close_removes_from_list() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        // Open the kiln
        km.open(&kiln_path).await.unwrap();

        // Verify it's in the list
        let list = km.list().await;
        assert_eq!(list.len(), 1);

        // Close it
        km.close(&kiln_path).await.unwrap();

        // Verify it's no longer in the list
        let list = km.list().await;
        assert_eq!(list.len(), 0);
    }

    #[tokio::test]
    async fn test_default_trait() {
        let km = KilnManager::default();
        let list = km.list().await;
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn test_get_or_open_creates_kiln() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        let result = km.get_or_open(&kiln_path).await;
        assert!(result.is_ok());

        // Should now be in the list
        let list = km.list().await;
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_get_or_open_reuses_existing() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        // First call creates the kiln
        let _handle1 = km.get_or_open(&kiln_path).await.unwrap();

        // Second call should reuse the same connection
        let _handle2 = km.get_or_open(&kiln_path).await.unwrap();

        // Should only have one entry in the list
        let list = km.list().await;
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_get_returns_none_if_not_open() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        // get() should return None if kiln is not open
        let result = km.get(&kiln_path).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_returns_handle_if_open() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        // Open the kiln first
        km.open(&kiln_path).await.unwrap();

        // get() should now return Some(handle)
        let result = km.get(&kiln_path).await;
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_find_kiln_for_path_returns_matching_kiln() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("my_kiln");

        km.open(&kiln_path).await.unwrap();

        let file_in_kiln = kiln_path.join("notes").join("test.md");
        let result = km.find_kiln_for_path(&file_in_kiln).await;
        assert!(result.is_some());
        assert_eq!(
            result.unwrap(),
            kiln_path.canonicalize().unwrap_or(kiln_path)
        );
    }

    #[tokio::test]
    async fn test_find_kiln_for_path_returns_none_for_unrelated_path() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("my_kiln");

        km.open(&kiln_path).await.unwrap();

        let unrelated = PathBuf::from("/some/other/path/note.md");
        let result = km.find_kiln_for_path(&unrelated).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_find_kiln_for_path_returns_none_when_no_kilns_open() {
        let km = KilnManager::new();
        let path = PathBuf::from("/any/path/note.md");
        let result = km.find_kiln_for_path(&path).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_find_kiln_for_path_with_multiple_kilns() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_a = tmp.path().join("kiln_a");
        let kiln_b = tmp.path().join("kiln_b");

        km.open(&kiln_a).await.unwrap();
        km.open(&kiln_b).await.unwrap();

        let file_in_b = kiln_b.join("sub").join("test.md");
        let result = km.find_kiln_for_path(&file_in_b).await;
        assert!(result.is_some());
        assert_eq!(result.unwrap(), kiln_b.canonicalize().unwrap_or(kiln_b));
    }

    #[tokio::test]
    async fn test_get_updates_last_access() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        // Open and get initial access time
        km.open(&kiln_path).await.unwrap();
        let initial_list = km.list().await;
        let initial_time = initial_list[0].1;

        // Wait a bit
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Call get()
        let _ = km.get(&kiln_path).await;

        // Last access should be updated
        let updated_list = km.list().await;
        let updated_time = updated_list[0].1;

        assert!(updated_time > initial_time);
    }

    #[tokio::test]
    async fn test_file_deleted_removes_note_after_processing() {
        use crucible_core::storage::NoteStore;

        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        // Create 3 markdown files
        std::fs::write(
            kiln_path.join("alpha.md"),
            "---\ntitle: Alpha\n---\n\nAlpha content.\n",
        )
        .unwrap();
        std::fs::write(
            kiln_path.join("beta.md"),
            "---\ntitle: Beta\n---\n\nBeta content.\n",
        )
        .unwrap();
        std::fs::write(
            kiln_path.join("gamma.md"),
            "---\ntitle: Gamma\n---\n\nGamma content.\n",
        )
        .unwrap();

        let km = KilnManager::new();

        // Process all files — they should all land in the DB
        let (processed, _skipped, errors) =
            km.open_and_process(&kiln_path, false).await.unwrap();
        assert!(errors.is_empty(), "processing errors: {:?}", errors);
        assert_eq!(processed, 3, "all 3 files should be processed");

        // Verify all 3 notes exist in the store
        let handle = km.get_or_open(&kiln_path).await.unwrap();
        let note_store = handle.as_note_store();
        let notes = note_store.list().await.unwrap();
        assert_eq!(notes.len(), 3, "DB should contain 3 notes after processing");

        // Delete beta.md from disk
        let beta_abs = kiln_path.join("beta.md");
        std::fs::remove_file(&beta_abs).unwrap();

        // Handle the deletion through KilnManager
        let existed = km
            .handle_file_deleted(&kiln_path, &beta_abs)
            .await
            .unwrap();
        assert!(existed, "handle_file_deleted should report the note existed");

        // Verify DB now has exactly 2 notes
        let notes = note_store.list().await.unwrap();
        assert_eq!(notes.len(), 2, "DB should contain 2 notes after deletion");

        // Verify the deleted note is gone
        assert!(
            note_store.get("beta.md").await.unwrap().is_none(),
            "deleted note should not be in the store",
        );

        // Verify the remaining 2 notes are intact
        let alpha = note_store.get("alpha.md").await.unwrap();
        assert!(alpha.is_some(), "alpha.md should still exist");
        assert_eq!(alpha.unwrap().title, "Alpha");

        let gamma = note_store.get("gamma.md").await.unwrap();
        assert!(gamma.is_some(), "gamma.md should still exist");
        assert_eq!(gamma.unwrap().title, "Gamma");
    }
}
