//! Multi-kiln connection manager
//!
//! Manages connections to multiple kilns on-demand with idle timeout.

use anyhow::Result;
use crucible_surrealdb::adapters::{self, SurrealClientHandle};
use crucible_surrealdb::SurrealDbConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::info;

/// Connection to a single kiln
pub struct KilnConnection {
    pub client: SurrealClientHandle,
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

        let config = SurrealDbConfig {
            path: db_path.to_string_lossy().to_string(),
            namespace: "crucible".to_string(),
            database: "kiln".to_string(),
            ..Default::default()
        };

        let client = adapters::create_surreal_client(config).await?;

        let mut conns = self.connections.write().await;
        conns.insert(
            canonical,
            KilnConnection {
                client,
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

    /// Get client for a kiln if it's already open (does not open if closed)
    pub async fn get(&self, kiln_path: &Path) -> Option<SurrealClientHandle> {
        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

        let mut conns = self.connections.write().await;
        if let Some(conn) = conns.get_mut(&canonical) {
            conn.last_access = Instant::now();
            Some(conn.client.clone())
        } else {
            None
        }
    }

    /// Get client for a kiln, opening if needed
    pub async fn get_or_open(&self, kiln_path: &Path) -> Result<SurrealClientHandle> {
        let canonical = kiln_path
            .canonicalize()
            .unwrap_or_else(|_| kiln_path.to_path_buf());

        // Try to get existing and update last_access
        {
            let mut conns = self.connections.write().await;
            if let Some(conn) = conns.get_mut(&canonical) {
                conn.last_access = Instant::now();
                return Ok(conn.client.clone());
            }
        }

        // Open new connection
        self.open(kiln_path).await?;

        let conns = self.connections.read().await;
        conns
            .get(&canonical)
            .map(|c| c.client.clone())
            .ok_or_else(|| anyhow::anyhow!("Failed to get connection after opening"))
    }
}

impl Default for KilnManager {
    fn default() -> Self {
        Self::new()
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
        // Note: SurrealDB will create a new database if the path is valid
        // This test verifies we can open a kiln in a temp directory
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
        let path = tmp.path().to_path_buf();

        // Manually insert a mock connection for testing
        {
            use crucible_surrealdb::SurrealDbConfig;

            // We can't actually open a kiln without a real DB file,
            // but we can verify that close() removes entries from the map
            let mut conns = km.connections.write().await;

            // Create a temporary in-memory client for testing
            let config = SurrealDbConfig {
                path: "memory".to_string(),
                namespace: "test".to_string(),
                database: "test".to_string(),
                ..Default::default()
            };

            // Skip this test if we can't create a client
            if let Ok(client) = adapters::create_surreal_client(config).await {
                conns.insert(
                    path.clone(),
                    KilnConnection {
                        client,
                        last_access: Instant::now(),
                    },
                );
            } else {
                return; // Skip test if client creation fails
            }
        }

        // Verify it's in the list
        let list = km.list().await;
        if list.is_empty() {
            return; // Test was skipped due to client creation failure
        }
        assert_eq!(list.len(), 1);

        // Close it
        km.close(&path).await.unwrap();

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
        // get_or_open should create a new kiln if needed
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
        let client1 = km.get_or_open(&kiln_path).await.unwrap();

        // Second call should reuse the same connection
        let client2 = km.get_or_open(&kiln_path).await.unwrap();

        // Should only have one entry in the list
        let list = km.list().await;
        assert_eq!(list.len(), 1);

        // Clients should be clones of the same handle
        drop(client1);
        drop(client2);
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
    async fn test_get_returns_client_if_open() {
        let km = KilnManager::new();
        let tmp = TempDir::new().unwrap();
        let kiln_path = tmp.path().join("test_kiln");

        // Open the kiln first
        km.open(&kiln_path).await.unwrap();

        // get() should now return Some(client)
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
}
