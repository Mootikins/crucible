//! Persistence Handler for Session Events
//!
//! This module provides `PersistenceHandler`, a `Handler` implementation
//! that persists session events to markdown files in the kiln.
//!
//! ## Architecture
//!
//! The `PersistenceHandler` is the first handler in the topo-sorted handler chain:
//!
//! ```text
//! Handlers: [Persist]→[React]→[Emit] (topo order)
//! ```
//!
//! It converts each `SessionEvent` to a markdown block using `EventToMarkdown`
//! and appends it to the current context file.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_rune::persistence_handler::PersistenceHandler;
//! use crucible_rune::handler_chain::SessionHandlerChain;
//! use std::path::PathBuf;
//!
//! let handler = PersistenceHandler::new(PathBuf::from("/kiln/Sessions/my-session"));
//! let mut chain = SessionHandlerChain::new();
//! chain.add_handler(Box::new(handler)).unwrap();
//! ```

use async_trait::async_trait;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::handler::{Handler, HandlerContext, HandlerResult};
use crate::reactor::SessionEvent;
use crucible_core::events::markdown::EventToMarkdown;
use crucible_core::events::EventError;

/// Handler that persists session events to markdown files.
///
/// `PersistenceHandler` is designed to be the first handler in the processing
/// chain (no dependencies). It converts events to markdown blocks and appends
/// them to the current context file.
///
/// ## File Naming
///
/// Files follow the pattern `{index:03}-context.md`:
/// - `000-context.md` - Initial context file
/// - `001-context.md` - After first compaction
/// - etc.
///
/// ## Thread Safety
///
/// The handler uses atomic operations for sequence tracking and file index
/// management. File writes are synchronized via filesystem operations.
///
/// ## Error Handling
///
/// File I/O errors are treated as non-fatal by default, allowing the handler
/// chain to continue processing. Set `fatal_on_error` to `true` to stop
/// the chain on persistence failures.
pub struct PersistenceHandler {
    /// Session folder path containing context files.
    folder: PathBuf,
    /// Current file index (0 = 000-context.md).
    file_index: AtomicUsize,
    /// Sequence number of last persisted event.
    last_persisted_seq: AtomicU64,
    /// Whether to treat I/O errors as fatal.
    fatal_on_error: bool,
    /// Handler name for dependency declarations.
    name: String,
}

impl PersistenceHandler {
    /// Handler name constant for dependency declarations.
    pub const NAME: &'static str = "persist";

    /// Create a new persistence handler.
    ///
    /// # Arguments
    ///
    /// * `folder` - Path to the session folder where context files are stored.
    pub fn new(folder: PathBuf) -> Self {
        Self {
            folder,
            file_index: AtomicUsize::new(0),
            last_persisted_seq: AtomicU64::new(0),
            fatal_on_error: false,
            name: Self::NAME.to_string(),
        }
    }

    /// Create a persistence handler with a custom name.
    ///
    /// Useful when multiple persistence handlers are needed in a chain.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set whether I/O errors should be fatal.
    ///
    /// When `true`, persistence failures stop the handler chain.
    /// When `false` (default), failures are logged but processing continues.
    pub fn with_fatal_on_error(mut self, fatal: bool) -> Self {
        self.fatal_on_error = fatal;
        self
    }

    /// Set the initial file index.
    ///
    /// Use when resuming a session that already has context files.
    pub fn with_file_index(self, index: usize) -> Self {
        self.file_index.store(index, Ordering::SeqCst);
        self
    }

    /// Get the current file index.
    pub fn file_index(&self) -> usize {
        self.file_index.load(Ordering::SeqCst)
    }

    /// Get the path to the current context file.
    pub fn current_file_path(&self) -> PathBuf {
        let index = self.file_index.load(Ordering::SeqCst);
        self.folder.join(format!("{:03}-context.md", index))
    }

    /// Increment the file index (used during compaction).
    ///
    /// Returns the new index.
    pub fn increment_file_index(&self) -> usize {
        self.file_index.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Get the sequence number of the last persisted event.
    pub fn last_persisted_seq(&self) -> u64 {
        self.last_persisted_seq.load(Ordering::SeqCst)
    }

    /// Get the session folder path.
    pub fn folder(&self) -> &PathBuf {
        &self.folder
    }

    /// Persist an event to the current context file.
    ///
    /// # Arguments
    ///
    /// * `event` - The session event to persist.
    /// * `timestamp_ms` - Optional timestamp in milliseconds since UNIX epoch.
    ///
    /// # Returns
    ///
    /// The number of bytes written on success.
    pub fn persist_event(
        &self,
        event: &SessionEvent,
        timestamp_ms: Option<u64>,
    ) -> Result<usize, std::io::Error> {
        let file_path = self.current_file_path();
        let timestamp = timestamp_ms.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        });

        // Convert event to markdown block
        let markdown = event.to_markdown_block(Some(timestamp));
        let bytes_written = markdown.len();

        // Open file in append mode, create if doesn't exist
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;

        // Write the markdown block
        file.write_all(markdown.as_bytes())?;

        // Flush to ensure data is written
        file.flush()?;

        tracing::trace!(
            file = %file_path.display(),
            event_type = event.event_type_name(),
            bytes_written,
            "Persisted event to context file"
        );

        Ok(bytes_written)
    }

    /// Persist multiple events in a batch.
    ///
    /// More efficient than persisting events one at a time as it
    /// only opens the file once.
    ///
    /// # Arguments
    ///
    /// * `events` - Slice of events to persist.
    /// * `timestamp_ms` - Optional timestamp to use for all events.
    ///
    /// # Returns
    ///
    /// The total number of bytes written on success.
    pub fn persist_batch(
        &self,
        events: &[Arc<SessionEvent>],
        timestamp_ms: Option<u64>,
    ) -> Result<usize, std::io::Error> {
        if events.is_empty() {
            return Ok(0);
        }

        let file_path = self.current_file_path();
        let timestamp = timestamp_ms.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        });

        // Open file in append mode, create if doesn't exist
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;

        let mut total_bytes = 0;

        for event in events {
            let markdown = event.to_markdown_block(Some(timestamp));
            total_bytes += markdown.len();
            file.write_all(markdown.as_bytes())?;
        }

        // Flush to ensure data is written
        file.flush()?;

        tracing::debug!(
            file = %file_path.display(),
            event_count = events.len(),
            total_bytes,
            "Persisted event batch to context file"
        );

        Ok(total_bytes)
    }

    /// Ensure the session folder exists.
    ///
    /// Creates the folder and any parent directories if they don't exist.
    pub fn ensure_folder_exists(&self) -> Result<(), std::io::Error> {
        if !self.folder.exists() {
            std::fs::create_dir_all(&self.folder)?;
            tracing::debug!(
                folder = %self.folder.display(),
                "Created session folder"
            );
        }
        Ok(())
    }
}

impl std::fmt::Debug for PersistenceHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistenceHandler")
            .field("name", &self.name)
            .field("folder", &self.folder)
            .field("file_index", &self.file_index.load(Ordering::SeqCst))
            .field(
                "last_persisted_seq",
                &self.last_persisted_seq.load(Ordering::SeqCst),
            )
            .field("fatal_on_error", &self.fatal_on_error)
            .finish()
    }
}

#[async_trait]
impl Handler for PersistenceHandler {
    fn name(&self) -> &str {
        &self.name
    }

    fn dependencies(&self) -> &[&str] {
        &[]
    }

    fn priority(&self) -> i32 {
        0
    }

    fn event_pattern(&self) -> &str {
        "*"
    }

    async fn handle(
        &self,
        ctx: &mut HandlerContext,
        event: SessionEvent,
    ) -> HandlerResult<SessionEvent> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        match self.persist_event(&event, Some(timestamp_ms)) {
            Ok(bytes_written) => {
                self.last_persisted_seq.fetch_add(1, Ordering::SeqCst);

                ctx.set("persist:bytes_written", serde_json::json!(bytes_written));
                ctx.set(
                    "persist:file_path",
                    serde_json::json!(self.current_file_path()),
                );

                tracing::trace!(
                    handler = %self.name,
                    event_type = event.event_type_name(),
                    bytes_written,
                    "Event persisted"
                );

                HandlerResult::ok(event)
            }
            Err(e) => {
                let error_msg = format!(
                    "Failed to persist event to {}: {}",
                    self.current_file_path().display(),
                    e
                );

                tracing::error!(
                    handler = %self.name,
                    file = %self.current_file_path().display(),
                    error = %e,
                    "Event persistence failed"
                );

                if self.fatal_on_error {
                    HandlerResult::FatalError(EventError::other(error_msg))
                } else {
                    HandlerResult::soft_error(event, error_msg)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::Handler;
    use serde_json::json;
    use std::path::PathBuf;
    use tempfile::tempdir;

    /// Cross-platform test path helper
    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("crucible_test_{}", name))
    }

    #[test]
    fn test_persistence_handler_new() {
        let folder = test_path("test-session");
        let handler = PersistenceHandler::new(folder.clone());

        assert_eq!(Handler::name(&handler), PersistenceHandler::NAME);
        assert_eq!(handler.folder(), &folder);
        assert_eq!(handler.file_index(), 0);
        assert_eq!(handler.last_persisted_seq(), 0);
    }

    #[test]
    fn test_persistence_handler_with_name() {
        let handler = PersistenceHandler::new(test_path("test")).with_name("custom_persist");

        assert_eq!(Handler::name(&handler), "custom_persist");
    }

    #[test]
    fn test_persistence_handler_with_fatal_on_error() {
        let handler = PersistenceHandler::new(test_path("test")).with_fatal_on_error(true);

        assert!(handler.fatal_on_error);
    }

    #[test]
    fn test_persistence_handler_with_file_index() {
        let handler = PersistenceHandler::new(test_path("test")).with_file_index(5);

        assert_eq!(handler.file_index(), 5);
    }

    #[test]
    fn test_current_file_path() {
        let folder = test_path("test-session");
        let handler = PersistenceHandler::new(folder.clone());

        assert_eq!(handler.current_file_path(), folder.join("000-context.md"));

        handler.file_index.store(3, Ordering::SeqCst);
        assert_eq!(handler.current_file_path(), folder.join("003-context.md"));

        handler.file_index.store(42, Ordering::SeqCst);
        assert_eq!(handler.current_file_path(), folder.join("042-context.md"));
    }

    #[test]
    fn test_increment_file_index() {
        let handler = PersistenceHandler::new(test_path("test"));

        assert_eq!(handler.file_index(), 0);

        let new_index = handler.increment_file_index();
        assert_eq!(new_index, 1);
        assert_eq!(handler.file_index(), 1);

        let new_index = handler.increment_file_index();
        assert_eq!(new_index, 2);
        assert_eq!(handler.file_index(), 2);
    }

    #[test]
    fn test_persist_event() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        let event = SessionEvent::MessageReceived {
            content: "Hello, world!".into(),
            participant_id: "user".into(),
        };

        let bytes_written = handler.persist_event(&event, None).unwrap();
        assert!(bytes_written > 0);

        // Verify file was created
        let file_path = dir.path().join("000-context.md");
        assert!(file_path.exists());

        // Verify content
        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("MessageReceived"));
        assert!(content.contains("Hello, world!"));
        assert!(content.contains("**Participant:** user"));
        assert!(content.contains("---"));
    }

    #[test]
    fn test_persist_event_with_timestamp() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        let event = SessionEvent::AgentThinking {
            thought: "Processing...".into(),
        };

        // Use a fixed timestamp: 2025-12-14T15:30:45.123
        let timestamp_ms = 1765726245123u64;
        handler.persist_event(&event, Some(timestamp_ms)).unwrap();

        let content = std::fs::read_to_string(handler.current_file_path()).unwrap();
        assert!(content.contains("2025-12-14T15:30:45.123"));
        assert!(content.contains("AgentThinking"));
        assert!(content.contains("Processing..."));
    }

    #[test]
    fn test_persist_batch() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        let events: Vec<Arc<SessionEvent>> = vec![
            Arc::new(SessionEvent::MessageReceived {
                content: "Message 1".into(),
                participant_id: "user".into(),
            }),
            Arc::new(SessionEvent::AgentThinking {
                thought: "Thinking...".into(),
            }),
            Arc::new(SessionEvent::MessageReceived {
                content: "Message 2".into(),
                participant_id: "assistant".into(),
            }),
        ];

        let bytes_written = handler.persist_batch(&events, None).unwrap();
        assert!(bytes_written > 0);

        // Verify content
        let content = std::fs::read_to_string(handler.current_file_path()).unwrap();

        // All events should be in the file
        assert!(content.contains("Message 1"));
        assert!(content.contains("Thinking..."));
        assert!(content.contains("Message 2"));

        // Should have 3 event blocks (3 separators)
        let separator_count = content.matches("\n---\n").count();
        assert_eq!(separator_count, 3);
    }

    #[test]
    fn test_persist_batch_empty() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        let events: Vec<Arc<SessionEvent>> = vec![];
        let bytes_written = handler.persist_batch(&events, None).unwrap();

        assert_eq!(bytes_written, 0);
        // File should not be created for empty batch
        assert!(!handler.current_file_path().exists());
    }

    #[test]
    fn test_ensure_folder_exists() {
        let dir = tempdir().unwrap();
        let nested_folder = dir.path().join("nested").join("deep").join("session");
        let handler = PersistenceHandler::new(nested_folder.clone());

        assert!(!nested_folder.exists());

        handler.ensure_folder_exists().unwrap();

        assert!(nested_folder.exists());
        assert!(nested_folder.is_dir());
    }

    #[test]
    fn test_ensure_folder_exists_already_exists() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        // Should succeed even if folder exists
        handler.ensure_folder_exists().unwrap();
        handler.ensure_folder_exists().unwrap();
    }

    #[test]
    fn test_handler_debug() {
        let folder = test_path("test");
        let handler = PersistenceHandler::new(folder.clone());
        let debug = format!("{:?}", handler);

        assert!(debug.contains("PersistenceHandler"));
        assert!(debug.contains("persist"));
        assert!(debug.contains(&folder.to_string_lossy().to_string()));
    }

    #[test]
    fn test_handler_no_dependencies() {
        let handler = PersistenceHandler::new(test_path("test"));
        assert!(Handler::dependencies(&handler).is_empty());
    }

    #[tokio::test]
    async fn test_handler_handle() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        let mut ctx = HandlerContext::new();
        let mock_path = test_path("test.txt");
        let event = SessionEvent::ToolCalled {
            name: "read_file".into(),
            args: json!({"path": mock_path.to_string_lossy()}),
        };

        let result = Handler::handle(&handler, &mut ctx, event).await;

        assert!(result.is_continue());
        assert!(ctx.get("persist:bytes_written").is_some());
        assert!(ctx.get("persist:file_path").is_some());

        let content = std::fs::read_to_string(handler.current_file_path()).unwrap();
        assert!(content.contains("ToolCalled"));
        assert!(content.contains("read_file"));
    }

    #[tokio::test]
    async fn test_handler_handle_io_error_non_fatal() {
        let handler = PersistenceHandler::new(PathBuf::from("/root/nonexistent/session"));

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        };

        let result = Handler::handle(&handler, &mut ctx, event).await;

        assert!(result.is_soft_error());
        assert!(result.event().is_some());
    }

    #[tokio::test]
    async fn test_handler_handle_io_error_fatal() {
        let handler = PersistenceHandler::new(PathBuf::from("/root/nonexistent/session"))
            .with_fatal_on_error(true);

        let mut ctx = HandlerContext::new();
        let event = SessionEvent::MessageReceived {
            content: "Test".into(),
            participant_id: "user".into(),
        };

        let result = Handler::handle(&handler, &mut ctx, event).await;

        assert!(result.is_fatal());
    }

    #[tokio::test]
    async fn test_handler_handle_multiple_events() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        let events = [
            SessionEvent::MessageReceived {
                content: "First".into(),
                participant_id: "user".into(),
            },
            SessionEvent::MessageReceived {
                content: "Second".into(),
                participant_id: "user".into(),
            },
            SessionEvent::MessageReceived {
                content: "Third".into(),
                participant_id: "user".into(),
            },
        ];

        for event in events {
            let mut ctx = HandlerContext::new();
            let result = Handler::handle(&handler, &mut ctx, event).await;
            assert!(result.is_continue());
        }

        let content = std::fs::read_to_string(handler.current_file_path()).unwrap();
        assert!(content.contains("First"));
        assert!(content.contains("Second"));
        assert!(content.contains("Third"));
    }

    #[tokio::test]
    async fn test_integration_with_session_handler_chain() {
        use crate::handler_chain::SessionHandlerChain;

        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());
        handler.ensure_folder_exists().unwrap();

        let mut chain = SessionHandlerChain::new();
        chain.add_handler(Box::new(handler)).unwrap();
        chain.validate().unwrap();

        let event = SessionEvent::MessageReceived {
            content: "Test via chain".into(),
            participant_id: "user".into(),
        };

        let (result, _) = chain.process(event).await.unwrap();

        assert!(result.is_ok());
        assert!(!result.has_errors());

        let content = std::fs::read_to_string(dir.path().join("000-context.md")).unwrap();
        assert!(content.contains("Test via chain"));
    }

    #[tokio::test]
    async fn test_persist_all_event_types() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        // Test all event types
        let events: Vec<SessionEvent> = vec![
            SessionEvent::MessageReceived {
                content: "Hello".into(),
                participant_id: "user".into(),
            },
            SessionEvent::AgentResponded {
                content: "Response".into(),
                tool_calls: vec![],
            },
            SessionEvent::AgentThinking {
                thought: "Thinking...".into(),
            },
            SessionEvent::ToolCalled {
                name: "search".into(),
                args: json!({"query": "test"}),
            },
            SessionEvent::ToolCompleted {
                name: "search".into(),
                result: "Found 5 results".into(),
                error: None,
            },
            SessionEvent::SessionStarted {
                config: crate::reactor::SessionEventConfig::new("test")
                    .with_folder(test_path("test")),
            },
            SessionEvent::SessionCompacted {
                summary: "Session summary".into(),
                new_file: test_path("001-context.md"),
            },
            SessionEvent::SessionEnded {
                reason: "User ended session".into(),
            },
            SessionEvent::SubagentSpawned {
                id: "sub1".into(),
                prompt: "Do something".into(),
            },
            SessionEvent::SubagentCompleted {
                id: "sub1".into(),
                result: "Done".into(),
            },
            SessionEvent::SubagentFailed {
                id: "sub1".into(),
                error: "Timeout".into(),
            },
            SessionEvent::Custom {
                name: "custom_event".into(),
                payload: json!({"key": "value"}),
            },
        ];

        for event in &events {
            handler.persist_event(event, None).unwrap();
        }

        // Verify all events were written
        let content = std::fs::read_to_string(handler.current_file_path()).unwrap();

        // Check for key event types
        assert!(content.contains("MessageReceived"));
        assert!(content.contains("AgentResponded"));
        assert!(content.contains("AgentThinking"));
        assert!(content.contains("ToolCalled"));
        assert!(content.contains("ToolCompleted"));
        assert!(content.contains("SessionStarted"));
        assert!(content.contains("SessionCompacted"));
        assert!(content.contains("SessionEnded"));
        assert!(content.contains("SubagentSpawned"));
        assert!(content.contains("SubagentCompleted"));
        assert!(content.contains("SubagentFailed"));
        assert!(content.contains("Custom"));

        // Should have 12 event blocks
        let separator_count = content.matches("\n---\n").count();
        assert_eq!(separator_count, 12);
    }

    #[test]
    fn test_file_index_zero_padding() {
        let handler = PersistenceHandler::new(test_path("test"));

        // Test various indices for proper zero-padding
        handler.file_index.store(0, Ordering::SeqCst);
        assert!(handler
            .current_file_path()
            .to_string_lossy()
            .contains("000-context.md"));

        handler.file_index.store(1, Ordering::SeqCst);
        assert!(handler
            .current_file_path()
            .to_string_lossy()
            .contains("001-context.md"));

        handler.file_index.store(10, Ordering::SeqCst);
        assert!(handler
            .current_file_path()
            .to_string_lossy()
            .contains("010-context.md"));

        handler.file_index.store(99, Ordering::SeqCst);
        assert!(handler
            .current_file_path()
            .to_string_lossy()
            .contains("099-context.md"));

        handler.file_index.store(100, Ordering::SeqCst);
        assert!(handler
            .current_file_path()
            .to_string_lossy()
            .contains("100-context.md"));

        handler.file_index.store(999, Ordering::SeqCst);
        assert!(handler
            .current_file_path()
            .to_string_lossy()
            .contains("999-context.md"));
    }

    #[test]
    fn test_multiple_appends_same_file() {
        let dir = tempdir().unwrap();
        let handler = PersistenceHandler::new(dir.path().to_path_buf());

        // Append multiple events
        for i in 0..5 {
            let event = SessionEvent::MessageReceived {
                content: format!("Message {}", i),
                participant_id: "user".into(),
            };
            handler.persist_event(&event, None).unwrap();
        }

        // Verify all are in the same file
        let content = std::fs::read_to_string(handler.current_file_path()).unwrap();

        for i in 0..5 {
            assert!(
                content.contains(&format!("Message {}", i)),
                "File should contain Message {}",
                i
            );
        }

        // Still using the same file
        assert_eq!(handler.file_index(), 0);
    }
}
