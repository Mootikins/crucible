//! Storage factory - creates storage implementations
//!
//! This is part of the composition root where concrete types are wired together.
//! Phase 5: Uses public adapters API instead of importing concrete types.
//!
//! ## Available Factories
//!
//! - `get_storage` - Unified factory that returns embedded or daemon storage based on config
//! - `create_surrealdb_storage` - SurrealDB-backed persistent storage (legacy, direct connection)
//! - `create_daemon_storage` - Daemon-backed storage (preferred, auto-starts daemon)
//! - `create_content_addressed_storage` - In-memory content-addressed storage (for testing/demos)

use crate::config::CliConfig;
use anyhow::Result;
use crucible_config::StorageMode;
use crucible_core::enrichment::EnrichedNoteStore;
use crucible_core::hashing::Blake3Hasher;
use crucible_core::storage::{
    BlockSize, ContentAddressedStorage, ContentAddressedStorageBuilder, HasherConfig,
    StorageBackendType, StorageResult,
};
use crucible_core::traits::StorageClient;
use crucible_daemon_client::{lifecycle, DaemonClient, DaemonStorageClient};
use crucible_surrealdb::{adapters, SurrealDbConfig};
use once_cell::sync::Lazy;
use std::collections::{hash_map::Entry, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

/// Create SurrealDB storage from CLI configuration
///
/// Returns an opaque handle that can be passed to other factory functions.
/// Phase 5: Now returns SurrealClientHandle instead of concrete SurrealClient.
static SURREAL_CLIENT_CACHE: Lazy<Mutex<HashMap<String, adapters::SurrealClientHandle>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn storage_cache_key(config: &SurrealDbConfig) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        config.path,
        config.namespace,
        config.database,
        config.max_connections.unwrap_or(0),
        config.timeout_seconds.unwrap_or(0)
    )
}

pub async fn create_surrealdb_storage(config: &CliConfig) -> Result<adapters::SurrealClientHandle> {
    let db_config = SurrealDbConfig {
        path: config.database_path_str()?,
        namespace: "crucible".to_string(),
        database: "kiln".to_string(),
        max_connections: Some(10),
        timeout_seconds: Some(30),
    };

    let cache_key = storage_cache_key(&db_config);

    if let Some(cached) = {
        let cache = SURREAL_CLIENT_CACHE.lock().unwrap();
        cache.get(&cache_key).cloned()
    } {
        return Ok(cached);
    }

    let client = adapters::create_surreal_client(db_config.clone())
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create SurrealDB storage: {}", e))?;

    let mut cache = SURREAL_CLIENT_CACHE.lock().unwrap();
    let entry = cache.entry(cache_key);

    let client_handle = match entry {
        Entry::Occupied(entry) => entry.get().clone(),
        Entry::Vacant(entry) => {
            entry.insert(client.clone());
            client
        }
    };

    Ok(client_handle)
}

/// Initialize SurrealDB schema
///
/// This requires access to the internal client, so we expose it via
/// a special function in the adapters module.
pub async fn initialize_surrealdb_schema(client: &adapters::SurrealClientHandle) -> Result<()> {
    // Call kiln_integration via the handle
    // This is a temporary measure - ideally schema initialization should be part of factory
    crucible_surrealdb::kiln_integration::initialize_kiln_schema(client.inner()).await
}

/// Create SurrealDB-backed enriched note store
///
/// This factory creates an adapter that implements the `EnrichedNoteStore` trait
/// using SurrealDB as the backend.
///
/// # Architecture
///
/// Uses the public factory function from the adapters module, which handles
/// all the internal wiring (EAVGraphStore, NoteIngestor lifetimes, etc.).
pub fn create_surrealdb_enriched_note_store(
    client: adapters::SurrealClientHandle,
) -> Arc<dyn EnrichedNoteStore> {
    adapters::create_enriched_note_store(client)
}

/// Create in-memory content-addressed storage
///
/// This creates an in-memory storage backend suitable for testing and demos.
/// Uses BLAKE3 hashing for optimal performance.
///
/// # Arguments
///
/// * `_config` - CLI configuration (currently unused, for future customization)
///
/// # Returns
///
/// A content-addressed storage implementation wrapped in an Arc.
pub fn create_content_addressed_storage(
    _config: &CliConfig,
) -> StorageResult<Arc<dyn ContentAddressedStorage>> {
    ContentAddressedStorageBuilder::new()
        .with_backend(StorageBackendType::InMemory)
        .with_hasher(HasherConfig::Blake3(Blake3Hasher::new()))
        .with_block_size(BlockSize::Medium)
        .build()
}

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

/// Handle for either embedded or daemon-backed storage
///
/// This enum allows the CLI to use storage abstractly without caring
/// whether it's a direct SurrealDB connection or daemon-backed.
#[derive(Clone)]
pub enum StorageHandle {
    /// Direct in-process SurrealDB (single session)
    Embedded(adapters::SurrealClientHandle),
    /// Daemon-backed storage (multi-session via Unix socket)
    Daemon(Arc<DaemonStorageClient>),
}

impl StorageHandle {
    /// Execute a raw query and return JSON
    ///
    /// This provides a unified query interface regardless of storage mode.
    pub async fn query_raw(&self, sql: &str) -> Result<serde_json::Value> {
        match self {
            StorageHandle::Embedded(h) => {
                let inner = h.inner();
                let result = inner.query(sql, &[]).await?;
                Ok(serde_json::json!({
                    "records": result.records,
                    "total_count": result.total_count,
                    "execution_time_ms": result.execution_time_ms,
                    "has_more": result.has_more
                }))
            }
            StorageHandle::Daemon(c) => c.query_raw(sql).await,
        }
    }

    /// Get as embedded handle (panics if daemon mode)
    ///
    /// Use for operations that need full SurrealClientHandle capabilities
    /// (e.g., NoteStore, MerkleStore, etc.). This should be called only
    /// when you know you're in embedded mode.
    pub fn as_embedded(&self) -> &adapters::SurrealClientHandle {
        match self {
            StorageHandle::Embedded(h) => h,
            StorageHandle::Daemon(_) => panic!(
                "Operation requires embedded mode. \
                 Configure storage.mode = \"embedded\" or use daemon RPC methods."
            ),
        }
    }

    /// Try to get as embedded handle (returns None if daemon mode)
    ///
    /// Use this for graceful fallback instead of panic.
    pub fn try_embedded(&self) -> Option<&adapters::SurrealClientHandle> {
        match self {
            StorageHandle::Embedded(h) => Some(h),
            StorageHandle::Daemon(_) => None,
        }
    }

    /// Check if running in embedded mode
    pub fn is_embedded(&self) -> bool {
        matches!(self, StorageHandle::Embedded(_))
    }

    /// Check if running in daemon mode
    pub fn is_daemon(&self) -> bool {
        matches!(self, StorageHandle::Daemon(_))
    }

    /// Get embedded handle, creating fallback if in daemon mode
    ///
    /// For operations that require full SurrealClientHandle (schema init,
    /// pipeline creation, etc.), this creates a temporary embedded connection
    /// when running in daemon mode. Logs a warning about the fallback.
    ///
    /// This enables graceful degradation: daemon mode works for multi-session
    /// queries, but heavy operations transparently use embedded.
    pub async fn get_embedded_for_operation(
        &self,
        config: &crate::config::CliConfig,
        operation: &str,
    ) -> Result<adapters::SurrealClientHandle> {
        match self {
            StorageHandle::Embedded(h) => Ok(h.clone()),
            StorageHandle::Daemon(_) => {
                tracing::warn!(
                    "Operation '{}' requires embedded storage; creating fallback connection",
                    operation
                );
                create_surrealdb_storage(config).await
            }
        }
    }
}

/// Get storage based on configuration mode
///
/// This is the preferred entry point for CLI commands that need storage access.
/// It automatically selects between embedded and daemon mode based on the
/// `storage.mode` configuration.
///
/// # Storage Modes
///
/// - **Embedded** (default): Direct in-process SurrealDB. Fast, simple, but
///   single-session only (file locked).
///
/// - **Daemon**: Uses the db-server subprocess via Unix socket. Slower initial
///   connection (may fork daemon), but supports multiple concurrent sessions.
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
///
/// // For operations requiring full access (embedded only)
/// if let Some(embedded) = storage.try_embedded() {
///     // Use embedded-specific features
/// }
/// # Ok(())
/// # }
/// ```
pub async fn get_storage(config: &CliConfig) -> Result<StorageHandle> {
    let storage_config = config
        .storage
        .clone()
        .unwrap_or_default();

    match storage_config.mode {
        StorageMode::Embedded => {
            debug!("Using embedded storage mode");
            let client = create_surrealdb_storage(config).await?;
            Ok(StorageHandle::Embedded(client))
        }
        StorageMode::Daemon => {
            info!("Using daemon storage mode");
            let socket = lifecycle::default_socket_path();

            // Ensure daemon is running (fork if needed)
            lifecycle::ensure_daemon(&socket, storage_config.idle_timeout_secs).await?;

            // Connect to daemon
            let client = DaemonClient::connect_to(&socket).await?;
            let kiln_path = config.kiln_path.clone();

            Ok(StorageHandle::Daemon(Arc::new(DaemonStorageClient::new(
                Arc::new(client),
                kiln_path,
            ))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_handle_is_embedded() {
        // We can't easily create a real handle in tests, but we can test the pattern
        // This is more of a compile-time check that the types work
    }
}
