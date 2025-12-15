//! Parser handler for file change event processing.
//!
//! This handler subscribes to `FileChanged` events and parses markdown files
//! to emit `NoteParsed` events. It bridges the file watching system with
//! the parsing and storage pipeline.
//!
//! # Event Flow
//!
//! ```text
//! FileChanged -> ParserHandler -> NoteParsed
//!      ^                              ^
//!    Watch                         Storage
//! ```
//!
//! # Event Subscriptions
//!
//! | Event | Action | Emits |
//! |-------|--------|-------|
//! | `FileChanged` | Parse markdown file | `NoteParsed` |
//!
//! # Priority
//!
//! ParserHandler runs at priority 50 (before storage handlers at 100) to ensure
//! files are parsed before storage operations attempt to persist them.

use crucible_core::events::{EventEmitter, NoOpEmitter, NotePayload, SessionEvent};
use crucible_parser::{CrucibleParser, MarkdownParser};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, warn};

/// Handler for parsing markdown files when they change.
///
/// Subscribes to `FileChanged` events and uses `CrucibleParser` to parse
/// markdown content. On successful parse, emits `NoteParsed` event with
/// block count for downstream handlers (StorageHandler, TagHandler, etc.).
pub struct ParserHandler {
    /// The markdown parser implementation
    parser: CrucibleParser,
    /// Event emitter for emitting NoteParsed events
    emitter: Arc<dyn EventEmitter<Event = SessionEvent>>,
    /// Supported file extensions (without leading dot)
    supported_extensions: Vec<String>,
}

impl ParserHandler {
    /// Create a new parser handler with default parser and NoOpEmitter.
    ///
    /// Uses a no-op emitter by default. To emit events, use `with_emitter()` to
    /// provide a real event bus.
    pub fn new() -> Self {
        Self::with_emitter(Arc::new(NoOpEmitter::new()))
    }

    /// Create a new parser handler with a custom event emitter.
    ///
    /// The emitter is used to emit `SessionEvent::NoteParsed` events after
    /// successfully parsing markdown files.
    pub fn with_emitter(emitter: Arc<dyn EventEmitter<Event = SessionEvent>>) -> Self {
        Self {
            parser: CrucibleParser::new(),
            emitter,
            supported_extensions: vec![
                "md".to_string(),
                "markdown".to_string(),
            ],
        }
    }

    /// Create a new parser handler with a custom parser and emitter.
    pub fn with_parser_and_emitter(
        parser: CrucibleParser,
        emitter: Arc<dyn EventEmitter<Event = SessionEvent>>,
    ) -> Self {
        Self {
            parser,
            emitter,
            supported_extensions: vec![
                "md".to_string(),
                "markdown".to_string(),
            ],
        }
    }

    /// Set custom supported file extensions.
    ///
    /// Extensions should be provided without the leading dot (e.g., "md" not ".md").
    pub fn with_supported_extensions(mut self, extensions: Vec<String>) -> Self {
        self.supported_extensions = extensions;
        self
    }

    /// Get reference to the parser.
    pub fn parser(&self) -> &CrucibleParser {
        &self.parser
    }

    /// Get reference to the emitter.
    pub fn emitter(&self) -> &Arc<dyn EventEmitter<Event = SessionEvent>> {
        &self.emitter
    }

    /// Get the supported file extensions.
    pub fn supported_extensions(&self) -> &[String] {
        &self.supported_extensions
    }

    /// Check if a file should be parsed based on its extension.
    pub fn should_parse(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| self.supported_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
            .unwrap_or(false)
    }

    /// Handle a FileChanged event by parsing the file and emitting NoteParsed.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the changed file
    ///
    /// # Returns
    ///
    /// Ok(()) if parsing and event emission succeeded (or file was skipped).
    /// Logs errors but doesn't fail - maintains fail-open semantics.
    pub async fn handle_file_changed(&self, path: &Path) {
        // Skip non-markdown files
        if !self.should_parse(path) {
            debug!(path = %path.display(), "Skipping non-markdown file");
            return;
        }

        // Skip non-existent files (might have been deleted between FileChanged and processing)
        if !path.exists() {
            debug!(path = %path.display(), "File does not exist, skipping parse");
            return;
        }

        debug!(path = %path.display(), "Parsing file for FileChanged event");

        // Parse the file
        match self.parser.parse_file(path).await {
            Ok(parsed_note) => {
                // Count blocks for the NoteParsed event
                let block_count = Self::count_blocks(&parsed_note);

                debug!(
                    path = %path.display(),
                    block_count = block_count,
                    title = ?parsed_note.title(),
                    tags = parsed_note.tags.len(),
                    wikilinks = parsed_note.wikilinks.len(),
                    "Successfully parsed file"
                );

                // Build NotePayload from parsed note
                let payload = NotePayload::new(
                    path.display().to_string(),
                    parsed_note.title(),
                )
                .with_tags(parsed_note.tags.iter().map(|t| t.name.clone()).collect())
                .with_wikilinks(parsed_note.wikilinks.iter().map(|w| w.target.clone()).collect());

                // Emit NoteParsed event with payload
                let event = SessionEvent::NoteParsed {
                    path: path.to_path_buf(),
                    block_count,
                    payload: Some(payload),
                };

                if let Err(e) = self.emitter.emit(event).await {
                    error!(
                        error = %e,
                        path = %path.display(),
                        "Failed to emit NoteParsed event"
                    );
                }
            }
            Err(e) => {
                // Log parse error but don't fail - file might be malformed temporarily
                warn!(
                    error = %e,
                    path = %path.display(),
                    "Failed to parse file"
                );
            }
        }
    }

    /// Count the total number of content blocks in a parsed note.
    fn count_blocks(note: &crucible_parser::ParsedNote) -> usize {
        let content = &note.content;
        content.headings.len()
            + content.paragraphs.len()
            + content.code_blocks.len()
            + content.lists.len()
            + content.blockquotes.len()
            + content.tables.len()
    }

    /// Handle a SessionEvent by dispatching to the appropriate handler method.
    ///
    /// This method can be called by any event system (EventBus, reactor, etc.)
    /// to process events. It handles:
    /// - `FileChanged` -> `handle_file_changed`
    ///
    /// Other events are ignored.
    ///
    /// # Priority
    ///
    /// This handler should be registered at priority 50 to ensure
    /// parsing happens before storage handlers process the NoteParsed events.
    pub async fn handle_event(&self, event: &SessionEvent) {
        match event {
            SessionEvent::FileChanged { path, kind: _ } => {
                self.handle_file_changed(path).await;
            }
            _ => {
                // Ignore other event types
            }
        }
    }

    /// Get the list of event types this handler processes.
    ///
    /// Useful for registering with an event system that supports filtering.
    pub fn handled_event_types() -> &'static [&'static str] {
        &["file_changed"]
    }

    /// Get the recommended handler priority.
    ///
    /// Parser handlers should run before storage handlers (priority 100) to ensure
    /// NoteParsed events are emitted before storage operations.
    pub const PRIORITY: i64 = 50;
}

impl Default for ParserHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::test_support::mocks::MockEventEmitter;
    use std::path::Path;
    use tempfile::TempDir;
    use tokio::fs;

    /// Helper to create a mock event emitter
    fn create_mock_emitter() -> (Arc<MockEventEmitter<SessionEvent>>, Arc<dyn EventEmitter<Event = SessionEvent>>) {
        let mock = Arc::new(MockEventEmitter::new());
        let emitter: Arc<dyn EventEmitter<Event = SessionEvent>> = mock.clone();
        (mock, emitter)
    }

    #[test]
    fn test_should_parse_markdown_files() {
        let handler = ParserHandler::new();

        assert!(handler.should_parse(Path::new("test.md")));
        assert!(handler.should_parse(Path::new("test.markdown")));
        assert!(handler.should_parse(Path::new("test.MD")));
        assert!(handler.should_parse(Path::new("test.MARKDOWN")));
        assert!(handler.should_parse(Path::new("/path/to/file.md")));

        assert!(!handler.should_parse(Path::new("test.txt")));
        assert!(!handler.should_parse(Path::new("test.rs")));
        assert!(!handler.should_parse(Path::new("test")));
        assert!(!handler.should_parse(Path::new("")));
    }

    #[test]
    fn test_custom_extensions() {
        let handler = ParserHandler::new()
            .with_supported_extensions(vec!["md".to_string(), "txt".to_string()]);

        assert!(handler.should_parse(Path::new("test.md")));
        assert!(handler.should_parse(Path::new("test.txt")));
        assert!(!handler.should_parse(Path::new("test.markdown")));
    }

    #[test]
    fn test_handled_event_types() {
        let types = ParserHandler::handled_event_types();
        assert!(types.contains(&"file_changed"));
    }

    #[test]
    fn test_priority_constant() {
        assert_eq!(ParserHandler::PRIORITY, 50);
        // Parser should run before storage handlers
        assert!(ParserHandler::PRIORITY < 100);
    }

    #[tokio::test]
    async fn test_handle_file_changed_emits_note_parsed() {
        let (mock, emitter) = create_mock_emitter();
        let handler = ParserHandler::with_emitter(emitter);

        // Create a temp directory with a markdown file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "# Test Note\n\nSome content here.\n").await.unwrap();

        // Handle the file change
        handler.handle_file_changed(&file_path).await;

        // Verify NoteParsed was emitted
        let events = mock.emitted_events();
        assert_eq!(events.len(), 1, "Expected exactly 1 event, got {}", events.len());

        match &events[0] {
            SessionEvent::NoteParsed { path, block_count, payload } => {
                assert_eq!(path, &file_path);
                assert!(*block_count > 0, "Expected at least 1 block");
                assert!(payload.is_some(), "Expected payload to be present");
            }
            other => panic!("Expected NoteParsed event, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_handle_file_changed_skips_non_markdown() {
        let (mock, emitter) = create_mock_emitter();
        let handler = ParserHandler::with_emitter(emitter);

        // Create a temp directory with a non-markdown file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "Some text content").await.unwrap();

        // Handle the file change
        handler.handle_file_changed(&file_path).await;

        // Verify no event was emitted
        let events = mock.emitted_events();
        assert!(events.is_empty(), "Expected no events for non-markdown file");
    }

    #[tokio::test]
    async fn test_handle_file_changed_skips_nonexistent() {
        let (mock, emitter) = create_mock_emitter();
        let handler = ParserHandler::with_emitter(emitter);

        // Use a path that doesn't exist
        let file_path = Path::new("/nonexistent/path/to/file.md");

        // Handle the file change
        handler.handle_file_changed(file_path).await;

        // Verify no event was emitted
        let events = mock.emitted_events();
        assert!(events.is_empty(), "Expected no events for nonexistent file");
    }

    #[tokio::test]
    async fn test_handle_event_dispatches_file_changed() {
        let (mock, emitter) = create_mock_emitter();
        let handler = ParserHandler::with_emitter(emitter);

        // Create a temp directory with a markdown file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("dispatch_test.md");
        fs::write(&file_path, "# Dispatch Test\n\nContent.\n").await.unwrap();

        // Create a FileChanged event
        let event = SessionEvent::FileChanged {
            path: file_path.clone(),
            kind: crucible_core::events::FileChangeKind::Modified,
        };

        // Handle the event
        handler.handle_event(&event).await;

        // Verify NoteParsed was emitted
        let events = mock.emitted_events();
        assert!(
            events.iter().any(|e| matches!(e, SessionEvent::NoteParsed { .. })),
            "FileChanged should trigger NoteParsed emission"
        );
    }

    #[tokio::test]
    async fn test_handle_event_ignores_other_events() {
        let (mock, emitter) = create_mock_emitter();
        let handler = ParserHandler::with_emitter(emitter);

        // Create events that should be ignored
        let events_to_ignore = vec![
            SessionEvent::FileDeleted {
                path: Path::new("/test.md").to_path_buf(),
            },
            SessionEvent::EntityStored {
                entity_id: "test".to_string(),
                entity_type: crucible_core::events::EntityType::Note,
            },
            SessionEvent::ToolCalled {
                name: "test".to_string(),
                args: serde_json::json!({}),
            },
        ];

        for event in events_to_ignore {
            handler.handle_event(&event).await;
        }

        // Verify no events were emitted
        let emitted = mock.emitted_events();
        assert!(emitted.is_empty(), "Should not emit events for non-FileChanged events");
    }

    #[tokio::test]
    async fn test_count_blocks() {
        let (_, emitter) = create_mock_emitter();
        let handler = ParserHandler::with_emitter(emitter);

        // Create a temp file with known structure
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("blocks.md");
        let content = r#"# Heading 1

A paragraph.

## Heading 2

Another paragraph.

```rust
let code = "block";
```

- List item 1
- List item 2

> A blockquote
"#;
        fs::write(&file_path, content).await.unwrap();

        // Parse and count
        let parsed = handler.parser().parse_file(&file_path).await.unwrap();
        let count = ParserHandler::count_blocks(&parsed);

        // Should have: 2 headings + 2 paragraphs + 1 code block + 1 list + 1 blockquote = 7
        assert!(count >= 5, "Expected at least 5 blocks, got {}", count);
    }
}
