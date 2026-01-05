//! Multi-kiln connection manager
//!
//! Manages connections to multiple kilns on-demand with idle timeout.
//! Supports both SQLite (default) and SurrealDB backends via feature flags.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::info;

// Backend-specific imports
#[cfg(feature = "storage-sqlite")]
use crucible_sqlite::{adapters as sqlite_adapters, SqliteClientHandle, SqliteConfig};

#[cfg(feature = "storage-surrealdb")]
use crucible_surrealdb::{adapters as surreal_adapters, SurrealClientHandle, SurrealDbConfig};

// ===========================================================================
// Backend Abstraction
// ===========================================================================

/// Storage backend handle that wraps either SQLite or SurrealDB client
#[derive(Clone)]
pub enum StorageHandle {
    #[cfg(feature = "storage-sqlite")]
    Sqlite(SqliteClientHandle),

    #[cfg(feature = "storage-surrealdb")]
    Surreal(SurrealClientHandle),
}

impl StorageHandle {
    /// Execute a query against the storage backend
    pub async fn query(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<crucible_core::database::QueryResult> {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(client) => client.query(sql, params).await,

            #[cfg(feature = "storage-surrealdb")]
            StorageHandle::Surreal(client) => {
                let result = client.inner().query(sql, params).await?;
                Ok(result)
            }
        }
    }

    /// Get the backend name for logging
    pub fn backend_name(&self) -> &'static str {
        match self {
            #[cfg(feature = "storage-sqlite")]
            StorageHandle::Sqlite(_) => "sqlite",

            #[cfg(feature = "storage-surrealdb")]
            StorageHandle::Surreal(_) => "surrealdb",
        }
    }
}

// ===========================================================================
// KilnConnection and KilnManager
// ===========================================================================

/// Connection to a single kiln
pub struct KilnConnection {
    pub handle: StorageHandle,
    pub last_access: Instant,
}

/// Manages connections to multiple kilns
pub struct KilnManager {
    connections: RwLock<HashMap<PathBuf, KilnConnection>>,
}

impl KilnManager {
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
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

        let db_path = canonical.join(".crucible").join("kiln.db");
        info!("Opening kiln at {:?}", db_path);

        let handle = create_storage_handle(&db_path).await?;
        info!(
            "Kiln opened with {} backend at {:?}",
            handle.backend_name(),
            db_path
        );

        let mut conns = self.connections.write().await;
        conns.insert(
            canonical,
            KilnConnection {
                handle,
                last_access: Instant::now(),
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
        if conns.remove(&canonical).is_some() {
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
}

impl Default for KilnManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Backend Factory
// ===========================================================================

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
    async fn test_query_works() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        let handle = km.get_or_open(&kiln_path).await.unwrap();

        // Basic query
        let result = handle.query("SELECT 1 + 1 AS result", &[]).await;
        assert!(result.is_ok());
    }
}
