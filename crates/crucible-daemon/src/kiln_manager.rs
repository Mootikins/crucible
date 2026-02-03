//! Multi-kiln connection manager
//!
//! Manages connections to multiple kilns on-demand with idle timeout.
//! Supports both SQLite (default) and SurrealDB backends via feature flags.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

use crucible_core::processing::InMemoryChangeDetectionStore;
use crucible_core::storage::note_store::NoteRecord;
use crucible_core::traits::NoteInfo;
use crucible_pipeline::{NotePipeline, NotePipelineConfig, ParserBackend};
use crucible_watch::{EventFilter, WatchManager, WatchManagerConfig};

use crate::file_watch_bridge::create_event_bridge;
use crate::protocol::SessionEventMessage;

// Backend-specific imports
#[cfg(feature = "storage-sqlite")]
use crucible_sqlite::{adapters as sqlite_adapters, SqliteClientHandle, SqliteConfig};

#[cfg(feature = "storage-surrealdb")]
use crucible_surrealdb::adapters::SurrealClientHandle;

#[cfg(all(feature = "storage-surrealdb", not(feature = "storage-sqlite")))]
use crucible_surrealdb::{adapters as surreal_adapters, SurrealDbConfig};

// ===========================================================================
// Backend Abstraction
// ===========================================================================

/// Storage backend handle that wraps either SQLite or SurrealDB client
#[derive(Clone)]
#[allow(dead_code)] // Variants may appear unused when both features enabled (SQLite takes precedence)
pub enum StorageHandle {
    #[cfg(feature = "storage-sqlite")]
    Sqlite(SqliteClientHandle),

    #[cfg(feature = "storage-surrealdb")]
    Surreal(SurrealClientHandle),
}

impl StorageHandle {
    /// Get the backend name for logging
    pub fn backend_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(_) => "sqlite",

            #[cfg(feature = "storage-surrealdb")]
            StorageHandle::Surreal(_) => "surrealdb",
        }
    }

    /// Get a NoteStore trait object for this storage backend
    pub fn as_note_store(&self) -> std::sync::Arc<dyn crucible_core::storage::NoteStore> {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(client) => client.as_note_store(),

            #[cfg(feature = "storage-surrealdb")]
            StorageHandle::Surreal(client) => client.as_note_store(),
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

            #[cfg(feature = "storage-surrealdb")]
            StorageHandle::Surreal(client) => {
                use crucible_core::database::SearchResult;
                let repo = client.as_knowledge_repository();
                let results: Vec<SearchResult> = repo.search_vectors(vector).await?;
                let pairs: Vec<(String, f64)> = results
                    .into_iter()
                    .take(limit)
                    .map(|r| (r.document_id.0, r.score))
                    .collect();
                Ok(pairs)
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

            #[cfg(feature = "storage-surrealdb")]
            StorageHandle::Surreal(client) => {
                let repo = client.as_knowledge_repository();
                let notes: Vec<NoteInfo> = repo.list_notes(path_filter).await?;
                Ok(notes)
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
    /// LIKE with index, or SurrealDB string functions).
    pub async fn get_note_by_name(&self, name: &str) -> Result<Option<NoteRecord>> {
        use crucible_core::storage::NoteStore;

        let records: Vec<NoteRecord> = match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(client) => client.as_note_store().list().await?,

            #[cfg(feature = "storage-surrealdb")]
            StorageHandle::Surreal(client) => client.as_note_store().list().await?,
        };

        let name_lower = name.to_lowercase();
        Ok(records.into_iter().find(|r| {
            r.path.to_lowercase().contains(&name_lower)
                || r.title.to_lowercase().contains(&name_lower)
        }))
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

        // Use backend-specific database names so SQLite and SurrealDB can coexist
        #[cfg(feature = "storage-sqlite")]
        let db_path = canonical.join(".crucible").join("crucible-sqlite.db");

        #[cfg(all(feature = "storage-surrealdb", not(feature = "storage-sqlite")))]
        let db_path = canonical.join(".crucible").join("crucible-surreal.db");
        info!("Opening kiln at {:?}", db_path);

        let handle = create_storage_handle(&db_path).await?;
        info!(
            "Kiln opened with {} backend at {:?}",
            handle.backend_name(),
            db_path
        );

        // Create pipeline for this kiln
        let pipeline = create_pipeline(&handle)?;
        info!("Pipeline created for kiln at {:?}", canonical);

        let watch_manager = self.start_watch_manager(&canonical).await;

        let mut conns = self.connections.write().await;
        conns.insert(
            canonical,
            KilnConnection {
                handle,
                pipeline,
                last_access: Instant::now(),
                watch_manager,
            },
        );

        Ok(())
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
                    warn!("Failed to shutdown watch manager for {:?}: {}", canonical, e);
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
        use crucible_pipeline::ProcessingResult;
        match conn.pipeline.process(file_path).await {
            Ok(ProcessingResult::Success { .. }) => Ok(true),
            Ok(ProcessingResult::Skipped) => Ok(false),
            Ok(ProcessingResult::NoChanges) => Ok(false),
            Err(e) => Err(e),
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
        use crucible_pipeline::ProcessingResult;

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
                Ok(ProcessingResult::Success { .. }) => {
                    processed += 1;
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
            .exclude_dir(kiln_path.join(".crucible"));

        let watch_config = crucible_watch::traits::WatchConfig::new(format!(
            "kiln-{}",
            kiln_path.display()
        ))
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

/// Create a NotePipeline for daemon-side file processing
///
/// Creates a pipeline with:
/// - In-memory change detection
/// - Enrichment disabled (parsing only for now - embeddings can be added later)
/// - NoteStore from the storage handle
///
/// This allows the daemon to process files without requiring embedding configuration.
fn create_pipeline(handle: &StorageHandle) -> Result<NotePipeline> {
    // Change detection (in-memory)
    let change_detector = Arc::new(InMemoryChangeDetectionStore::new());

    // Enrichment service with embeddings disabled
    // TODO: Add embedding support with daemon configuration
    let enrichment_service = crucible_enrichment::create_default_enrichment_service(None)?;

    // Get NoteStore from handle
    let note_store = handle.as_note_store();

    // Pipeline configuration - skip enrichment for now (parsing only)
    let config = NotePipelineConfig {
        parser: ParserBackend::default(),
        skip_enrichment: true, // No embeddings until daemon has embedding config
        force_reprocess: false,
    };

    Ok(NotePipeline::with_config(
        change_detector,
        enrichment_service,
        note_store,
        config,
    ))
}

/// Create a storage handle for the given database path.
/// Uses SQLite by default, SurrealDB if SQLite feature is disabled.
#[allow(clippy::needless_return)] // Returns needed for cfg-gated branches
async fn create_storage_handle(db_path: &Path) -> Result<StorageHandle> {
    // SQLite is the default backend
    #[cfg(feature = "storage-sqlite")]
    {
        let config = SqliteConfig::new(db_path);
        let client = sqlite_adapters::create_sqlite_client(config).await?;
        return Ok(StorageHandle::Sqlite(client));
    }

    // Fall back to SurrealDB if SQLite is not enabled
    #[cfg(all(feature = "storage-surrealdb", not(feature = "storage-sqlite")))]
    {
        let config = SurrealDbConfig {
            path: db_path.to_string_lossy().to_string(),
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            ..Default::default()
        };

        let client = surreal_adapters::create_surreal_client(config).await?;

        // Initialize schema on first open (idempotent)
        crucible_surrealdb::kiln_integration::initialize_kiln_schema(client.inner()).await?;

        return Ok(StorageHandle::Surreal(client));
    }

    // If neither feature is enabled, compilation will fail here
    #[cfg(not(any(feature = "storage-sqlite", feature = "storage-surrealdb")))]
    {
        compile_error!("At least one storage backend must be enabled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper to get a path that doesn't exist and works cross-platform
    fn nonexistent_path() -> PathBuf {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();
        drop(tmp); // Remove the temp dir
        base.join("nonexistent").join("path")
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
        assert_eq!(
            result.unwrap(),
            kiln_b.canonicalize().unwrap_or(kiln_b)
        );
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
}
