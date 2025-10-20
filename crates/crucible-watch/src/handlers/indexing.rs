//! Integration handler for automatic embedding database indexing.
//! NOTE: MCP functionality has been removed - this handler is now a stub.

use crate::{events::FileEvent, traits::EventHandler, error::{Error, Result}};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Handler for automatically indexing files when they changes.
/// NOTE: This is now a stub since MCP functionality has been removed.
pub struct IndexingHandler {
    supported_extensions: Vec<String>,
    index_debounce: std::time::Duration,
}

impl IndexingHandler {
    /// Create a new indexing handler.
    pub fn new() -> Result<Self> {
        warn!("IndexingHandler created - MCP functionality has been disabled");
        Ok(Self {
            supported_extensions: vec![
                "md".to_string(),
                "txt".to_string(),
                "rst".to_string(),
                "adoc".to_string(),
            ],
            index_debounce: std::time::Duration::from_millis(500),
        })
    }

    /// Set the supported file extensions.
    pub fn with_supported_extensions(mut self, extensions: Vec<String>) -> Self {
        self.supported_extensions = extensions;
        self
    }

    /// Set the debounce delay for indexing operations.
    pub fn with_debounce(mut self, debounce: std::time::Duration) -> Self {
        self.index_debounce = debounce;
        self
    }

    /// Initialize the database connection.
    pub async fn initialize_database(&self, _db_path: &str) -> Result<()> {
        warn!("IndexingHandler.initialize_database called - MCP functionality disabled");
        Ok(())
    }

    /// Set the embedding configuration.
    pub fn set_embedding_config(&mut self, _config: ()) {
        warn!("IndexingHandler.set_embedding_config called - MCP functionality disabled");
    }

    fn should_index_file(&self, path: &PathBuf) -> bool {
        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                return self.supported_extensions.contains(&ext_str.to_lowercase());
            }
        }
        false
    }

    async fn index_file(&self, path: &PathBuf) -> Result<()> {
        debug!("Indexing file: {} (STUB - MCP functionality disabled)", path.display());

        // Skip if not a supported file type
        if !self.should_index_file(path) {
            debug!("Skipping unsupported file: {}", path.display());
            return Ok(());
        }

        warn!("File indexing disabled - MCP integration removed: {}", path.display());
        Ok(())
    }

    async fn remove_file_index(&self, path: &PathBuf) -> Result<()> {
        debug!("Removing index for file: {} (STUB - MCP functionality disabled)", path.display());
        warn!("File index removal disabled - MCP integration removed: {}", path.display());
        Ok(())
    }
}

#[async_trait]
impl EventHandler for IndexingHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        debug!("Indexing handler processing event: {:?}", event.kind);

        match event.kind {
            crate::events::FileEventKind::Created | crate::events::FileEventKind::Modified => {
                if let Err(e) = self.index_file(&event.path).await {
                    error!("Failed to index file {}: {}", event.path.display(), e);
                    return Err(e);
                }
            }
            crate::events::FileEventKind::Deleted => {
                if let Err(e) = self.remove_file_index(&event.path).await {
                    error!("Failed to remove index for file {}: {}", event.path.display(), e);
                    return Err(e);
                }
            }
            crate::events::FileEventKind::Moved { from, to } => {
                // Remove old index and create new one
                if let Err(e) = self.remove_file_index(&from).await {
                    warn!("Failed to remove index for moved file {}: {}", from.display(), e);
                }
                if let Err(e) = self.index_file(&to).await {
                    error!("Failed to index moved file {}: {}", to.display(), e);
                    return Err(e);
                }
            }
            crate::events::FileEventKind::Batch(_) => {
                warn!("Batch events not yet supported by indexing handler");
            }
            crate::events::FileEventKind::Unknown(_) => {
                debug!("Unknown event type, skipping");
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        "indexing"
    }

    fn priority(&self) -> u32 {
        200 // High priority for indexing
    }

    fn can_handle(&self, event: &FileEvent) -> bool {
        // Handle all file events, but will filter internally
        !event.is_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{FileEvent, FileEventKind};
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_supported_extensions() {
        let handler = IndexingHandler::new().unwrap();

        assert!(handler.should_index_file(&PathBuf::from("test.md")));
        assert!(handler.should_index_file(&PathBuf::from("test.txt")));
        assert!(!handler.should_index_file(&PathBuf::from("test.exe")));
        assert!(!handler.should_index_file(&PathBuf::from("test")));
    }

    #[tokio::test]
    async fn test_handler_capabilities() {
        let handler = IndexingHandler::new().unwrap();

        assert_eq!(handler.name(), "indexing");
        assert_eq!(handler.priority(), 200);

        let file_event = FileEvent::new(FileEventKind::Created, PathBuf::from("test.md"));
        assert!(handler.can_handle(&file_event));

        let dir_event = FileEvent::new(FileEventKind::Created, PathBuf::from("test"));
        dir_event.is_dir = true;
        assert!(!handler.can_handle(&dir_event));
    }
}