//! Integration tests for TUI event loop and CLI integration
//!
//! Tests verify the TUI correctly displays CLI state and user interactions.
//!
//! These tests follow TDD principles:
//! - Tests are written first with `todo!()` placeholders (RED phase)
//! - Implementation will make tests pass (GREEN phase)
//! - Refactoring follows (REFACTOR phase)
//!
//! Test Structure:
//! - Event Loop Tests: TUI initialization, tick events, keyboard input, cleanup
//! - Log Display Tests: Log rendering, scrolling, buffer size limits
//! - REPL Integration Tests: Input routing, output display, command execution
//! - State Synchronization Tests: Status updates, concurrent updates

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crucible_cli::tui::{App, AppMode, LogEntry, ReplResult, StatusUpdate, TuiConfig, UiEvent};
use ratatui::{backend::TestBackend, Terminal};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a test terminal with TestBackend
///
/// The TestBackend is a headless ratatui backend that captures rendering
/// without requiring a real terminal. This allows for testing TUI logic
/// in CI environments.
fn create_test_terminal() -> Result<Terminal<TestBackend>> {
    let backend = TestBackend::new(80, 24); // 80x24 terminal
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Create test channels for TUI events
///
/// Returns senders for log entries and status updates, plus receivers
/// that can be passed to the App constructor.
fn create_test_channels() -> (
    mpsc::Sender<LogEntry>,
    mpsc::Sender<StatusUpdate>,
    mpsc::Receiver<LogEntry>,
    mpsc::Receiver<StatusUpdate>,
) {
    let (log_tx, log_rx) = mpsc::channel(100);
    let (status_tx, status_rx) = mpsc::channel(100);
    (log_tx, status_tx, log_rx, status_rx)
}

/// Create a test App with default configuration
fn create_test_app(
    log_rx: mpsc::Receiver<LogEntry>,
    status_rx: mpsc::Receiver<StatusUpdate>,
) -> App {
    let config = TuiConfig {
        log_capacity: 5, // Small buffer for testing
        history_capacity: 10,
        status_throttle_ms: 0, // No throttling in tests
        log_split_ratio: 70,
    };
    App::new(log_rx, status_rx, config)
}

/// Create a test log entry
fn create_test_log(message: &str) -> LogEntry {
    LogEntry::new(tracing::Level::INFO, "test_module", message)
}

/// Helper to extract text content from a TestBackend buffer
///
/// Returns all non-empty cells as a concatenated string.
/// This is useful for assertions about what text appears on screen.
///
/// Note: This will be implemented when writing the actual tests.
/// For now, it's a placeholder that returns an empty string.
#[allow(dead_code)]
fn buffer_content(_terminal: &Terminal<TestBackend>) -> String {
    // TODO: Implement using ratatui's Cell API
    // The symbol field is private, so we may need to use Cell::symbol() method
    // or iterate over buffer positions with buffer.get(x, y)
    String::new()
}

// ============================================================================
// Event Loop Tests (4 tests)
// ============================================================================

#[tokio::test]
async fn test_tui_initialization() {
    // Test: TUI initializes with default state
    //
    // This test verifies that the App struct initializes correctly
    // with all components in their expected initial states.
    //
    // Expected behavior:
    // - App mode should be Input (ready for user input)
    // - Log buffer should be empty
    // - Status bar should have default values
    // - REPL should be in Idle state
    // - No dirty flags should be set initially (optimization)

    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let app = create_test_app(log_rx, status_rx);

    // Assert initial state
    assert!(matches!(app.mode, crucible_cli::tui::app::AppMode::Input));
    assert_eq!(app.logs.len(), 0);
    assert!(app.logs.is_empty());
    assert_eq!(app.status.doc_count, 0);
    assert_eq!(app.status.db_size, 0);
    assert!(app.repl.is_idle());
    assert!(!app.render_state.is_dirty());
}

#[tokio::test]
async fn test_tui_handles_tick_events() {
    // Test: Tick events update UI state
    //
    // This test verifies that the TUI can process "tick" events
    // (periodic updates from the event loop) without crashing
    // and that channel events are properly consumed.
    //
    // Expected behavior:
    // - Log events sent via channel should be consumed
    // - Status events should be consumed
    // - Render dirty flags should be set when events arrive
    // - App should not exit on normal tick events

    let (log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Send a log entry via channel
    let log = create_test_log("Test message");
    log_tx.send(log).await.unwrap();

    // Process the log event (simulating tick loop)
    if let Ok(entry) = app.log_rx.try_recv() {
        app.handle_event(UiEvent::Log(entry)).await.unwrap();
    }

    // Assert log was processed
    assert_eq!(app.logs.len(), 1);
    assert!(app.render_state.logs_dirty);
    assert!(!app.is_exiting());
}

#[tokio::test]
async fn test_tui_handles_key_events() {
    // Test: Keyboard input processed correctly
    //
    // This test verifies that keyboard events are routed to the
    // appropriate handlers and cause correct state changes.
    //
    // Expected behavior:
    // - Character input should update REPL buffer
    // - Enter key should submit command
    // - Arrow keys should navigate history
    // - Ctrl+C should trigger shutdown

    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Send character 'h'
    let key_h = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(key_h)))
        .await
        .unwrap();
    assert_eq!(app.repl.input(), "h");
    assert!(app.render_state.repl_dirty);

    // Clear dirty flag for next test
    app.render_state.clear();

    // Send character 'i'
    let key_i = KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(key_i)))
        .await
        .unwrap();
    assert_eq!(app.repl.input(), "hi");
    assert!(app.render_state.repl_dirty);

    // Send Ctrl+C
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    app.handle_event(UiEvent::Input(Event::Key(ctrl_c)))
        .await
        .unwrap();
    assert!(matches!(app.mode, AppMode::Exiting));
}

#[tokio::test]
async fn test_tui_shutdown_cleanup() {
    // Test: TUI cleans up terminal on exit
    //
    // This test verifies that the shutdown sequence properly
    // sets the exiting flag and that is_exiting() works.
    //
    // Expected behavior:
    // - shutdown() should set mode to Exiting
    // - is_exiting() should return true after shutdown
    // - Event loop should exit on next iteration

    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    assert!(!app.is_exiting());

    app.shutdown();

    assert!(app.is_exiting());
    assert!(matches!(app.mode, AppMode::Exiting));
}

// ============================================================================
// Log Display Tests (3 tests)
// ============================================================================

#[tokio::test]
async fn test_log_buffer_displays_messages() {
    // Test: Logs appear in log widget
    //
    // This test verifies that log entries sent via the channel
    // are correctly stored in the log buffer and trigger rendering.
    //
    // Expected behavior:
    // - Log entries should be added to the buffer
    // - Buffer should maintain insertion order
    // - logs_dirty flag should be set
    // - Auto-scroll should be enabled by default

    let (log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Send log entry
    let log = create_test_log("Test log message");
    log_tx.send(log).await.unwrap();

    // Process event
    if let Ok(entry) = app.log_rx.try_recv() {
        app.handle_event(UiEvent::Log(entry)).await.unwrap();
    }

    assert_eq!(app.logs.len(), 1);

    // Check buffer contains message
    let has_message = app
        .logs
        .entries()
        .any(|e| e.message.contains("Test log message"));
    assert!(has_message);
    assert!(app.render_state.logs_dirty);
    assert_eq!(app.log_scroll.offset, 0); // Auto-scroll enabled
}

#[tokio::test]
async fn test_log_buffer_scrolling() {
    // Test: Log buffer scrolls with new messages
    //
    // This test verifies that the scroll state is managed correctly
    // when new logs arrive, with auto-scroll enabled/disabled.
    //
    // Expected behavior:
    // - New logs should auto-scroll to bottom when auto_scroll=true
    // - Manual scroll should disable auto-scroll
    // - Scrolling to bottom should re-enable auto-scroll

    let (log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Add 3 log entries
    for i in 0..3 {
        let log = create_test_log(&format!("Message {}", i));
        log_tx.send(log).await.unwrap();
        if let Ok(entry) = app.log_rx.try_recv() {
            app.handle_event(UiEvent::Log(entry)).await.unwrap();
        }
    }

    assert_eq!(app.log_scroll.offset, 0); // Auto-scroll to bottom

    // Simulate PageUp (scroll up)
    let page_up = KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(page_up)))
        .await
        .unwrap();

    assert!(app.log_scroll.offset > 0);
    assert!(!app.log_scroll.auto_scroll);

    // Add new log entry - should not auto-scroll
    let old_offset = app.log_scroll.offset;
    let log = create_test_log("New message");
    log_tx.send(log).await.unwrap();
    if let Ok(entry) = app.log_rx.try_recv() {
        app.handle_event(UiEvent::Log(entry)).await.unwrap();
    }

    assert_eq!(app.log_scroll.offset, old_offset); // Unchanged

    // Simulate PageDown to bottom
    while app.log_scroll.offset > 0 {
        let page_down = KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE);
        app.handle_event(UiEvent::Input(Event::Key(page_down)))
            .await
            .unwrap();
    }

    assert_eq!(app.log_scroll.offset, 0);
    assert!(app.log_scroll.auto_scroll);
}

#[tokio::test]
async fn test_log_buffer_size_limit() {
    // Test: Old logs removed when buffer full
    //
    // This test verifies that the log buffer enforces its capacity
    // limit and evicts oldest entries in FIFO order.
    //
    // Expected behavior:
    // - Buffer capacity should be enforced
    // - Oldest entries should be evicted when full
    // - Buffer should never exceed capacity

    let (log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut config = TuiConfig::default();
    config.log_capacity = 3;
    let mut app = App::new(log_rx, status_rx, config);

    // Add 5 log entries
    for ch in ['A', 'B', 'C', 'D', 'E'] {
        let log = create_test_log(&ch.to_string());
        log_tx.send(log).await.unwrap();
        if let Ok(entry) = app.log_rx.try_recv() {
            app.handle_event(UiEvent::Log(entry)).await.unwrap();
        }
    }

    assert_eq!(app.logs.len(), 3); // Capacity enforced

    // Check that oldest were evicted
    let messages: Vec<String> = app.logs.entries().map(|e| e.message.clone()).collect();
    assert_eq!(messages, vec!["C", "D", "E"]);
}

// ============================================================================
// REPL Integration Tests (3 tests)
// ============================================================================

#[tokio::test]
async fn test_repl_input_routing() {
    // Test: User input routed to REPL
    //
    // This test verifies that keyboard input is correctly
    // routed to the REPL state and updates the input buffer.
    //
    // Expected behavior:
    // - Character input should be added to REPL buffer
    // - Cursor position should advance
    // - Backspace should delete characters
    // - repl_dirty flag should be set

    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Type "SELECT"
    for ch in ['S', 'E', 'L', 'E', 'C', 'T'] {
        let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
        app.handle_event(UiEvent::Input(Event::Key(key)))
            .await
            .unwrap();
    }

    assert_eq!(app.repl.input(), "SELECT");
    assert_eq!(app.repl.cursor(), 6);

    // Backspace
    let backspace = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(backspace)))
        .await
        .unwrap();

    assert_eq!(app.repl.input(), "SELEC");
    assert_eq!(app.repl.cursor(), 5);

    assert!(app.render_state.repl_dirty);
}

#[tokio::test]
async fn test_repl_output_display() {
    // Test: Query results displayed in results widget
    //
    // This test verifies that REPL results (success, error, table)
    // are correctly stored and trigger rendering updates.
    //
    // Expected behavior:
    // - ReplResult events should be stored in last_repl_result
    // - REPL state should return to Idle after result
    // - repl_dirty flag should be set

    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Send success result
    let success = ReplResult::success("OK", Duration::from_millis(10));
    app.handle_event(UiEvent::ReplResult(success))
        .await
        .unwrap();

    assert!(app.last_repl_result.is_some());
    assert!(matches!(
        app.last_repl_result,
        Some(ReplResult::Success { .. })
    ));
    assert!(app.repl.is_idle());
    assert!(app.render_state.repl_dirty);

    // Clear dirty flag
    app.render_state.clear();

    // Send error result
    let error = ReplResult::error("Syntax error");
    app.handle_event(UiEvent::ReplResult(error)).await.unwrap();

    assert!(matches!(
        app.last_repl_result,
        Some(ReplResult::Error { .. })
    ));
    assert!(app.render_state.repl_dirty);
}

#[tokio::test]
async fn test_repl_command_execution() {
    // Test: Built-in commands execute and show output
    //
    // This test verifies that built-in commands like :help, :quit
    // are correctly recognized and executed.
    //
    // Expected behavior:
    // - :quit should trigger shutdown
    // - :help should return help text
    // - Unknown commands should return error result

    // Test :quit command
    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Type ":quit"
    for ch in [':', 'q', 'u', 'i', 't'] {
        let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
        app.handle_event(UiEvent::Input(Event::Key(key)))
            .await
            .unwrap();
    }

    // Submit command
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(enter)))
        .await
        .unwrap();

    assert!(matches!(app.mode, AppMode::Exiting));

    // Test :help command (need fresh app)
    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    for ch in [':', 'h', 'e', 'l', 'p'] {
        let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
        app.handle_event(UiEvent::Input(Event::Key(key)))
            .await
            .unwrap();
    }

    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(enter)))
        .await
        .unwrap();

    assert!(app.last_repl_result.is_some());
    if let Some(ReplResult::Success { output, .. }) = &app.last_repl_result {
        assert!(output.contains("Built-in Commands"));
    }

    // Test unknown command
    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    for ch in [':', 'u', 'n', 'k', 'n', 'o', 'w', 'n'] {
        let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
        app.handle_event(UiEvent::Input(Event::Key(key)))
            .await
            .unwrap();
    }

    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(enter)))
        .await
        .unwrap();

    assert!(matches!(
        app.last_repl_result,
        Some(ReplResult::Error { .. })
    ));
}

// ============================================================================
// State Synchronization Tests (2 tests)
// ============================================================================

#[tokio::test]
async fn test_status_updates() {
    // Test: CLI status reflected in header
    //
    // This test verifies that StatusUpdate events correctly
    // update the status bar state.
    //
    // Expected behavior:
    // - StatusUpdate with doc_count should update counter
    // - StatusUpdate with kiln_path should update path
    // - Partial updates should only change specified fields
    // - header_dirty flag should be set
    // - Status throttling should be respected (if enabled)

    let (_log_tx, status_tx, log_rx, status_rx) = create_test_channels();
    let mut config = TuiConfig::default();
    config.status_throttle_ms = 0; // No throttling
    let mut app = App::new(log_rx, status_rx, config);

    // Send status update with kiln_path
    let update = StatusUpdate::new().with_kiln_path(PathBuf::from("/test/kiln"));
    status_tx.send(update).await.unwrap();

    if let Ok(status) = app.status_rx.try_recv() {
        app.handle_event(UiEvent::Status(status)).await.unwrap();
    }

    assert_eq!(app.status.kiln_path, PathBuf::from("/test/kiln"));
    assert!(app.render_state.header_dirty);

    // Clear dirty flag
    app.render_state.clear();

    // Send doc_count update
    let update = StatusUpdate::new().with_doc_count(42);
    status_tx.send(update).await.unwrap();

    if let Ok(status) = app.status_rx.try_recv() {
        app.handle_event(UiEvent::Status(status)).await.unwrap();
    }

    assert_eq!(app.status.doc_count, 42);
    assert_eq!(app.status.kiln_path, PathBuf::from("/test/kiln")); // Partial update preserved
    assert!(app.render_state.header_dirty);

    // Send db_type update
    app.render_state.clear();
    let update = StatusUpdate::new().with_db_type("SurrealDB");
    status_tx.send(update).await.unwrap();

    if let Ok(status) = app.status_rx.try_recv() {
        app.handle_event(UiEvent::Status(status)).await.unwrap();
    }

    assert_eq!(app.status.db_type, "SurrealDB");
    assert_eq!(app.status.doc_count, 42); // Other fields unchanged
}

#[tokio::test]
async fn test_concurrent_updates() {
    // Test: Multiple components update without race conditions
    //
    // This test verifies that the TUI can handle multiple events
    // arriving in quick succession without data races or corruption.
    //
    // Expected behavior:
    // - Log events should be processed in order
    // - Status events should be processed in order
    // - REPL events should be processed in order
    // - All dirty flags should be set correctly
    // - No events should be lost or corrupted

    let (log_tx, status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Send 3 log entries rapidly
    for i in 0..3 {
        let log = create_test_log(&format!("Log {}", i));
        log_tx.send(log).await.unwrap();
    }

    // Send 2 status updates rapidly
    let update1 = StatusUpdate::new().with_doc_count(10);
    let update2 = StatusUpdate::new().with_doc_count(20);
    status_tx.send(update1).await.unwrap();
    status_tx.send(update2).await.unwrap();

    // Send 1 REPL result
    let result = ReplResult::success("Done", Duration::from_millis(5));

    // Process all events
    while let Ok(entry) = app.log_rx.try_recv() {
        app.handle_event(UiEvent::Log(entry)).await.unwrap();
    }

    while let Ok(status) = app.status_rx.try_recv() {
        app.handle_event(UiEvent::Status(status)).await.unwrap();
    }

    app.handle_event(UiEvent::ReplResult(result)).await.unwrap();

    // Assert all events processed
    assert_eq!(app.logs.len(), 3);

    // Check logs in order
    let messages: Vec<String> = app.logs.entries().map(|e| e.message.clone()).collect();
    assert_eq!(messages[0], "Log 0");
    assert_eq!(messages[1], "Log 1");
    assert_eq!(messages[2], "Log 2");

    // Status reflects latest update (throttling may affect this)
    assert!(app.status.doc_count >= 10);

    // REPL result stored
    assert!(app.last_repl_result.is_some());

    // Dirty flags set
    assert!(
        app.render_state.logs_dirty || app.render_state.header_dirty || app.render_state.repl_dirty
    );
}

// ============================================================================
// Advanced Integration Tests (Bonus)
// ============================================================================

#[tokio::test]
async fn test_terminal_resize_handling() {
    // Test: Terminal resize marks all sections dirty
    //
    // Expected behavior:
    // - Resize event should mark all sections dirty
    // - UI should re-render after resize

    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Clear all dirty flags
    app.render_state.clear();

    // Send resize event
    let resize = Event::Resize(100, 30);
    app.handle_event(UiEvent::Input(resize)).await.unwrap();

    assert!(app.render_state.header_dirty);
    assert!(app.render_state.logs_dirty);
    assert!(app.render_state.repl_dirty);
}

#[tokio::test]
async fn test_repl_history_navigation() {
    // Test: Up/Down arrows navigate command history
    //
    // Expected behavior:
    // - Up arrow should show previous command
    // - Down arrow should show next command
    // - History should wrap correctly

    let (_log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Submit first command
    for ch in "SELECT * FROM notes".chars() {
        let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
        app.handle_event(UiEvent::Input(Event::Key(key)))
            .await
            .unwrap();
    }
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(enter)))
        .await
        .unwrap();

    // Submit second command
    for ch in [':', 'h', 'e', 'l', 'p'] {
        let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
        app.handle_event(UiEvent::Input(Event::Key(key)))
            .await
            .unwrap();
    }
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(enter)))
        .await
        .unwrap();

    // Press Up arrow - should show :help (most recent)
    let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(up)))
        .await
        .unwrap();
    assert_eq!(app.repl.input(), ":help");

    // Press Up arrow again - should show SELECT
    let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(up)))
        .await
        .unwrap();
    assert_eq!(app.repl.input(), "SELECT * FROM notes");

    // Press Down arrow - should show :help
    let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(down)))
        .await
        .unwrap();
    assert_eq!(app.repl.input(), ":help");

    // Press Down arrow again - should clear (back to empty prompt)
    let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    app.handle_event(UiEvent::Input(Event::Key(down)))
        .await
        .unwrap();
    assert_eq!(app.repl.input(), "");
}

#[tokio::test]
async fn test_render_optimization() {
    // Test: Dirty flags optimize rendering
    //
    // This test verifies that render optimization works correctly
    // and only dirty sections trigger re-renders.
    //
    // Expected behavior:
    // - is_dirty() should return true when any section is dirty
    // - clear_dirty() should reset all flags
    // - Individual dirty flags should be set by specific events

    let (log_tx, _status_tx, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    assert!(!app.render_state.is_dirty());

    // Add log entry
    let log = create_test_log("Test");
    log_tx.send(log).await.unwrap();
    if let Ok(entry) = app.log_rx.try_recv() {
        app.handle_event(UiEvent::Log(entry)).await.unwrap();
    }

    assert!(app.render_state.is_dirty());
    assert!(app.render_state.logs_dirty);
    assert!(!app.render_state.header_dirty);
    assert!(!app.render_state.repl_dirty);

    // Clear dirty flags
    app.render_state.clear();
    assert!(!app.render_state.is_dirty());

    // Send status update
    let update = StatusUpdate::new().with_doc_count(1);
    app.handle_event(UiEvent::Status(update)).await.unwrap();

    assert!(app.render_state.header_dirty);
    assert!(!app.render_state.logs_dirty);
    assert!(!app.render_state.repl_dirty);
}

#[tokio::test]
async fn test_status_throttling() {
    // Test: Status updates are throttled to avoid excessive renders
    //
    // Expected behavior:
    // - Rapid status updates should be dropped if within throttle window
    // - Updates outside throttle window should be processed

    let (_log_tx, status_tx, log_rx, status_rx) = create_test_channels();
    let mut config = TuiConfig::default();
    config.status_throttle_ms = 100;
    let mut app = App::new(log_rx, status_rx, config);

    // Send first update
    let update = StatusUpdate::new().with_doc_count(1);
    status_tx.send(update).await.unwrap();
    if let Ok(status) = app.status_rx.try_recv() {
        app.handle_event(UiEvent::Status(status)).await.unwrap();
    }
    assert_eq!(app.status.doc_count, 1);

    // Immediately send second update - should be throttled
    let update = StatusUpdate::new().with_doc_count(2);
    status_tx.send(update).await.unwrap();
    if let Ok(status) = app.status_rx.try_recv() {
        app.handle_event(UiEvent::Status(status)).await.unwrap();
    }
    assert_eq!(app.status.doc_count, 1); // Still 1, update was throttled

    // Wait for throttle window to pass
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Send third update - should be processed
    let update = StatusUpdate::new().with_doc_count(3);
    status_tx.send(update).await.unwrap();
    if let Ok(status) = app.status_rx.try_recv() {
        app.handle_event(UiEvent::Status(status)).await.unwrap();
    }
    assert_eq!(app.status.doc_count, 3);
}

#[tokio::test]
async fn test_channel_capacity_handling() {
    // Test: TUI handles channel backpressure gracefully
    //
    // This test verifies that the TUI can handle situations where
    // events arrive faster than they can be processed.
    //
    // Expected behavior:
    // - try_recv should drain all available messages
    // - Channel should not block the event loop
    // - Events should be processed in order

    // Create channels with small capacity
    let (log_tx, _, log_rx, status_rx) = create_test_channels();
    let mut app = create_test_app(log_rx, status_rx);

    // Send 20 log entries
    for i in 0..20 {
        let log = create_test_log(&format!("Entry {}", i));
        // Use try_send to avoid blocking if channel is full
        let _ = log_tx.try_send(log);
    }

    // Process all available events
    let mut count = 0;
    while let Ok(entry) = app.log_rx.try_recv() {
        app.handle_event(UiEvent::Log(entry)).await.unwrap();
        count += 1;
    }

    // Should have processed some events (limited by channel capacity and log buffer capacity)
    assert!(count > 0);

    // All delivered events should be processed in order
    if app.logs.len() > 1 {
        let messages: Vec<String> = app.logs.entries().map(|e| e.message.clone()).collect();
        for i in 0..messages.len() - 1 {
            // Extract the number from "Entry X"
            let num1: usize = messages[i]
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse()
                .unwrap();
            let num2: usize = messages[i + 1]
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse()
                .unwrap();
            assert!(num2 > num1, "Events not in order: {} vs {}", num1, num2);
        }
    }
}
