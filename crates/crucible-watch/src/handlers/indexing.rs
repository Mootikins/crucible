//! Integration handler for automatic embedding database indexing.

use crate::{events::FileEvent, traits::EventHandler, error::{Error, Result}};
use async_trait::async_trait;
use crucible_mcp::database::EmbeddingDatabase;
use crucible_mcp::embeddings::{create_provider, EmbeddingConfig};
use crucible_mcp::types::EmbeddingMetadata;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Handler for automatically indexing files when they change.
pub struct IndexingHandler {
    database: Arc<RwLock<Option<EmbeddingDatabase>>>,
    embedding_config: Option<EmbeddingConfig>,
    supported_extensions: Vec<String>,
    index_debounce: std::time::Duration,
}

impl IndexingHandler {
    /// Create a new indexing handler.
    pub fn new() -> Result<Self> {
        Ok(Self {
            database: Arc::new(RwLock::new(None)),
            embedding_config: None,
            supported_extensions: vec![
                "md".to_string(),
                "txt".to_string(),
                "rst".to_string(),
                "adoc".to_string(),
            ],
            index_debounce: std::time::Duration::from_millis(500),
        })
    }

    /// Create an indexing handler with database and embedding configuration.
    pub fn with_config(
        database: EmbeddingDatabase,
        embedding_config: EmbeddingConfig,
    ) -> Result<Self> {
        let mut handler = Self::new()?;
        handler.database = Arc::new(RwLock::new(Some(database)));
        handler.embedding_config = Some(embedding_config);
        Ok(handler)
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
    pub async fn initialize_database(&self, db_path: &str) -> Result<()> {
        let db = EmbeddingDatabase::new(db_path).await?;
        *self.database.write().await = Some(db);
        info!("Indexing handler database initialized");
        Ok(())
    }

    /// Set the embedding configuration.
    pub fn set_embedding_config(&mut self, config: EmbeddingConfig) {
        self.embedding_config = Some(config);
    }

    async fn get_database(&self) -> Result<EmbeddingDatabase> {
        let db_guard = self.database.read().await;
        db_guard.as_ref()
            .cloned()
            .ok_or_else(|| Error::Config("Database not initialized".to_string()))
    }

    async fn get_embedding_provider(&self) -> Result<Arc<dyn crucible_mcp::embeddings::EmbeddingProvider>> {
        let config = self.embedding_config.as_ref()
            .ok_or_else(|| Error::Config("Embedding configuration not set".to_string()))?;
        create_provider(config.clone()).await
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
        debug!("Indexing file: {}", path.display());

        // Skip if not a supported file type
        if !self.should_index_file(path) {
            debug!("Skipping unsupported file: {}", path.display());
            return Ok(());
        }

        // Read file content
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| Error::Io(e))?;

        if content.trim().is_empty() {
            debug!("Skipping empty file: {}", path.display());
            return Ok(());
        }

        // Get database and provider
        let db = self.get_database().await?;
        let provider = self.get_embedding_provider().await?;

        // Generate embedding
        let embedding_response = provider.embed(&content)
            .await
            .map_err(|e| Error::Handler(format!("Embedding generation failed: {}", e)))?;

        // Create metadata
        let path_str = path.to_string_lossy().to_string();
        let now = chrono::Utc::now();
        let folder = path.parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        let title = path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());

        let metadata = EmbeddingMetadata {
            file_path: path_str.clone(),
            title,
            tags: Vec::new(),
            folder,
            properties: std::collections::HashMap::new(),
            created_at: now,
            updated_at: now,
        };

        // Store embedding
        db.store_embedding(&path_str, &content, &embedding_response.embedding, &metadata)
            .await
            .map_err(|e| Error::Handler(format!("Failed to store embedding: {}", e)))?;

        info!("Successfully indexed file: {}", path.display());
        Ok(())
    }

    async fn remove_file_index(&self, path: &PathBuf) -> Result<()> {
        debug!("Removing index for file: {}", path.display());

        let db = self.get_database().await?;
        let path_str = path.to_string_lossy().to_string();

        db.remove_embedding(&path_str)
            .await
            .map_err(|e| Error::Handler(format!("Failed to remove embedding: {}", e)))?;

        info!("Successfully removed index for file: {}", path.display());
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