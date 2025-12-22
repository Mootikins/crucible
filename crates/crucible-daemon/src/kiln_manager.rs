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
        let canonical = kiln_path.canonicalize().unwrap_or_else(|_| kiln_path.to_path_buf());

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
        conns.insert(canonical, KilnConnection {
            client,
            last_access: Instant::now(),
        });

        Ok(())
    }

    /// Close a kiln connection
    pub async fn close(&self, kiln_path: &Path) -> Result<()> {
        let canonical = kiln_path.canonicalize().unwrap_or_else(|_| kiln_path.to_path_buf());
        let mut conns = self.connections.write().await;
        if conns.remove(&canonical).is_some() {
            info!("Closed kiln at {:?}", canonical);
        }
        Ok(())
    }

    /// List all open kilns
    pub async fn list(&self) -> Vec<(PathBuf, Instant)> {
        let conns = self.connections.read().await;
        conns.iter()
            .map(|(path, conn)| (path.clone(), conn.last_access))
            .collect()
    }

    /// Get client for a kiln, opening if needed
    pub async fn get_or_open(&self, kiln_path: &Path) -> Result<SurrealClientHandle> {
        let canonical = kiln_path.canonicalize().unwrap_or_else(|_| kiln_path.to_path_buf());

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
        conns.get(&canonical)
            .map(|c| c.client.clone())
            .ok_or_else(|| anyhow::anyhow!("Failed to get connection after opening"))
    }
}

impl Default for KilnManager {
    fn default() -> Self {
        Self::new()
    }
}
