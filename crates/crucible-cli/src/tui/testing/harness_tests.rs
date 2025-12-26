//! Integration tests for the TUI test harness
//!
//! These tests demonstrate harness usage patterns and verify
//! cross-component behavior.

use super::fixtures::{events, registries, sessions};
use super::Harness;
use crate::tui::state::PopupKind;
use crossterm::event::KeyCode;

/// Test: Popup navigation works correctly
#[test]
fn popup_navigation() {
    let mut h = Harness::new(80, 24).with_popup_items(PopupKind::Command, registries::standard_commands());

    // Initially selected index is 0
    assert_eq!(h.popup_selected(), Some(0));

    // Down arrow moves selection
    h.key(KeyCode::Down);
    assert_eq!(h.popup_selected(), Some(1));

    h.key(KeyCode::Down);
    assert_eq!(h.popup_selected(), Some(2));

    // Up arrow moves back
    h.key(KeyCode::Up);
    assert_eq!(h.popup_selected(), Some(1));
}

/// Test: Popup query accumulates typed characters
#[test]
fn popup_query_typing() {
    let mut h = Harness::new(80, 24);

    // Open command popup
    h.key(KeyCode::Char('/'));
    assert!(h.has_popup());
    assert_eq!(h.popup_query(), Some(""));

    // Type query
    h.keys("sea");
    assert_eq!(h.popup_query(), Some("sea"));

    // Backspace removes character
    h.key(KeyCode::Backspace);
    assert_eq!(h.popup_query(), Some("se"));
}

/// Test: Selecting popup item inserts token
#[test]
fn popup_selection_inserts_token() {
    let mut h = Harness::new(80, 24).with_popup_items(PopupKind::Command, registries::minimal_commands());

    // Select first item (search)
    h.key(KeyCode::Enter);

    // Popup closes, token inserted
    assert!(!h.has_popup());
    assert!(h.input_text().contains("search"));
}

/// Test: Input editing with readline keybindings
#[test]
fn readline_keybindings() {
    let mut h = Harness::new(80, 24);

    h.keys("hello world test");
    assert_eq!(h.cursor_position(), 16);

    // Ctrl+A moves to start
    h.key_ctrl('a');
    assert_eq!(h.cursor_position(), 0);

    // Ctrl+E moves to end
    h.key_ctrl('e');
    assert_eq!(h.cursor_position(), 16);

    // Ctrl+W deletes word backward
    h.key_ctrl('w');
    assert_eq!(h.input_text(), "hello world ");

    // Ctrl+K deletes to end (from current position)
    h.key_ctrl('a'); // go to start
    h.key(KeyCode::Right); // move past 'h'
    h.key(KeyCode::Right); // move past 'e'
    h.key(KeyCode::Right); // move past 'l'
    h.key(KeyCode::Right); // move past 'l'
    h.key(KeyCode::Right); // move past 'o'
    h.key_ctrl('k');
    assert_eq!(h.input_text(), "hello");
}

/// Test: Streaming events update view state
#[test]
fn streaming_events_flow() {
    let mut h = Harness::new(80, 24);

    // Inject streaming chunks
    let chunks = events::streaming_chunks("Hello from the agent!");
    h.events(chunks);

    // No error should occur
    assert!(!h.has_error());
}

/// Test: Error events set error state
#[test]
fn error_events_set_state() {
    let mut h = Harness::new(80, 24);

    // Inject error event sequence
    let error_events = events::streaming_error("Partial response", "Connection lost");
    h.events(error_events);

    // Error should be set
    assert!(h.has_error());
    assert_eq!(h.error(), Some("Connection lost"));
}

/// Test: Session fixture loads correctly
#[test]
fn session_fixtures_load() {
    let h = Harness::new(80, 24).with_session(sessions::with_tool_calls());

    // Should have 4 items: user, assistant, tool, assistant
    assert_eq!(h.conversation_len(), 4);
}

/// Test: Render produces non-empty output
#[test]
fn render_produces_output() {
    let h = Harness::new(80, 24).with_session(sessions::basic_exchange());

    let output = h.render();

    // Output should not be empty
    assert!(!output.is_empty());
    // Output should have expected dimensions (80 chars wide + newlines)
    let lines: Vec<&str> = output.lines().collect();
    assert!(lines.len() <= 24);
}

/// Test: Multiple interactions in sequence
#[test]
fn complex_interaction_sequence() {
    let mut h = Harness::new(80, 24);

    // Type a message
    h.keys("What is Crucible?");
    assert_eq!(h.input_text(), "What is Crucible?");

    // Clear with Ctrl+U
    h.key_ctrl('u');
    assert_eq!(h.input_text(), "");

    // Open popup, type, cancel
    h.key(KeyCode::Char('/'));
    assert!(h.has_popup());
    h.keys("he");
    h.key(KeyCode::Esc);
    assert!(!h.has_popup());
    // Input should still have the /
    assert_eq!(h.input_text(), "/");

    // Clear and start fresh
    h.key_ctrl('u');
    h.keys("New message");
    assert_eq!(h.input_text(), "New message");
}
