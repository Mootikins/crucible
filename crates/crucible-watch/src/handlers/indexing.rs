//! Integration handler for automatic file parsing and database indexing.
//! Integrates PulldownParser with file watching for real-time document processing.

use crate::{events::FileEvent, traits::EventHandler, error::{Error, Result}};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Handler for automatically indexing files when they change.
/// Integrates with PulldownParser for document parsing and prepares for database storage.
pub struct IndexingHandler {
    supported_extensions: Vec<String>,
    index_debounce: std::time::Duration,
    // Database connection will be added in Phase 4
}

impl IndexingHandler {
    /// Create a new indexing handler.
    pub fn new() -> Result<Self> {
        info!("IndexingHandler created with PulldownParser integration");
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

    /// Initialize the database connection (Phase 4 placeholder).
    pub async fn initialize_database(&self, _db_path: &str) -> Result<()> {
        info!("Database initialization will be implemented in Phase 4");
        // Phase 4: Initialize SurrealDB connection here
        Ok(())
    }

    /// Set the embedding configuration (Phase 4 placeholder).
    pub fn set_embedding_config(&mut self, _config: ()) {
        info!("Embedding configuration will be implemented in Phase 4");
        // Phase 4: Configure embedding generation here
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
                return Err(Error::io(format!("Failed to read metadata for {}: {}", path.display(), e)));
            }
        };

        let file_size = file_metadata.len();
        debug!("Starting indexing for file: {} ({} bytes)", path.display(), file_size);

        // Use PulldownParser to parse the file
        let parser = crucible_core::parser::PulldownParser::new();

        let start_time = std::time::Instant::now();

        match parser.parse_file(path).await {
            Ok(parsed_doc) => {
                let elapsed = start_time.elapsed();
                let total_blocks = parsed_doc.content.headings.len() +
                                 parsed_doc.content.paragraphs.len() +
                                 parsed_doc.content.code_blocks.len() +
                                 parsed_doc.content.lists.len();

                info!("Successfully parsed file: {} ({} blocks, {} bytes, {:?})",
                      path.display(),
                      total_blocks,
                      file_size,
                      elapsed);

                // Report parsing progress
                self.report_parsing_progress(&parsed_doc, file_size, elapsed);

                // Log extraction details for debugging
                self.log_parsed_document(&parsed_doc);

                // Phase 4: Store parsed blocks in database
                debug!("Database storage will be implemented in Phase 4");

                Ok(())
            }
            Err(e) => {
                let elapsed = start_time.elapsed();
                error!("Failed to parse file {} after {:?}: {}", path.display(), elapsed, e);

                // Provide more detailed error information
                let error_context = if let Some(io_err) = e.source().and_then(|e| e.downcast_ref::<std::io::Error>()) {
                    format!("I/O error while reading {}: {}", path.display(), io_err)
                } else if e.to_string().contains("frontmatter") {
                    format!("Frontmatter parsing error in {}: {}", path.display(), e)
                } else {
                    format!("Parse error for {}: {}", path.display(), e)
                };

                Err(Error::parser(error_context))
            }
        }
    }

    async fn remove_file_index(&self, path: &PathBuf) -> Result<()> {
        debug!("Removing index for file: {}", path.display());
        // Phase 4: Remove document and associated blocks from database
        debug!("Database removal will be implemented in Phase 4");
        Ok(())
    }

    /// Report parsing progress and performance metrics
    fn report_parsing_progress(&self, doc: &crucible_core::parser::ParsedDocument, file_size: u64, elapsed: std::time::Duration) {
        let content = &doc.content;
        let total_blocks = content.headings.len() + content.paragraphs.len() + content.code_blocks.len() + content.lists.len();

        // Calculate performance metrics
        let bytes_per_second = if elapsed.as_secs() > 0 {
            file_size / elapsed.as_secs()
        } else {
            file_size
        };

        let blocks_per_second = if elapsed.as_secs() > 0 {
            total_blocks as u64 / elapsed.as_secs()
        } else {
            total_blocks as u64
        };

        let bytes_per_block = if total_blocks > 0 {
            file_size / total_blocks as u64
        } else {
            file_size
        };

        info!("Parsing performance metrics:");
        info!("  - Processing rate: {} bytes/sec", bytes_per_second);
        info!("  - Block extraction rate: {} blocks/sec", blocks_per_second);
        info!("  - Average block size: {} bytes/block", bytes_per_block);
        info!("  - Content density: {:.2} blocks/KB", (total_blocks as f64) / (file_size as f64 / 1024.0));

        // Report content breakdown
        if total_blocks > 0 {
            let heading_pct = (content.headings.len() as f64 / total_blocks as f64) * 100.0;
            let paragraph_pct = (content.paragraphs.len() as f64 / total_blocks as f64) * 100.0;
            let code_pct = (content.code_blocks.len() as f64 / total_blocks as f64) * 100.0;
            let list_pct = (content.lists.len() as f64 / total_blocks as f64) * 100.0;

            debug!("Content breakdown:");
            debug!("  - Headings: {} ({:.1}%)", content.headings.len(), heading_pct);
            debug!("  - Paragraphs: {} ({:.1}%)", content.paragraphs.len(), paragraph_pct);
            debug!("  - Code blocks: {} ({:.1}%)", content.code_blocks.len(), code_pct);
            debug!("  - Lists: {} ({:.1}%)", content.lists.len(), list_pct);
        }

        // Report task progress if applicable
        let total_tasks: usize = content.lists.iter()
            .flat_map(|l| &l.items)
            .filter(|item| item.task_status.is_some())
            .count();

        if total_tasks > 0 {
            let completed_tasks = content.lists.iter()
                .flat_map(|l| &l.items)
                .filter(|item| item.task_status == Some(crucible_core::parser::TaskStatus::Completed))
                .count();

            let completion_rate = (completed_tasks as f64 / total_tasks as f64) * 100.0;
            info!("Task progress: {} total tasks ({:.1}% completed)", total_tasks, completion_rate);
        }

        // Report link and tag density
        let link_density = if doc.content.word_count > 0 {
            (doc.wikilinks.len() as f64 / doc.content.word_count as f64) * 100.0
        } else {
            0.0
        };

        let tag_density = if doc.content.word_count > 0 {
            (doc.tags.len() as f64 / doc.content.word_count as f64) * 100.0
        } else {
            0.0
        };

        if link_density > 0.0 || tag_density > 0.0 {
            debug!("Link and tag density: {:.2}% links, {:.2}% tags", link_density, tag_density);
        }
    }

    /// Log details about a parsed document for debugging and progress tracking
    fn log_parsed_document(&self, doc: &crucible_core::parser::ParsedDocument) {
        let content = &doc.content;

        debug!("Parsed document summary:");
        debug!("  - Title: {}", doc.title());
        debug!("  - Headings: {}", content.headings.len());
        debug!("  - Paragraphs: {}", content.paragraphs.len());
        debug!("  - Code blocks: {}", content.code_blocks.len());
        debug!("  - Lists: {}", content.lists.len());
        debug!("  - Word count: {}", content.word_count);
        debug!("  - Char count: {}", content.char_count);

        // Log task statistics if any tasks found
        let total_tasks: usize = content.lists.iter()
            .flat_map(|l| &l.items)
            .filter(|item| item.task_status.is_some())
            .count();

        if total_tasks > 0 {
            let completed_tasks = content.lists.iter()
                .flat_map(|l| &l.items)
                .filter(|item| item.task_status == Some(crucible_core::parser::TaskStatus::Completed))
                .count();

            debug!("  - Tasks: {} total ({} completed, {} pending)",
                   total_tasks, completed_tasks, total_tasks - completed_tasks);
        }

        // Log wikilink and tag statistics
        if !doc.wikilinks.is_empty() {
            debug!("  - Wikilinks: {}", doc.wikilinks.len());
        }
        if !doc.tags.is_empty() {
            debug!("  - Tags: {}", doc.tags.len());
        }
    }
}

#[async_trait]
impl EventHandler for IndexingHandler {
    async fn handle(&self, event: FileEvent) -> Result<()> {
        debug!("Indexing handler processing event: {:?}", event.kind);

        // Add debouncing for rapid successive events
        let should_process = match &event.kind {
            crate::events::FileEventKind::Created | crate::events::FileEventKind::Modified => {
                self.should_process_file_event(&event.path).await
            }
            _ => true, // Always process deletes and moves
        };

        if !should_process {
            debug!("Skipping debounced event for: {}", event.path.display());
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        let result = match event.kind {
            crate::events::FileEventKind::Created | crate::events::FileEventKind::Modified => {
                self.index_file(&event.path).await
            }
            crate::events::FileEventKind::Deleted => {
                self.remove_file_index(&event.path).await
            }
            crate::events::FileEventKind::Moved { from, to } => {
                // Handle move as delete + create operation
                self.handle_file_move(&from, &to).await
            }
            crate::events::FileEventKind::Batch(ref events) => {
                self.handle_batch_events(events).await
            }
            crate::events::FileEventKind::Unknown(_) => {
                debug!("Unknown event type, skipping: {}", event.path.display());
                Ok(())
            }
        };

        let elapsed = start_time.elapsed();

        // Log event processing performance
        match &result {
            Ok(_) => {
                debug!("Successfully processed event {:?} for {} in {:?}",
                       event.kind, event.path.display(), elapsed);
            }
            Err(e) => {
                warn!("Failed to process event {:?} for {} after {:?}: {}",
                      event.kind, event.path.display(), elapsed, e);

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
            warn!("Failed to remove index for moved file {}: {}", from.display(), e);
        }

        // Index new location
        if let Err(e) = self.index_file(to).await {
            error!("Failed to index moved file {}: {}", to.display(), e);
            return Err(e);
        }

        info!("Successfully processed file move: {} -> {}", from.display(), to.display());
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
                    warn!("Failed to process batch event for {}: {}", event.path.display(), e);
                }
            }
        }

        let elapsed = start_time.elapsed();
        info!("Batch processing completed: {}/{} events successful in {:?}",
              successful, events.len(), elapsed);

        if failed > 0 {
            warn!("{} out of {} batch events failed processing", failed, events.len());
        }

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{FileEvent, FileEventKind};
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tokio::fs;

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

    #[tokio::test]
    async fn test_index_file_with_pulldown_parser() -> Result<()> {
        let handler = IndexingHandler::new()?;
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.md");

        // Create test markdown content
        let test_content = r#"---
title: Test Document
tags: [test, indexing]
---

# Main Heading

This is a test paragraph with **bold** text.

## Code Section

```rust
fn hello() {
    println!("Hello, world!");
}
```

## Task List

- [x] Completed task
- [ ] Pending task

"#;

        fs::write(&file_path, test_content).await?;

        // Test file indexing
        handler.index_file(&file_path).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_index_unsupported_file() -> Result<()> {
        let handler = IndexingHandler::new()?;
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.exe");

        fs::write(&file_path, "binary content").await?;

        // Should skip unsupported file without error
        handler.index_file(&file_path).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_index_nonexistent_file() {
        let handler = IndexingHandler::new().unwrap();
        let nonexistent_path = PathBuf::from("/nonexistent/path/file.md");

        // Should return error for nonexistent file
        let result = handler.index_file(&nonexistent_path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_event_handling() -> Result<()> {
        let handler = IndexingHandler::new()?;
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.md");

        let test_content = r#"---
title: Event Test
---

# Test Content

Content for event handling test.

"#;

        fs::write(&file_path, test_content).await?;

        // Test file creation event
        let create_event = FileEvent::new(FileEventKind::Created, file_path.clone());
        handler.handle(create_event).await?;

        // Test file modification event
        let modify_event = FileEvent::new(FileEventKind::Modified, file_path.clone());
        handler.handle(modify_event).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_moved_file_event() -> Result<()> {
        let handler = IndexingHandler::new()?;
        let temp_dir = TempDir::new()?;
        let from_path = temp_dir.path().join("from.md");
        let to_path = temp_dir.path().join("to.md");

        let test_content = r#"---
title: Moved File
---

# Moved Content

This file was moved.

"#;

        fs::write(&from_path, test_content).await?;

        // Test file move event
        let move_event = FileEvent::new(FileEventKind::Moved { from: from_path.clone(), to: to_path.clone() }, to_path.clone());
        handler.handle(move_event).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_file_event() -> Result<()> {
        let handler = IndexingHandler::new()?;
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("delete.md");

        let test_content = r#"---
title: Delete Test
---

# To Be Deleted

This file will be deleted.

"#;

        fs::write(&file_path, test_content).await?;

        // Test file deletion event
        let delete_event = FileEvent::new(FileEventKind::Deleted, file_path.clone());
        handler.handle(delete_event).await?;

        Ok(())
    }
}