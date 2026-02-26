//! Storage factory - creates storage implementations
//!
//! Daemon-only: all storage access goes through the daemon via RPC.

use crate::config::CliConfig;
use anyhow::Result;
use crucible_core::storage::NoteStore;
use crucible_core::traits::StorageClient;
use crucible_rpc::{DaemonClient, DaemonNoteStore, DaemonStorageClient};
use std::sync::Arc;
use tracing::info;

/// Handle for daemon-backed storage.
///
/// Wraps a `DaemonStorageClient` — the daemon is the only storage backend.
#[derive(Clone)]
pub enum StorageHandle {
    Daemon(Arc<DaemonStorageClient>),
}

impl StorageHandle {
    /// Execute a raw query and return JSON
    pub async fn query_raw(&self, sql: &str) -> Result<serde_json::Value> {
        match self {
            Self::Daemon(client) => client.query_raw(sql).await,
        }
    }

    /// Get the inner `DaemonStorageClient`.
    pub fn as_daemon_client(&self) -> &Arc<DaemonStorageClient> {
        match self {
            Self::Daemon(client) => client,
        }
    }

    /// List notes in the kiln.
    pub async fn list_notes(
        &self,
        path_filter: Option<&str>,
    ) -> Result<Vec<crucible_core::traits::NoteInfo>> {
        use crucible_core::traits::KnowledgeRepository;

        let repo = match self {
            Self::Daemon(client) => Arc::clone(client) as Arc<dyn KnowledgeRepository>,
        };
        repo.list_notes(path_filter)
            .await
            .map_err(|e| anyhow::anyhow!("list_notes failed: {}", e))
    }

    /// Get NoteStore trait object.
    pub fn note_store(&self) -> Arc<dyn NoteStore> {
        match self {
            Self::Daemon(client) => Arc::new(DaemonNoteStore::new(Arc::clone(client))),
        }
    }

    pub fn as_knowledge_repository(&self) -> Arc<dyn crucible_core::traits::KnowledgeRepository> {
        match self {
            Self::Daemon(client) => {
                Arc::clone(client) as Arc<dyn crucible_core::traits::KnowledgeRepository>
            }
        }
    }
}

/// Get daemon-backed storage.
///
/// Connects to the daemon (auto-starting if needed), opens the kiln,
/// and returns a `StorageHandle` for queries.
pub async fn get_storage(config: &CliConfig) -> Result<StorageHandle> {
    info!("Using daemon storage mode");
    let client = DaemonClient::connect_or_start().await?;
    let kiln_path = config.kiln_path.clone();

    // Open the kiln in the daemon (required before any queries).
    // process=true ensures files are processed on open, replacing the old
    // separate process_files_with_change_detection call.
    client
        .kiln_open_with_options(&kiln_path, true, false)
        .await?;

    let client = Arc::new(client);
    Ok(StorageHandle::Daemon(Arc::new(DaemonStorageClient::new(
        client, kiln_path,
    ))))
}
