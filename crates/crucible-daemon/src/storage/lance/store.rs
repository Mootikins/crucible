//! LanceDB store management

use anyhow::{Context, Result};
use lancedb::{connect, Connection};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// LanceDB connection wrapper
pub struct LanceStore {
    connection: Arc<RwLock<Connection>>,
    path: String,
}

impl LanceStore {
    /// Open or create a LanceDB store at the given path
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path_str = path.as_ref().to_string_lossy().to_string();

        // Ensure directory exists
        if let Some(parent) = path.as_ref().parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let connection = connect(&path_str)
            .execute()
            .await
            .context("Failed to connect to LanceDB")?;

        Ok(Self {
            connection: Arc::new(RwLock::new(connection)),
            path: path_str,
        })
    }

    /// Get the database path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get a reference to the connection
    pub async fn connection(&self) -> tokio::sync::RwLockReadGuard<'_, Connection> {
        self.connection.read().await
    }

    /// Get a mutable reference to the connection
    pub async fn connection_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, Connection> {
        self.connection.write().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_lance_store_open_creates_db() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("lance");

        let store = LanceStore::open(&db_path).await.unwrap();
        assert_eq!(store.path(), db_path.to_string_lossy());
    }

    #[tokio::test]
    async fn test_lance_store_reopen_existing() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("lance");

        // Open and close
        let _store = LanceStore::open(&db_path).await.unwrap();
        drop(_store);

        // Reopen
        let store = LanceStore::open(&db_path).await.unwrap();
        assert_eq!(store.path(), db_path.to_string_lossy());
    }
}
