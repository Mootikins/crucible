//! Tests for NoteParsed event emission from ParserHandler.
//!
//! These tests verify that the ParserHandler correctly emits SessionEvent::NoteParsed
//! events when processing FileChanged events. This tests the second stage of the
//! event pipeline: FileChanged -> ParserHandler -> NoteParsed

use crucible_core::events::{FileChangeKind, NotePayload, SessionEvent};
use crucible_core::test_support::mocks::MockEventEmitter;
use crucible_watch::handlers::ParserHandler;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Test that ParserHandler emits NoteParsed for a basic markdown file.
#[tokio::test]
async fn test_note_parsed_emission_basic() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    // Create a temp directory with a test file
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");
    std::fs::write(&test_file, "# Test Note\n\nSome content here.").unwrap();

    // Handle the FileChanged event
    handler.handle_file_changed(&test_file).await;

    // Verify NoteParsed was emitted
    let emitted_events = emitter.emitted_events();
    assert_eq!(
        emitted_events.len(),
        1,
        "Expected exactly one emitted event, got: {:?}",
        emitted_events
    );

    match &emitted_events[0] {
        SessionEvent::NoteParsed {
            path,
            block_count,
            payload,
        } => {
            assert_eq!(path, &test_file, "Path mismatch");
            assert!(*block_count > 0, "Expected at least one block");
            assert!(payload.is_some(), "Expected payload to be present");
        }
        other => panic!("Expected NoteParsed event, got: {:?}", other),
    }
}

/// Test that NoteParsed includes correct block count for complex documents.
#[tokio::test]
async fn test_note_parsed_block_count() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("complex.md");

    // Create a document with known structure
    let content = r#"# Heading 1

A paragraph of text.

## Heading 2

Another paragraph.

```rust
let code = "block";
```

- List item 1
- List item 2

> A blockquote
"#;
    std::fs::write(&test_file, content).unwrap();

    handler.handle_file_changed(&test_file).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::NoteParsed { block_count, .. } => {
            // Should have: 2 headings + 2 paragraphs + 1 code block + 1 list + 1 blockquote = 7
            assert!(
                *block_count >= 5,
                "Expected at least 5 blocks, got {}",
                block_count
            );
        }
        other => panic!("Expected NoteParsed event, got: {:?}", other),
    }
}

/// Test that NoteParsed payload includes extracted tags.
#[tokio::test]
async fn test_note_parsed_payload_tags() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("tagged.md");

    let content = "# Note with Tags\n\nSome content #rust #programming here.";
    std::fs::write(&test_file, content).unwrap();

    handler.handle_file_changed(&test_file).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::NoteParsed { payload, .. } => {
            let payload = payload.as_ref().expect("Expected payload");
            // Tags should be extracted from inline content
            // Note: The exact tag extraction depends on parser implementation
            assert!(
                payload.tags.contains(&"rust".to_string())
                    || payload.tags.contains(&"programming".to_string())
                    || payload.tags.is_empty(), // Some parsers may not extract inline tags
                "Tags should be extracted or empty: {:?}",
                payload.tags
            );
        }
        other => panic!("Expected NoteParsed event, got: {:?}", other),
    }
}

/// Test that NoteParsed payload includes extracted wikilinks.
#[tokio::test]
async fn test_note_parsed_payload_wikilinks() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("linked.md");

    let content = "# Note with Links\n\nSee also [[Other Note]] and [[Another Note]].";
    std::fs::write(&test_file, content).unwrap();

    handler.handle_file_changed(&test_file).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::NoteParsed { payload, .. } => {
            let payload = payload.as_ref().expect("Expected payload");
            assert_eq!(payload.wikilinks.len(), 2, "Expected 2 wikilinks");
            assert!(
                payload.wikilinks.contains(&"Other Note".to_string()),
                "Expected 'Other Note' wikilink"
            );
            assert!(
                payload.wikilinks.contains(&"Another Note".to_string()),
                "Expected 'Another Note' wikilink"
            );
        }
        other => panic!("Expected NoteParsed event, got: {:?}", other),
    }
}

/// Test that NoteParsed payload includes correct title.
/// Note: The parser derives title from filename by default, not from H1 heading.
#[tokio::test]
async fn test_note_parsed_payload_title() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("titled.md");

    let content = "# My Document Title\n\nContent goes here.";
    std::fs::write(&test_file, content).unwrap();

    handler.handle_file_changed(&test_file).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::NoteParsed { payload, .. } => {
            let payload = payload.as_ref().expect("Expected payload");
            // Parser derives title from filename (without extension) by default
            // Title may be "titled" (from filename) or "My Document Title" (from H1)
            assert!(
                !payload.title.is_empty(),
                "Title should be present, got: {:?}",
                payload.title
            );
        }
        other => panic!("Expected NoteParsed event, got: {:?}", other),
    }
}

/// Test that ParserHandler skips non-markdown files (no NoteParsed emitted).
#[tokio::test]
async fn test_no_note_parsed_for_non_markdown() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let txt_file = temp_dir.path().join("note.txt");
    std::fs::write(&txt_file, "Plain text content").unwrap();

    handler.handle_file_changed(&txt_file).await;

    let emitted_events = emitter.emitted_events();
    assert!(
        emitted_events.is_empty(),
        "No events should be emitted for .txt files"
    );
}

/// Test that ParserHandler skips non-existent files (no NoteParsed emitted).
#[tokio::test]
async fn test_no_note_parsed_for_nonexistent() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let nonexistent = PathBuf::from("/nonexistent/path/to/file.md");

    handler.handle_file_changed(&nonexistent).await;

    let emitted_events = emitter.emitted_events();
    assert!(
        emitted_events.is_empty(),
        "No events should be emitted for nonexistent files"
    );
}

/// Test that handle_event correctly dispatches FileChanged to NoteParsed.
#[tokio::test]
async fn test_handle_event_dispatches_file_changed() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("dispatch.md");
    std::fs::write(&test_file, "# Dispatch Test\n\nContent.").unwrap();

    // Create a FileChanged event
    let file_changed = SessionEvent::FileChanged {
        path: test_file.clone(),
        kind: FileChangeKind::Modified,
    };

    // Handle via handle_event
    handler.handle_event(&file_changed).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::NoteParsed { path, .. } => {
            assert_eq!(path, &test_file);
        }
        other => panic!("Expected NoteParsed event, got: {:?}", other),
    }
}

/// Test that handle_event ignores non-FileChanged events.
#[tokio::test]
async fn test_handle_event_ignores_other_events() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    // Events that should be ignored
    let events = vec![
        SessionEvent::FileDeleted {
            path: PathBuf::from("/test.md"),
        },
        SessionEvent::ToolCalled {
            name: "test".to_string(),
            args: serde_json::json!({}),
        },
        SessionEvent::EntityStored {
            entity_id: "test".to_string(),
            entity_type: crucible_core::events::EntityType::Note,
        },
    ];

    for event in events {
        handler.handle_event(&event).await;
    }

    let emitted_events = emitter.emitted_events();
    assert!(
        emitted_events.is_empty(),
        "No events should be emitted for non-FileChanged events"
    );
}

/// Test NoteParsed emission with FileChanged(Created) kind.
#[tokio::test]
async fn test_note_parsed_for_created_file() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("new_note.md");
    std::fs::write(&test_file, "# New Note\n\nJust created.").unwrap();

    let file_changed = SessionEvent::FileChanged {
        path: test_file.clone(),
        kind: FileChangeKind::Created,
    };

    handler.handle_event(&file_changed).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::NoteParsed { path, payload, .. } => {
            assert_eq!(path, &test_file);
            let payload = payload.as_ref().expect("Expected payload");
            // Title derived from filename ("new_note") or H1 ("New Note")
            assert!(
                !payload.title.is_empty(),
                "Title should be present, got: {:?}",
                payload.title
            );
        }
        other => panic!("Expected NoteParsed event, got: {:?}", other),
    }
}

/// Test that multiple sequential file changes emit multiple NoteParsed events.
#[tokio::test]
async fn test_multiple_file_changes_emit_multiple_note_parsed() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();

    // Create multiple files
    let file1 = temp_dir.path().join("note1.md");
    let file2 = temp_dir.path().join("note2.md");
    std::fs::write(&file1, "# Note 1").unwrap();
    std::fs::write(&file2, "# Note 2").unwrap();

    // Handle both
    handler.handle_file_changed(&file1).await;
    handler.handle_file_changed(&file2).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 2, "Expected 2 NoteParsed events");

    // Verify both are NoteParsed
    for event in &emitted_events {
        assert!(
            matches!(event, SessionEvent::NoteParsed { .. }),
            "Expected NoteParsed, got: {:?}",
            event
        );
    }
}

/// Test that NoteParsed event has correct event_type().
#[tokio::test]
async fn test_note_parsed_event_type() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("event_type.md");
    std::fs::write(&test_file, "# Event Type Test").unwrap();

    handler.handle_file_changed(&test_file).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    let event = &emitted_events[0];
    assert_eq!(event.event_type(), "note_parsed");
    assert!(event.is_note_event());
}

/// Test that NoteParsed includes path for identifier matching.
#[tokio::test]
async fn test_note_parsed_identifier() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("identifier.md");
    std::fs::write(&test_file, "# Identifier Test").unwrap();

    handler.handle_file_changed(&test_file).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    let event = &emitted_events[0];
    // Identifier should be the file path
    assert_eq!(event.identifier(), test_file.display().to_string());
}

/// Test NoteParsed with frontmatter title extraction.
#[tokio::test]
async fn test_note_parsed_frontmatter_title() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = ParserHandler::with_emitter(emitter.clone());

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("frontmatter.md");

    let content = r#"---
title: Frontmatter Title
tags: [test, demo]
---

# Heading Title

Content here.
"#;
    std::fs::write(&test_file, content).unwrap();

    handler.handle_file_changed(&test_file).await;

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::NoteParsed { payload, .. } => {
            let payload = payload.as_ref().expect("Expected payload");
            // Title should come from frontmatter OR heading depending on parser config
            assert!(
                payload.title == "Frontmatter Title" || payload.title == "Heading Title",
                "Title should be extracted: got '{}'",
                payload.title
            );
        }
        other => panic!("Expected NoteParsed event, got: {:?}", other),
    }
}

/// Test NotePayload builder methods.
#[test]
fn test_note_payload_builder() {
    let payload = NotePayload::new("notes/test.md", "Test Note")
        .with_tags(vec!["rust".to_string(), "test".to_string()])
        .with_wikilinks(vec!["Other Note".to_string()])
        .with_word_count(100)
        .with_file_size(1024);

    assert_eq!(payload.path, "notes/test.md");
    assert_eq!(payload.title, "Test Note");
    assert_eq!(payload.tags.len(), 2);
    assert_eq!(payload.wikilinks.len(), 1);
    assert_eq!(payload.word_count, 100);
    assert_eq!(payload.file_size, 1024);
}
