//! Integration handler for Obsidian synchronization and API changes.

use crate::{
    error::{Error, Result},
    events::FileEvent,
    traits::EventHandler,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Handler for synchronizing with Obsidian and handling API changes.
pub struct ObsidianSyncHandler {
    /// Obsidian kiln configuration
    kiln_config: Arc<RwLock<Option<ObsidianKilnConfig>>>,
    /// Sync configuration
    sync_config: SyncConfig,
    /// Last sync state
    sync_state: Arc<RwLock<SyncState>>,
    /// Supported file types
    supported_extensions: Vec<String>,
}

/// Configuration for an Obsidian kiln.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsidianKilnConfig {
    /// Path to the kiln
    pub kiln_path: PathBuf,
    /// Obsidian installation path
    pub obsidian_path: Option<PathBuf>,
    /// API port for Obsidian
    pub api_port: Option<u16>,
    /// Whether to use local API or file-based sync
    pub use_local_api: bool,
    /// Authentication token for API
    pub api_token: Option<String>,
}

/// Configuration for synchronization behavior.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// Debounce delay for sync operations
    #[allow(dead_code)]
    sync_debounce: std::time::Duration,
    /// Whether to sync frontmatter changes
    #[allow(dead_code)]
    sync_frontmatter: bool,
    /// Whether to sync content changes
    #[allow(dead_code)]
    sync_content: bool,
    /// Whether to trigger Obsidian reindex
    trigger_reindex: bool,
    /// Maximum batch size for sync operations
    #[allow(dead_code)]
    max_batch_size: usize,
}

/// Current synchronization state.
#[derive(Debug, Clone, Default)]
struct SyncState {
    /// Files currently being processed
    #[allow(dead_code)]
    processing_files: HashMap<PathBuf, std::time::Instant>,
    /// Last full sync timestamp
    #[allow(dead_code)]
    last_full_sync: Option<chrono::DateTime<chrono::Utc>>,
    /// Pending changes to sync
    #[allow(dead_code)]
    pending_changes: Vec<FileEvent>,
    /// Sync statistics
    stats: SyncStats,
}

/// Synchronization statistics.
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    /// Total files synchronized
    pub total_synced: u64,
    /// Total errors encountered
    pub total_errors: u64,
    /// Last sync duration
    pub last_sync_duration: Option<std::time::Duration>,
    /// Files currently in queue
    pub queue_size: usize,
}

impl ObsidianSyncHandler {
    /// Create a new Obsidian sync handler.
    pub fn new() -> Result<Self> {
        Ok(Self {
            kiln_config: Arc::new(RwLock::new(None)),
            sync_config: SyncConfig::default(),
            sync_state: Arc::new(RwLock::new(SyncState::default())),
            supported_extensions: vec!["md".to_string()],
        })
    }

    /// Create a handler with kiln configuration.
    pub fn with_kiln_config(self, config: ObsidianKilnConfig) -> Result<Self> {
        let mut handler = Self::new()?;
        handler.kiln_config = Arc::new(RwLock::new(Some(config)));
        Ok(handler)
    }

    /// Set the sync configuration.
    pub fn with_sync_config(mut self, config: SyncConfig) -> Self {
        self.sync_config = config;
        self
    }

    /// Set supported file extensions.
    pub fn with_supported_extensions(mut self, extensions: Vec<String>) -> Self {
        self.supported_extensions = extensions;
        self
    }

    /// Get current sync statistics.
    pub async fn get_sync_stats(&self) -> SyncStats {
        let state = self.sync_state.read().await;
        state.stats.clone()
    }

    /// Update kiln configuration.
    pub async fn update_kiln_config(&self, config: ObsidianKilnConfig) {
        let mut kiln_config = self.kiln_config.write().await;
        *kiln_config = Some(config);
        info!("Obsidian kiln configuration updated");
    }

    async fn get_kiln_config(&self) -> Result<ObsidianKilnConfig> {
        let kiln_config = self.kiln_config.read().await;
        kiln_config
            .as_ref()
            .cloned()
            .ok_or_else(|| Error::Config("Obsidian kiln configuration not set".to_string()))
    }

    fn should_sync_file(&self, path: &PathBuf) -> bool {
        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                return self.supported_extensions.contains(&ext_str.to_lowercase());
            }
        }
        false
    }

    async fn sync_file_change(&self, event: &FileEvent) -> Result<()> {
        debug!("Syncing file change: {:?}", event);

        let kiln_config = self.get_kiln_config().await?;
        let kiln_path = &kiln_config.kiln_path;

        // Ensure file is within kiln
        if !event.path.starts_with(kiln_path) {
            debug!("File outside kiln, skipping: {}", event.path.display());
            return Ok(());
        }

        match &event.kind {
            crate::events::FileEventKind::Created | crate::events::FileEventKind::Modified => {
                self.handle_file_modification(&event.path, &kiln_config)
                    .await?;
            }
            crate::events::FileEventKind::Deleted => {
                self.handle_file_deletion(&event.path, &kiln_config)
                    .await?;
            }
            crate::events::FileEventKind::Moved { from, to } => {
                self.handle_file_move(from, to, &kiln_config).await?;
            }
            _ => {
                debug!("Unsupported event type for sync: {:?}", event.kind);
            }
        }

        Ok(())
    }

    async fn handle_file_modification(
        &self,
        path: &PathBuf,
        config: &ObsidianKilnConfig,
    ) -> Result<()> {
        debug!("Handling file modification: {}", path.display());

        if config.use_local_api {
            self.sync_via_api(path, config).await?;
        } else {
            self.sync_via_file_system(path, config).await?;
        }

        Ok(())
    }

    async fn sync_via_api(&self, path: &PathBuf, config: &ObsidianKilnConfig) -> Result<()> {
        // Implementation for Obsidian local API sync
        let api_port = config
            .api_port
            .ok_or_else(|| Error::Config("Obsidian API port not configured".to_string()))?;

        let relative_path = path
            .strip_prefix(&config.kiln_path)
            .map_err(|e| Error::Internal(format!("Failed to get relative path: {}", e)))?;

        let url = format!(
            "http://localhost:{}/api/kiln/{}",
            api_port,
            relative_path.display()
        );

        // TODO: Implement actual API call to notify Obsidian
        debug!("Would notify Obsidian API at: {}", url);

        Ok(())
    }

    async fn sync_via_file_system(
        &self,
        path: &PathBuf,
        _config: &ObsidianKilnConfig,
    ) -> Result<()> {
        // Implementation for file system based sync
        // This could involve updating Obsidian's internal cache files
        debug!("File system sync for: {}", path.display());

        // TODO: Implement file system sync logic
        // This might involve updating .obsidian/cache files or triggering reindex

        Ok(())
    }

    async fn handle_file_deletion(
        &self,
        path: &PathBuf,
        config: &ObsidianKilnConfig,
    ) -> Result<()> {
        debug!("Handling file deletion: {}", path.display());

        if config.use_local_api {
            // Notify API about deletion
            debug!(
                "Would notify Obsidian API about file deletion: {}",
                path.display()
            );
        } else {
            // Update file system cache
            debug!(
                "Would update file system cache for deletion: {}",
                path.display()
            );
        }

        Ok(())
    }

    async fn handle_file_move(
        &self,
        from: &PathBuf,
        to: &PathBuf,
        config: &ObsidianKilnConfig,
    ) -> Result<()> {
        debug!("Handling file move: {} -> {}", from.display(), to.display());

        if config.use_local_api {
            // Notify API about move
            debug!("Would notify Obsidian API about file move");
        } else {
            // Update file system cache
            debug!("Would update file system cache for move");
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn trigger_obsidian_reindex(&self, config: &ObsidianKilnConfig) -> Result<()> {
        if !self.sync_config.trigger_reindex {
            return Ok(());
        }

        debug!("Triggering Obsidian reindex");

        if config.use_local_api {
            let api_port = config
                .api_port
                .ok_or_else(|| Error::Config("Obsidian API port not configured".to_string()))?;

            let url = format!("http://localhost:{}/api/reindex", api_port);
            // TODO: Implement actual API call
            debug!("Would trigger Obsidian reindex via API: {}", url);
        } else {
            // TODO: Implement file system based reindex trigger
            debug!("Would trigger Obsidian reindex via file system");
        }

        Ok(())
    }
}

#[async_trait]
impl EventHandler for ObsidianSyncHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        debug!("Obsidian sync handler processing event: {:?}", event.kind);

        // Update sync statistics
        {
            let mut state = self.sync_state.write().await;
            state.stats.queue_size += 1;
        }

        let result = match event.kind {
            crate::events::FileEventKind::Created
            | crate::events::FileEventKind::Modified
            | crate::events::FileEventKind::Deleted
            | crate::events::FileEventKind::Moved { .. } => self.sync_file_change(&event).await,
            crate::events::FileEventKind::Batch(events) => {
                // Handle batch events
                for e in &events {
                    if let Err(err) = self.sync_file_change(e).await {
                        error!("Error in batch sync for {}: {}", e.path.display(), err);
                    }
                }
                Ok(())
            }
            crate::events::FileEventKind::Unknown(_) => {
                debug!("Unknown event type, skipping");
                Ok(())
            }
        };

        // Update sync statistics
        {
            let mut state = self.sync_state.write().await;
            state.stats.queue_size = state.stats.queue_size.saturating_sub(1);

            match result {
                Ok(()) => {
                    state.stats.total_synced += 1;
                }
                Err(ref e) => {
                    state.stats.total_errors += 1;
                    error!("Obsidian sync error: {}", e);
                }
            }
        }

        result
    }

    fn name(&self) -> &'static str {
        "obsidian_sync"
    }

    fn priority(&self) -> u32 {
        120 // Medium-high priority for Obsidian sync
    }

    fn can_handle(&self, event: &FileEvent) -> bool {
        !event.is_dir && self.should_sync_file(&event.path)
    }
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            sync_debounce: std::time::Duration::from_millis(300),
            sync_frontmatter: true,
            sync_content: true,
            trigger_reindex: false,
            max_batch_size: 50,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileEvent, FileEventKind};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_obsidian_sync_handler() {
        let handler = ObsidianSyncHandler::new().unwrap();

        assert_eq!(handler.name(), "obsidian_sync");
        assert_eq!(handler.priority(), 120);

        let md_file = PathBuf::from("test.md");
        let event = FileEvent::new(FileEventKind::Modified, md_file);

        assert!(handler.can_handle(&event));

        let other_file = PathBuf::from("test.txt");
        let other_event = FileEvent::new(FileEventKind::Modified, other_file);

        assert!(!handler.can_handle(&other_event));
    }

    #[tokio::test]
    async fn test_sync_configuration() {
        let config = SyncConfig::default();
        assert!(config.sync_frontmatter);
        assert!(config.sync_content);
        assert!(!config.trigger_reindex);
        assert_eq!(config.max_batch_size, 50);
    }

    #[tokio::test]
    async fn test_supported_extensions() {
        let handler = ObsidianSyncHandler::new().unwrap();

        assert!(handler.should_sync_file(&PathBuf::from("test.md")));
        assert!(!handler.should_sync_file(&PathBuf::from("test.txt")));
        assert!(!handler.should_sync_file(&PathBuf::from("test")));
    }
}
