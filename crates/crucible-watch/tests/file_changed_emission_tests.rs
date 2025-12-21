//! Tests for FileChanged event emission from IndexingHandler.
//!
//! These tests verify that the IndexingHandler correctly emits SessionEvent::FileChanged
//! events when processing file events.

use crucible_core::events::{FileChangeKind, SessionEvent};
use crucible_core::test_support::mocks::MockEventEmitter;
use crucible_watch::handlers::IndexingHandler;
use crucible_watch::traits::EventHandler;
use crucible_watch::{FileEvent, FileEventKind};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Test that IndexingHandler emits SessionEvent::FileChanged for Created events.
#[tokio::test]
async fn test_file_changed_emission_on_created() {
    // Create a mock event emitter to capture emitted events
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());

    // Create the indexing handler with our mock emitter
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    // Create a temp directory with a test file
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.md");
    std::fs::write(&test_file, "# Test Note\n\nSome content.").unwrap();

    // Create a FileEvent for the created file using the constructor
    let file_event = FileEvent::new(FileEventKind::Created, test_file.clone());

    // Handle the event
    handler.handle(file_event).await.expect("Handler failed");

    // Verify the emitted event
    let emitted_events = emitter.emitted_events();
    assert_eq!(
        emitted_events.len(),
        1,
        "Expected exactly one emitted event, got: {:?}",
        emitted_events
    );

    match &emitted_events[0] {
        SessionEvent::FileChanged { path, kind } => {
            assert_eq!(path, &test_file, "Path mismatch");
            assert_eq!(
                *kind,
                FileChangeKind::Created,
                "Expected Created kind, got: {:?}",
                kind
            );
        }
        other => panic!("Expected FileChanged event, got: {:?}", other),
    }
}

/// Test that IndexingHandler emits SessionEvent::FileChanged for Modified events.
#[tokio::test]
async fn test_file_changed_emission_on_modified() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("existing.md");
    std::fs::write(&test_file, "# Existing Note\n\nModified content.").unwrap();

    let file_event = FileEvent::new(FileEventKind::Modified, test_file.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::FileChanged { path, kind } => {
            assert_eq!(path, &test_file);
            assert_eq!(*kind, FileChangeKind::Modified);
        }
        other => panic!("Expected FileChanged event, got: {:?}", other),
    }
}

/// Cross-platform test path helper
fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}

/// Test that IndexingHandler emits SessionEvent::FileDeleted for Deleted events.
#[tokio::test]
async fn test_file_deleted_emission() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    // For deleted files, the file doesn't need to exist - just simulate the event
    let deleted_path = test_path("deleted_note.md");

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

/// Test that IndexingHandler emits SessionEvent::FileMoved for Move events.
#[tokio::test]
async fn test_file_moved_emission() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let temp_dir = TempDir::new().unwrap();
    let from_path = temp_dir.path().join("old_name.md");
    let to_path = temp_dir.path().join("new_name.md");

    // Create the destination file (simulate the file has been moved)
    std::fs::write(&to_path, "# Moved Note").unwrap();

    let file_event = FileEvent::new(
        FileEventKind::Moved {
            from: from_path.clone(),
            to: to_path.clone(),
        },
        to_path.clone(),
    );

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::FileMoved { from, to } => {
            assert_eq!(from, &from_path);
            assert_eq!(to, &to_path);
        }
        other => panic!("Expected FileMoved event, got: {:?}", other),
    }
}

/// Test that non-markdown files still emit FileChanged events when they pass the filter.
#[tokio::test]
async fn test_file_changed_emission_for_supported_extension() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let temp_dir = TempDir::new().unwrap();
    let txt_file = temp_dir.path().join("note.txt");
    std::fs::write(&txt_file, "Plain text content").unwrap();

    let file_event = FileEvent::new(FileEventKind::Created, txt_file.clone());

    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    // .txt is in the supported extensions list
    assert_eq!(emitted_events.len(), 1);

    match &emitted_events[0] {
        SessionEvent::FileChanged { path, kind } => {
            assert_eq!(path, &txt_file);
            assert_eq!(*kind, FileChangeKind::Created);
        }
        other => panic!("Expected FileChanged event, got: {:?}", other),
    }
}

/// Test that unsupported file types do NOT emit FileChanged events.
/// The IndexingHandler filters unsupported extensions via should_process_file_event()
/// which returns false for non-supported extensions, causing early return.
#[tokio::test]
async fn test_no_emission_for_unsupported_files() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let temp_dir = TempDir::new().unwrap();
    let log_file = temp_dir.path().join("debug.log");
    std::fs::write(&log_file, "Log content").unwrap();

    let file_event = FileEvent::new(FileEventKind::Created, log_file.clone());

    // The handler should skip unsupported files and NOT emit events
    // This is because should_process_file_event() returns false for .log files
    handler.handle(file_event).await.expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    // No events should be emitted for unsupported file types
    assert_eq!(
        emitted_events.len(),
        0,
        "No events should be emitted for unsupported extensions, got: {:?}",
        emitted_events
    );
}

/// Test that directories do not emit FileChanged events (can_handle returns false).
#[tokio::test]
async fn test_no_emission_for_directories() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let temp_dir = TempDir::new().unwrap();
    let sub_dir = temp_dir.path().join("subdirectory");
    std::fs::create_dir(&sub_dir).unwrap();

    // Create a FileEvent - FileEvent::new() sets is_dir based on path.is_dir()
    let file_event = FileEvent::new(FileEventKind::Created, sub_dir.clone());
    // Verify is_dir is correctly set
    assert!(file_event.is_dir, "Directory should have is_dir = true");

    // can_handle should return false for directories
    assert!(
        !handler.can_handle(&file_event),
        "Handler should not handle directories"
    );
}

/// Test multiple sequential file events emit corresponding SessionEvents.
#[tokio::test]
async fn test_multiple_file_events_emit_multiple_session_events() {
    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let temp_dir = TempDir::new().unwrap();

    // Create multiple files
    let file1 = temp_dir.path().join("note1.md");
    let file2 = temp_dir.path().join("note2.md");
    std::fs::write(&file1, "# Note 1").unwrap();
    std::fs::write(&file2, "# Note 2").unwrap();

    // Process Created event for file1
    handler
        .handle(FileEvent::new(FileEventKind::Created, file1.clone()))
        .await
        .expect("Handler failed");

    // Process Modified event for file2
    handler
        .handle(FileEvent::new(FileEventKind::Modified, file2.clone()))
        .await
        .expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    assert_eq!(
        emitted_events.len(),
        2,
        "Expected two emitted events, got: {:?}",
        emitted_events
    );

    // First event should be FileChanged(Created)
    match &emitted_events[0] {
        SessionEvent::FileChanged { path, kind } => {
            assert_eq!(path, &file1);
            assert_eq!(*kind, FileChangeKind::Created);
        }
        other => panic!("Expected FileChanged for file1, got: {:?}", other),
    }

    // Second event should be FileChanged(Modified)
    match &emitted_events[1] {
        SessionEvent::FileChanged { path, kind } => {
            assert_eq!(path, &file2);
            assert_eq!(*kind, FileChangeKind::Modified);
        }
        other => panic!("Expected FileChanged for file2, got: {:?}", other),
    }
}

/// Test that FileChanged events include correct priority.
#[tokio::test]
async fn test_file_changed_event_priority() {
    use crucible_core::events::Priority;

    let emitter: Arc<MockEventEmitter<SessionEvent>> = Arc::new(MockEventEmitter::new());
    let handler = IndexingHandler::with_emitter(emitter.clone()).expect("Failed to create handler");

    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("priority_test.md");
    std::fs::write(&test_file, "# Priority Test").unwrap();

    // Test Created event priority (should be High)
    handler
        .handle(FileEvent::new(FileEventKind::Created, test_file.clone()))
        .await
        .expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    let created_event = &emitted_events[0];
    assert_eq!(
        created_event.priority(),
        Priority::High,
        "Created events should have High priority"
    );

    // Reset and test Modified event priority (should be Normal)
    emitter.reset();
    handler
        .handle(FileEvent::new(FileEventKind::Modified, test_file.clone()))
        .await
        .expect("Handler failed");

    let emitted_events = emitter.emitted_events();
    let modified_event = &emitted_events[0];
    assert_eq!(
        modified_event.priority(),
        Priority::Normal,
        "Modified events should have Normal priority"
    );
}
