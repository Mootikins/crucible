//! Tests for FileDeleted event emission from IndexingHandler.
//!
//! These tests verify that the IndexingHandler correctly emits SessionEvent::FileDeleted
//! events when processing file deletion events.

use crucible_core::events::{Priority, SessionEvent};
use crucible_core::test_support::mocks::MockEventEmitter;
use crucible_watch::handlers::IndexingHandler;
use crucible_watch::traits::EventHandler;
use crucible_watch::{FileEvent, FileEventKind};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a mock path in the temp directory (cross-platform)
fn mock_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}

/// Test that IndexingHandler emits SessionEvent::FileDeleted for Deleted events.
#[tokio::test]
async fn test_file_deleted_emission() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    // For deleted files, the file doesn't need to exist - just simulate the event
    let deleted_path = mock_path("deleted_note.md");

    let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::FileDeleted { path } => {
            assert_eq!(path, &deleted_path);
        }
        other => panic!("Expected FileDeleted event, got: {:?}", other),
    }
}

/// Test that FileDeleted events have Low priority.
#[tokio::test]
async fn test_file_deleted_event_priority() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let deleted_path = mock_path("deleted_note.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    // FileDeleted events should have Low priority (cleanup can wait)
    assert_eq!(
        emitted_events[0].priority(),
        Priority::Low,
        "FileDeleted events should have Low priority"
    );
}

/// Test FileDeleted emission with various file extensions.
#[tokio::test]
async fn test_file_deleted_emission_various_extensions() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let extensions = vec!["md", "txt", "rst", "adoc"];

    for ext in extensions {
        emitter.reset();

        let deleted_path = mock_path(&format!("deleted_note.{}", ext));
        let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

        handler.handle(file_event).await.expect("Handler failed");

        let emitted_events = emitter.emitted_events();
        assert_eq!(
            emitted_events.len(),
            1,
            "Expected one FileDeleted event for .{} file",
            ext
        );

        match &emitted_events[0] {
            SessionEvent::FileDeleted { path } => {
                assert_eq!(path, &deleted_path, "Path mismatch for .{} file", ext);
            }
            other => panic!("Expected FileDeleted event for .{}, got: {:?}", ext, other),
        }
    }
}

/// Test that FileDeleted events are emitted for non-existent paths.
/// Unlike Created/Modified events, Deleted events don't require the file to exist.
#[tokio::test]
async fn test_file_deleted_emission_nonexistent_file() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    // Use a path that definitely doesn't exist
    let nonexistent_path = PathBuf::from("/nonexistent/directory/deleted_file.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, nonexistent_path.clone());

    // Handler should still emit the event successfully
    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(
        emitted_events.len(),
        1,
        "FileDeleted should be emitted even for non-existent paths"
    );

    match &emitted_events[0] {
        SessionEvent::FileDeleted { path } => {
            assert_eq!(path, &nonexistent_path);
        }
        other => panic!("Expected FileDeleted event, got: {:?}", other),
    }
}

/// Test multiple consecutive FileDeleted events.
#[tokio::test]
async fn test_multiple_file_deleted_events() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let paths = vec![
        mock_path("deleted1.md"),
        mock_path("deleted2.md"),
        mock_path("nested/deleted3.md"),
    ];

    for path in &paths {
        let file_event = FileEvent::new(FileEventKind::Deleted, path.clone());
        handler.handle(file_event).await.expect("Handler failed");
    }

    let emitted_events = emitter.emitted_events();
    assert_eq!(
        emitted_events.len(),
        paths.len(),
        "Expected {} FileDeleted events",
        paths.len()
    );

    for (i, emitted_event) in emitted_events.iter().enumerate() {
        match emitted_event {
            SessionEvent::FileDeleted { path } => {
                assert_eq!(path, &paths[i], "Path mismatch at index {}", i);
            }
            other => panic!("Expected FileDeleted at index {}, got: {:?}", i, other),
        }
    }
}

/// Test FileDeleted emission with absolute path.
#[tokio::test]
async fn test_file_deleted_emission_absolute_path() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let absolute_path = PathBuf::from("/home/user/vault/notes/important.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, absolute_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::FileDeleted { path } => {
            assert!(path.is_absolute(), "Path should remain absolute");
            assert_eq!(path, &absolute_path);
        }
        other => panic!("Expected FileDeleted event, got: {:?}", other),
    }
}

/// Test FileDeleted emission with Unicode filename.
#[tokio::test]
async fn test_file_deleted_emission_unicode_filename() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let unicode_path = mock_path("删除的笔记.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, unicode_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::FileDeleted { path } => {
            assert_eq!(path, &unicode_path, "Unicode path should be preserved");
        }
        other => panic!("Expected FileDeleted event, got: {:?}", other),
    }
}

/// Test FileDeleted emission with spaces in filename.
#[tokio::test]
async fn test_file_deleted_emission_spaces_in_path() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let spaced_path = mock_path("My Documents/My Note.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, spaced_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::FileDeleted { path } => {
            assert_eq!(path, &spaced_path, "Path with spaces should be preserved");
        }
        other => panic!("Expected FileDeleted event, got: {:?}", other),
    }
}

/// Test that FileDeleted is a file event.
#[tokio::test]
async fn test_file_deleted_is_file_event() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let deleted_path = mock_path("deleted.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    // FileDeleted should be categorized as a file event
    assert!(
        emitted_events[0].is_file_event(),
        "FileDeleted should be categorized as a file event"
    );
}

/// Test that FileDeleted is NOT a note event.
/// Note events are for parsed content (NoteParsed, NoteCreated, NoteModified).
/// File events are raw filesystem changes.
#[tokio::test]
async fn test_file_deleted_is_not_note_event() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let deleted_path = mock_path("deleted.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    // FileDeleted should NOT be categorized as a note event
    assert!(
        !emitted_events[0].is_note_event(),
        "FileDeleted should NOT be categorized as a note event"
    );
}

/// Test FileDeleted event_type string.
#[tokio::test]
async fn test_file_deleted_event_type() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let deleted_path = mock_path("deleted.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    assert_eq!(
        emitted_events[0].event_type(),
        "file_deleted",
        "FileDeleted event_type should be 'file_deleted'"
    );
}

/// Test FileDeleted identifier (used for pattern matching).
#[tokio::test]
async fn test_file_deleted_identifier() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let deleted_path = PathBuf::from("/notes/deleted.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    // The identifier should be the path display string
    assert_eq!(
        emitted_events[0].identifier(),
        "/notes/deleted.md",
        "FileDeleted identifier should be the path"
    );
}

/// Test that FileDeleted events work with files that were created and then deleted
/// within the same test (simulating a real deletion scenario).
#[tokio::test]
async fn test_file_deleted_after_creation() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    // Create a temp directory with a test file
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("to_be_deleted.md");

    // Create the file
    std::fs::write(&test_file, "# Note to be deleted").unwrap();
    assert!(test_file.exists());

    // Process Created event
    let created_event = FileEvent::new(FileEventKind::Created, test_file.clone());
    handler.handle(created_event).await.expect("Handler failed");

    // Now delete the file
    std::fs::remove_file(&test_file).unwrap();
    assert!(!test_file.exists());

    // Process Deleted event
    let deleted_event = FileEvent::new(FileEventKind::Deleted, test_file.clone());
    handler.handle(deleted_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(
        emitted_events.len(),
        2,
        "Expected two events: Created and Deleted"
    );

    // First event should be FileChanged(Created)
    match &emitted_events[0] {
        SessionEvent::FileChanged { path, kind } => {
            assert_eq!(path, &test_file);
            assert_eq!(*kind, crucible_core::events::FileChangeKind::Created);
        }
        other => panic!("Expected FileChanged(Created), got: {:?}", other),
    }

    // Second event should be FileDeleted
    match &emitted_events[1] {
        SessionEvent::FileDeleted { path } => {
            assert_eq!(path, &test_file);
        }
        other => panic!("Expected FileDeleted, got: {:?}", other),
    }
}

/// Test that FileDeleted events include the full path information.
#[tokio::test]
async fn test_file_deleted_preserves_full_path() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let deep_path =
        PathBuf::from("/home/user/Documents/vault/nested/folder/structure/important-note.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, deep_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::FileDeleted { path } => {
            // Verify the full path is preserved
            assert_eq!(path, &deep_path);
            assert_eq!(
                path.file_name().unwrap().to_str().unwrap(),
                "important-note.md"
            );
            assert!(path.to_string_lossy().contains("nested/folder/structure"));
        }
        other => panic!("Expected FileDeleted event, got: {:?}", other),
    }
}

/// Test FileDeleted emission with mock emitter cancellation behavior.
/// This tests that our handler properly handles the event emission outcome.
#[tokio::test]
async fn test_file_deleted_emission_with_cancelled_outcome() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

    // Configure emitter to cancel events
    emitter.set_cancel_events(true);

    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let deleted_path = mock_path("deleted.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

    // Handler should still complete successfully even if the event was cancelled
    handler.handle(file_event).await.expect("Handler failed");

    // Event should still be recorded by the mock
    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);
}

/// Test FileDeleted serialization/deserialization.
#[tokio::test]
async fn test_file_deleted_serialization() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let deleted_path = PathBuf::from("/notes/serialization-test.md");
    let file_event = FileEvent::new(FileEventKind::Deleted, deleted_path.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    // Test serialization round-trip
    let json = serde_json::to_string(&emitted_events[0]).unwrap();
    assert!(json.contains("file_deleted"), "JSON should contain type");
    assert!(
        json.contains("serialization-test.md"),
        "JSON should contain path"
    );

    let deserialized: SessionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(emitted_events[0], deserialized);
}
