//! Storage factory - creates storage implementations
//!
//! This is part of the composition root where concrete types are wired together.
//! Phase 5: Uses public adapters API instead of importing concrete types.
//!
//! ## Available Factories
//!
//! - `get_storage` - Unified factory that returns daemon or backend-specific storage based on config
//! - `create_daemon_storage` - Daemon-backed storage (preferred, auto-starts daemon)

use crate::config::CliConfig;
use anyhow::Result;
use crucible_config::StorageMode;
use crucible_core::storage::NoteStore;
use crucible_core::traits::StorageClient;
use crucible_rpc::{DaemonClient, DaemonNoteStore, DaemonStorageClient};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};

/// Create daemon-backed storage client (preferred path)
///
/// This creates a storage client that connects to the daemon, automatically
/// starting it if not already running. This is the preferred way to access
/// storage in the CLI as it enables:
///
/// - Shared connection pooling across multiple CLI invocations
/// - Automatic daemon lifecycle management
/// - Multi-kiln support with idle timeout
///
/// # Arguments
///
/// * `kiln_path` - Path to the kiln (notes directory)
///
/// # Returns
///
/// A storage client that implements the StorageClient trait, backed by the daemon.
///
/// # Example
///
/// ```rust,no_run
/// # use std::path::Path;
/// # use anyhow::Result;
/// # async fn example() -> Result<()> {
/// use crucible_cli::factories;
/// use crucible_core::traits::StorageClient;
///
/// let kiln_path = Path::new("/home/user/notes");
/// let storage = factories::create_daemon_storage(kiln_path).await?;
///
/// // Use the storage client
/// let result = storage.query_raw("SELECT * FROM notes LIMIT 10").await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_daemon_storage(kiln_path: &Path) -> Result<Arc<DaemonStorageClient>> {
    let client = Arc::new(DaemonClient::connect_or_start().await?);
    Ok(Arc::new(DaemonStorageClient::new(
        client,
        kiln_path.to_path_buf(),
    )))
}

// =============================================================================
// Unified Storage Factory (storage.mode aware)
// =============================================================================

/// Handle for either daemon-backed or direct storage
///
/// This enum allows the CLI to use storage abstractly without caring
/// whether it's a direct connection or daemon-backed.
#[derive(Clone)]
pub enum StorageHandle {
    /// Daemon-backed storage (multi-session via Unix socket)
    Daemon(Arc<DaemonStorageClient>),
    /// Lightweight mode (LanceDB vectors + ripgrep text search)
    Lightweight(Arc<crucible_lance::LanceNoteStore>),
    /// SQLite mode (experimental alternative)
    #[cfg(feature = "storage-sqlite")]
    Sqlite(Arc<crucible_sqlite::SqliteNoteStore>),
}

impl StorageHandle {
    /// Execute a raw query and return JSON
    ///
    /// This provides a unified query interface regardless of storage mode.
    /// Lightweight mode does not support SQL queries and will return an error.
    pub async fn query_raw(&self, sql: &str) -> Result<serde_json::Value> {
        match self {
            StorageHandle::Daemon(c) => c.query_raw(sql).await,
            StorageHandle::Lightweight(_) => {
                crate::output::storage_warning("SQL queries");
                anyhow::bail!(
                    "SQL queries not supported in lightweight mode.\n\
                     Configure storage.mode = \"embedded\" or \"daemon\" for full query support."
                )
            }
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(_) => {
                crate::output::storage_warning("SQL queries");
                anyhow::bail!(
                    "Raw SQL queries not supported in SQLite mode.\n\
                     Use NoteStore methods instead, or configure storage.mode = \"embedded\"."
                )
            }
        }
    }

    /// Check if running in daemon mode
    pub fn is_daemon(&self) -> bool {
        matches!(self, StorageHandle::Daemon(_))
    }

    /// Check if running in lightweight mode
    pub fn is_lightweight(&self) -> bool {
        matches!(self, StorageHandle::Lightweight(_))
    }

    /// Check if running in SQLite mode
    #[cfg(feature = "storage-sqlite")]
    pub fn is_sqlite(&self) -> bool {
        matches!(self, StorageHandle::Sqlite(_))
    }

    /// Get the DaemonStorageClient if in daemon mode
    ///
    /// Returns `Some(&Arc<DaemonStorageClient>)` if running in daemon mode, `None` otherwise.
    pub fn as_daemon_client(&self) -> Option<&Arc<DaemonStorageClient>> {
        match self {
            StorageHandle::Daemon(c) => Some(c),
            _ => None,
        }
    }

    /// List notes in the kiln (backend-agnostic)
    ///
    /// This provides a unified interface for listing notes regardless of storage mode.
    /// Uses the KnowledgeRepository trait which is implemented for all storage backends.
    pub async fn list_notes(
        &self,
        path_filter: Option<&str>,
    ) -> Result<Vec<crucible_core::traits::NoteInfo>> {
        use crucible_core::traits::KnowledgeRepository;

        let repo = self
            .as_knowledge_repository(None)
            .ok_or_else(|| anyhow::anyhow!("list_notes not supported in lightweight mode"))?;

        repo.list_notes(path_filter)
            .await
            .map_err(|e| anyhow::anyhow!("list_notes failed: {}", e))
    }

    /// Get the SqliteNoteStore if in SQLite mode
    #[cfg(feature = "storage-sqlite")]
    pub fn try_sqlite_note_store(&self) -> Option<&Arc<crucible_sqlite::SqliteNoteStore>> {
        match self {
            StorageHandle::Sqlite(store) => Some(store),
            _ => None,
        }
    }

    /// Get the LanceNoteStore if in lightweight mode
    ///
    /// Returns a reference to the underlying LanceNoteStore for lightweight mode.
    pub fn try_lance_note_store(&self) -> Option<&Arc<crucible_lance::LanceNoteStore>> {
        match self {
            StorageHandle::Lightweight(store) => Some(store),
            _ => None,
        }
    }

    /// Get NoteStore trait object (if available)
    ///
    /// Returns `Some` for all storage modes. Each mode provides its own
    /// implementation:
    /// - Daemon: RPC wrapper via DaemonNoteStore
    /// - Lightweight: LanceDB-backed store
    /// - SQLite: Direct SQLite access
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use crucible_cli::factories::get_storage;
    /// use crucible_core::storage::NoteStore;
    ///
    /// let storage = get_storage(&config).await?;
    /// if let Some(note_store) = storage.note_store() {
    ///     let notes = note_store.list().await?;
    /// }
    /// ```
    pub fn note_store(&self) -> Option<Arc<dyn NoteStore>> {
        match self {
            StorageHandle::Daemon(c) => Some(Arc::new(DaemonNoteStore::new(Arc::clone(c)))),
            StorageHandle::Lightweight(store) => Some(Arc::clone(store) as Arc<dyn NoteStore>),
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(store) => Some(Arc::clone(store) as Arc<dyn NoteStore>),
        }
    }

    pub fn as_knowledge_repository(
        &self,
        _kiln_path: Option<&std::path::Path>,
    ) -> Option<Arc<dyn crucible_core::traits::KnowledgeRepository>> {
        match self {
            StorageHandle::Daemon(c) => {
                Some(Arc::clone(c) as Arc<dyn crucible_core::traits::KnowledgeRepository>)
            }
            StorageHandle::Lightweight(_) => None,
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(store) => {
                if let Some(kp) = _kiln_path {
                    Some(crucible_sqlite::create_knowledge_repository_with_kiln(
                        Arc::clone(store),
                        kp.to_path_buf(),
                    ))
                } else {
                    Some(crucible_sqlite::create_knowledge_repository(Arc::clone(
                        store,
                    )))
                }
            }
        }
    }
}

/// Get storage based on configuration mode
///
/// This is the preferred entry point for CLI commands that need storage access.
/// It automatically selects the storage backend based on the
/// `storage.mode` configuration.
///
/// # Storage Modes
///
/// - **Daemon**: Uses the cru-server daemon via Unix socket. Supports
///   multiple concurrent sessions.
///

///
/// # Example
///
/// ```rust,no_run
/// # use anyhow::Result;
/// # async fn example() -> Result<()> {
/// use crucible_cli::config::CliConfig;
/// use crucible_cli::factories;
///
/// let config = CliConfig::load(None, None, None)?;
/// let storage = factories::get_storage(&config).await?;
///
/// // Use for queries
/// let result = storage.query_raw("SELECT * FROM notes LIMIT 10").await?;
/// # Ok(())
/// # }
/// ```
pub async fn get_storage(config: &CliConfig) -> Result<StorageHandle> {
    let storage_config = config.storage.clone().unwrap_or_default();

    match storage_config.mode {
        StorageMode::Daemon => {
            info!("Using daemon storage mode");
            let client = DaemonClient::connect_or_start().await?;
            let kiln_path = config.kiln_path.clone();

            // Open the kiln in the daemon (required before any queries)
            client.kiln_open(&kiln_path).await?;

            Ok(StorageHandle::Daemon(Arc::new(DaemonStorageClient::new(
                Arc::new(client),
                kiln_path,
            ))))
        }
        StorageMode::Lightweight => {
            warn!(
                "Non-daemon storage mode detected. Chat knowledge features (Precognition, semantic search) \
                 require daemon mode. Set storage.mode = 'daemon' in crucible.toml. \
                 Non-daemon chat will be removed in a future version."
            );
            info!("Using lightweight storage mode (LanceDB + ripgrep)");
            let lance_path = config.kiln_path.join(".crucible").join("lance");
            let store = crucible_lance::LanceNoteStore::new(lance_path.to_string_lossy().as_ref())
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create LanceNoteStore: {}", e))?;
            Ok(StorageHandle::Lightweight(Arc::new(store)))
        }
        StorageMode::Sqlite => {
            warn!(
                "Non-daemon storage mode detected. Chat knowledge features (Precognition, semantic search) \
                 require daemon mode. Set storage.mode = 'daemon' in crucible.toml. \
                 Non-daemon chat will be removed in a future version."
            );
            #[cfg(feature = "storage-sqlite")]
            {
                info!("Using SQLite storage mode (experimental)");
                let sqlite_path = config
                    .kiln_path
                    .join(".crucible")
                    .join("crucible-sqlite.db");
                let sqlite_config =
                    crucible_sqlite::SqliteConfig::new(sqlite_path.to_string_lossy().as_ref());
                let pool = crucible_sqlite::SqlitePool::new(sqlite_config)
                    .map_err(|e| anyhow::anyhow!("Failed to create SQLite pool: {}", e))?;
                let store = crucible_sqlite::create_note_store(pool)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create SQLite NoteStore: {}", e))?;
                Ok(StorageHandle::Sqlite(Arc::new(store)))
            }
            #[cfg(not(feature = "storage-sqlite"))]
            {
                anyhow::bail!(
                    "SQLite storage mode requires the 'storage-sqlite' feature.\n\
                     Build with: cargo build --features storage-sqlite\n\
                     Or use storage.mode = \"embedded\" (SurrealDB) instead."
                )
            }
        }
    }
}
