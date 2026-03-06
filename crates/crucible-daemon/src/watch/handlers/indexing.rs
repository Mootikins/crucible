//! Integration handler for automatic file parsing and database indexing.
//! Integrates PulldownParser with file watching for real-time note processing.
//!
//! This handler emits `SessionEvent` variants (FileChanged, FileDeleted, FileMoved)
//! to the event bus. Embedding generation is handled downstream by `EmbeddingHandler`
//! in `crucible-daemon::enrichment` which listens for `NoteParsed` events.

#![allow(clippy::ptr_arg)]

use crate::watch::{
    error::{Error, Result},
    events::FileEvent,
    traits::EventHandler,
};
use async_trait::async_trait;
use crucible_core::events::{EventEmitter, InternalSessionEvent, NoOpEmitter, SessionEvent};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub struct IndexingHandler {
    supported_extensions: Vec<String>,
    index_debounce: std::time::Duration,
    emitter: Arc<dyn EventEmitter<Event = SessionEvent>>,
}

impl IndexingHandler {
    pub fn new() -> Result<Self> {
        Self::with_emitter(Arc::new(NoOpEmitter::new()))
    }

    pub fn with_emitter(emitter: Arc<dyn EventEmitter<Event = SessionEvent>>) -> Result<Self> {
        info!("IndexingHandler created");
        Ok(Self {
            supported_extensions: vec![
                "md".to_string(),
                "txt".to_string(),
                "rst".to_string(),
                "adoc".to_string(),
            ],
            index_debounce: std::time::Duration::from_millis(500),
            emitter,
        })
    }

    pub fn set_emitter(&mut self, emitter: Arc<dyn EventEmitter<Event = SessionEvent>>) {
        self.emitter = emitter;
    }

    pub fn emitter(&self) -> &Arc<dyn EventEmitter<Event = SessionEvent>> {
        &self.emitter
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

    /// Initialize the database connection (Phase 4 placeholder).
    pub async fn initialize_database(&self, _db_path: &str) -> Result<()> {
        info!("Database initialization will be implemented in Phase 4");
        // Phase 4: Initialize storage connection here
        Ok(())
    }

    fn should_index_file(&self, path: &PathBuf) -> bool {
        if let Some(ext) = path.extension() {
            if let Some(ext_str) = ext.to_str() {
                return self.supported_extensions.contains(&ext_str.to_lowercase());
            }
        }
        false
    }

    async fn index_file(
        &self,
        path: &PathBuf,
        _event_kind: crate::watch::events::FileEventKind,
    ) -> Result<()> {
        debug!("Indexing file: {}", path.display());

        // Skip if not a supported file type
        if !self.should_index_file(path) {
            debug!("Skipping unsupported file: {}", path.display());
            return Ok(());
        }

        // Validate file exists and is accessible
        if !path.exists() {
            warn!("File does not exist, skipping indexing: {}", path.display());
            return Ok(());
        }

        // Get file metadata for progress reporting
        let file_metadata = match tokio::fs::metadata(path).await {
            Ok(metadata) => metadata,
            Err(e) => {
                error!("Failed to read metadata for file {}: {}", path.display(), e);
                return Err(Error::Io(e));
            }
        };

        let file_size = file_metadata.len();
        debug!(
            "Starting indexing for file: {} ({} bytes)",
            path.display(),
            file_size
        );

        // Note: Parsing is now handled by ParserHandler which listens for FileChanged events.
        // The IndexingHandler emits FileChanged events (via emit_session_event), and the
        // ParserHandler picks those up and emits NoteParsed events with the parsed content.
        // This method now just validates the file exists and returns success.
        debug!(
            "File validated for indexing: {} ({} bytes) - parsing handled by ParserHandler",
            path.display(),
            file_size
        );
        Ok(())
    }

    async fn remove_file_index(&self, path: &PathBuf) -> Result<()> {
        debug!("Removing index for file: {}", path.display());
        Ok(())
    }

    /// Check if a file event should be processed (debouncing logic)
    async fn should_process_file_event(&self, path: &PathBuf) -> bool {
        // Simple debouncing - in a real implementation, you'd track recent events
        // For now, always process supported files
        self.should_index_file(path)
    }

    /// Handle file move events (delete + create)
    async fn handle_file_move(&self, from: &PathBuf, to: &PathBuf) -> Result<()> {
        debug!("Handling file move: {} -> {}", from.display(), to.display());

        // Remove old index
        if let Err(e) = self.remove_file_index(from).await {
            warn!(
                "Failed to remove index for moved file {}: {}",
                from.display(),
                e
            );
        }

        // Index new location (treat as Created event)
        if let Err(e) = self
            .index_file(to, crate::watch::events::FileEventKind::Created)
            .await
        {
            error!("Failed to index moved file {}: {}", to.display(), e);
            return Err(e);
        }

        info!(
            "Successfully processed file move: {} -> {}",
            from.display(),
            to.display()
        );
        Ok(())
    }

    /// Handle batch events for improved performance
    async fn handle_batch_events(&self, events: &[FileEvent]) -> Result<()> {
        info!("Processing batch of {} events", events.len());

        let mut successful = 0;
        let mut failed = 0;
        let start_time = std::time::Instant::now();

        for event in events {
            match self.handle(event.clone()).await {
                Ok(_) => successful += 1,
                Err(e) => {
                    failed += 1;
                    warn!(
                        "Failed to process batch event for {}: {}",
                        event.path.display(),
                        e
                    );
                }
            }
        }

        let elapsed = start_time.elapsed();
        info!(
            "Batch processing completed: {}/{} events successful in {:?}",
            successful,
            events.len(),
            elapsed
        );

        if failed > 0 {
            warn!(
                "{} out of {} batch events failed processing",
                failed,
                events.len()
            );
        }

        Ok(())
    }

    /// Emit a SessionEvent corresponding to the file change.
    ///
    /// Converts a `FileEvent` from the watch system into a `SessionEvent` variant
    /// (`FileChanged`, `FileDeleted`, or `FileMoved`) and emits it to the event bus.
    async fn emit_session_event(&self, event: &FileEvent) {
        use crucible_core::events::FileChangeKind;

        let session_event = match &event.kind {
            crate::watch::events::FileEventKind::Created => {
                SessionEvent::internal(InternalSessionEvent::FileChanged {
                    path: event.path.clone(),
                    kind: FileChangeKind::Created,
                })
            }
            crate::watch::events::FileEventKind::Modified => {
                SessionEvent::internal(InternalSessionEvent::FileChanged {
                    path: event.path.clone(),
                    kind: FileChangeKind::Modified,
                })
            }
            crate::watch::events::FileEventKind::Deleted => {
                SessionEvent::internal(InternalSessionEvent::FileDeleted {
                    path: event.path.clone(),
                })
            }
            crate::watch::events::FileEventKind::Moved { from, to } => {
                SessionEvent::internal(InternalSessionEvent::FileMoved {
                    from: from.clone(),
                    to: to.clone(),
                })
            }
            crate::watch::events::FileEventKind::Batch(events) => {
                // Recursively emit events for batch operations
                for batch_event in events {
                    // Use Box::pin to avoid infinitely-sized future
                    Box::pin(self.emit_session_event(batch_event)).await;
                }
                return;
            }
            crate::watch::events::FileEventKind::Unknown(_) => {
                debug!(
                    "Not emitting SessionEvent for unknown file event: {}",
                    event.path.display()
                );
                return;
            }
        };

        // Emit the event to the bus
        match self.emitter.emit(session_event).await {
            Ok(outcome) => {
                if outcome.cancelled {
                    debug!(
                        "FileChanged event was cancelled for: {}",
                        event.path.display()
                    );
                } else if outcome.has_errors() {
                    warn!(
                        "FileChanged event had {} handler errors for: {}",
                        outcome.error_count(),
                        event.path.display()
                    );
                } else {
                    debug!(
                        "Successfully emitted SessionEvent for: {}",
                        event.path.display()
                    );
                }
            }
            Err(e) => {
                warn!(
                    "Failed to emit SessionEvent for {}: {}",
                    event.path.display(),
                    e
                );
            }
        }
    }

    /// Log detailed error information for debugging
    fn log_event_error(&self, event: &FileEvent, error: &Error, elapsed: std::time::Duration) {
        error!("Event processing error details:");
        error!("  - Event type: {:?}", event.kind);
        error!("  - File path: {}", event.path.display());
        error!("  - File exists: {}", event.path.exists());
        error!("  - Processing time: {:?}", elapsed);
        error!("  - Error: {}", error);

        // Add file-specific context if available
        if event.path.exists() {
            if let Ok(metadata) = std::fs::metadata(&event.path) {
                error!("  - File size: {} bytes", metadata.len());
                if let Ok(modified) = metadata.modified() {
                    error!("  - Last modified: {:?}", modified);
                }
            }
        }

        // Check for common issues
        if error.to_string().contains("permission") {
            error!("  - Likely cause: File permission issues");
        } else if error.to_string().contains("not found") {
            error!("  - Likely cause: File was deleted during processing");
        } else if error.to_string().contains("frontmatter") {
            error!("  - Likely cause: Invalid YAML frontmatter in markdown file");
        } else {
            error!("  - Likely cause: Parse error or I/O issue");
        }
    }
}

#[async_trait]
impl EventHandler for IndexingHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        debug!("Indexing handler processing event: {:?}", event.kind);

        // Add debouncing for rapid successive events
        let should_process = match &event.kind {
            crate::watch::events::FileEventKind::Created
            | crate::watch::events::FileEventKind::Modified => {
                self.should_process_file_event(&event.path).await
            }
            _ => true, // Always process deletes and moves
        };

        if !should_process {
            debug!("Skipping debounced event for: {}", event.path.display());
            return Ok(());
        }

        // Emit SessionEvent for the file change
        self.emit_session_event(&event).await;

        let start_time = std::time::Instant::now();
        let result = match event.kind {
            crate::watch::events::FileEventKind::Created
            | crate::watch::events::FileEventKind::Modified => {
                self.index_file(&event.path, event.kind.clone()).await
            }
            crate::watch::events::FileEventKind::Deleted => {
                self.remove_file_index(&event.path).await
            }
            crate::watch::events::FileEventKind::Moved { ref from, ref to } => {
                // Handle move as delete + create operation
                self.handle_file_move(from, to).await
            }
            crate::watch::events::FileEventKind::Batch(ref events) => {
                self.handle_batch_events(events).await
            }
            crate::watch::events::FileEventKind::Unknown(_) => {
                debug!("Unknown event type, skipping: {}", event.path.display());
                Ok(())
            }
        };

        let elapsed = start_time.elapsed();

        // Log event processing performance
        match &result {
            Ok(_) => {
                debug!(
                    "Successfully processed event {:?} for {} in {:?}",
                    event.kind,
                    event.path.display(),
                    elapsed
                );
            }
            Err(e) => {
                warn!(
                    "Failed to process event {:?} for {} after {:?}: {}",
                    event.kind,
                    event.path.display(),
                    elapsed,
                    e
                );

                // Add error context for better debugging
                self.log_event_error(&event, e, elapsed);
            }
        }

        result
    }

    fn name(&self) -> &'static str {
        "indexing"
    }

    fn priority(&self) -> u32 {
        200 // High priority for indexing
    }

    fn can_handle(&self, event: &FileEvent) -> bool {
        // Handle all file events, but will filter internally
        if event.is_dir {
            return false;
        }

        // Check if the file extension is supported
        self.should_index_file(&event.path)
    }
}
