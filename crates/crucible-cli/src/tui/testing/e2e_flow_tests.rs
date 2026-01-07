//! End-to-end flow snapshot tests
//!
//! These tests verify multi-step UI interactions work correctly by simulating
//! user input sequences and capturing snapshots at key points.
//!
//! Each test follows a flow pattern:
//! 1. Set up initial state
//! 2. Simulate user actions (key presses, events)
//! 3. Snapshot the result to verify visual correctness

use super::fixtures::{events, registries, sessions};
use super::Harness;
use crate::tui::state::PopupKind;
use crossterm::event::KeyCode;
use insta::assert_snapshot;

// =============================================================================
// Command Popup Flow - Type / → navigate → select
// =============================================================================

mod command_popup_flow {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    /// User types `/` to open command popup, sees it appear
    #[test]
    fn step1_slash_opens_popup() {
        // with_popup_items pre-populates the popup but doesn't set input_buffer
        // This simulates the popup already being open with commands loaded
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        assert!(h.has_popup());
        assert_snapshot!("e2e_command_step1_opened", h.render());
    }

    /// User types search query to filter commands
    #[test]
    fn step2_filter_commands() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        // Need to type with prefix to match runner behavior
        // Input buffer and popup query stay in sync
        h.keys("/sea");

        assert_eq!(h.popup_query(), Some("sea"));
        assert_eq!(h.input_text(), "/sea");
        assert_snapshot!("e2e_command_step2_filtered", h.render());
    }

    /// User navigates down to second item
    #[test]
    fn step3_navigate_selection() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        h.key(KeyCode::Down);
        h.key(KeyCode::Down);

        assert_eq!(h.popup_selected(), Some(2));
        assert_snapshot!("e2e_command_step3_navigated", h.render());
    }

    /// User presses Enter to select and insert command
    #[test]
    fn step4_select_command() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::minimal_commands());

        h.key(KeyCode::Down); // Select "help"
        h.key(KeyCode::Enter);

        assert!(!h.has_popup());
        assert!(h.input_text().starts_with("/help"));
        assert_snapshot!("e2e_command_step4_selected", h.render());
    }

    /// User presses Escape to cancel popup
    #[test]
    fn alternate_escape_cancels() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        h.key(KeyCode::Esc);

        assert!(!h.has_popup());
        assert_snapshot!("e2e_command_escape_cancels", h.render());
    }
}

// =============================================================================
// Agent/File Reference Flow - Type @ → navigate → select
// =============================================================================

mod agent_reference_flow {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    /// User types `@` to open agent/file popup
    #[test]
    fn step1_at_opens_popup() {
        // with_popup_items pre-populates the popup for testing
        let h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::mixed_agent_file_items());

        assert!(h.has_popup());
        assert_snapshot!("e2e_agent_step1_opened", h.render());
    }

    /// User navigates through mixed agents/files/notes
    #[test]
    fn step2_navigate_mixed_items() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::mixed_agent_file_items());

        // Navigate past agents into files section
        for _ in 0..4 {
            h.key(KeyCode::Down);
        }

        assert_eq!(h.popup_selected(), Some(4));
        assert_snapshot!("e2e_agent_step2_navigated", h.render());
    }

    /// User selects an agent
    #[test]
    fn step3_select_agent() {
        let mut h = Harness::new(WIDTH, HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::test_agents());

        h.key(KeyCode::Down); // Select second agent (coder)
        h.key(KeyCode::Enter);

        assert!(!h.has_popup());
        assert!(h.input_text().starts_with("@"));
        assert_snapshot!("e2e_agent_step3_selected", h.render());
    }
}

// =============================================================================
// Streaming Response Flow - User sends message → response streams in
// =============================================================================

mod streaming_response_flow {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    /// Initial state with user message, assistant starts responding
    #[test]
    fn step1_initial_response() {
        let mut h = Harness::new(WIDTH, HEIGHT).with_session(sessions::basic_exchange());

        // Inject start of streaming response
        h.events(vec![events::streaming_chunks("I understand")[0].clone()]);

        assert_snapshot!("e2e_streaming_step1_initial", h.render());
    }

    /// Response streams in word by word
    #[test]
    fn step2_streaming_progress() {
        let mut h = Harness::new(WIDTH, HEIGHT).with_session(sessions::basic_exchange());

        // Inject partial streaming
        let chunks = events::streaming_chunks("I can help you with that request.");
        for chunk in chunks.iter().take(4) {
            h.event(chunk.clone());
        }

        assert_snapshot!("e2e_streaming_step2_progress", h.render());
    }

    /// Response completes
    #[test]
    fn step3_streaming_complete() {
        let mut h = Harness::new(WIDTH, HEIGHT).with_session(sessions::basic_exchange());

        // Inject complete streaming response
        h.events(events::streaming_chunks(
            "I can help you with that request.",
        ));

        assert_snapshot!("e2e_streaming_step3_complete", h.render());
    }

    /// Error during streaming
    #[test]
    fn error_during_stream() {
        let mut h = Harness::new(WIDTH, HEIGHT).with_session(sessions::basic_exchange());

        // Inject error during streaming
        h.events(events::streaming_error(
            "I was responding when",
            "Connection lost",
        ));

        assert!(h.has_error());
        assert_snapshot!("e2e_streaming_error", h.render());
    }
}

// =============================================================================
// Tool Call Lifecycle - User asks → assistant uses tools → responds
// =============================================================================

mod tool_call_lifecycle {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    /// Tool call starts running
    #[test]
    fn step1_tool_running() {
        let mut h = Harness::new(WIDTH, HEIGHT).with_session(sessions::basic_exchange());

        // Inject tool call event
        h.event(events::tool_call_with_args(
            "glob",
            serde_json::json!({"pattern": "**/*.rs"}),
        ));

        assert_snapshot!("e2e_tool_step1_running", h.render());
    }

    /// Tool call completes
    #[test]
    fn step2_tool_complete() {
        let mut h = Harness::new(WIDTH, HEIGHT).with_session(sessions::basic_exchange());

        // Inject full tool lifecycle
        h.events(events::tool_lifecycle(
            "glob",
            serde_json::json!({"pattern": "**/*.rs"}),
            "Found 42 files",
        ));

        assert_snapshot!("e2e_tool_step2_complete", h.render());
    }

    /// Multiple tools in sequence
    #[test]
    fn step3_multi_tool_sequence() {
        let mut h = Harness::new(WIDTH, HEIGHT).with_session(sessions::basic_exchange());

        // Inject multi-tool sequence
        h.events(events::multi_tool_sequence());

        assert_snapshot!("e2e_tool_step3_multi", h.render());
    }

    /// Tool call with error
    #[test]
    fn tool_error_handling() {
        let mut h = Harness::new(WIDTH, HEIGHT).with_session(sessions::basic_exchange());

        // Inject tool call then error
        h.event(events::tool_call_event("read_file"));
        h.event(events::tool_error_event(
            "read_file",
            "Permission denied: /etc/shadow",
        ));

        assert_snapshot!("e2e_tool_error", h.render());
    }
}

// =============================================================================
// Input Editing Flow - Cursor movement, deletion, editing
// =============================================================================

mod input_editing_flow {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    /// Type a message, cursor at end
    #[test]
    fn step1_type_message() {
        let mut h = Harness::new(WIDTH, HEIGHT);

        h.keys("Hello, how are you?");

        assert_eq!(h.input_text(), "Hello, how are you?");
        assert_eq!(h.cursor_position(), 19);
        assert_snapshot!("e2e_input_step1_typed", h.render());
    }

    /// Move cursor to start with Ctrl+A
    #[test]
    fn step2_cursor_to_start() {
        let mut h = Harness::new(WIDTH, HEIGHT);

        h.keys("Hello world");
        h.key_ctrl('a');

        assert_eq!(h.cursor_position(), 0);
        assert_snapshot!("e2e_input_step2_cursor_start", h.render());
    }

    /// Delete word with Ctrl+W
    #[test]
    fn step3_delete_word() {
        let mut h = Harness::new(WIDTH, HEIGHT);

        h.keys("one two three");
        h.key_ctrl('w');

        assert_eq!(h.input_text(), "one two ");
        assert_snapshot!("e2e_input_step3_delete_word", h.render());
    }

    /// Clear line with Ctrl+U
    #[test]
    fn step4_clear_line() {
        let mut h = Harness::new(WIDTH, HEIGHT);

        h.keys("This will be cleared");
        h.key_ctrl('u');

        assert_eq!(h.input_text(), "");
        assert_snapshot!("e2e_input_step4_cleared", h.render());
    }

    /// Navigate and insert in middle
    #[test]
    fn step5_insert_in_middle() {
        let mut h = Harness::new(WIDTH, HEIGHT);

        h.keys("Hello world");
        h.key(KeyCode::Left);
        h.key(KeyCode::Left);
        h.key(KeyCode::Left);
        h.key(KeyCode::Left);
        h.key(KeyCode::Left);
        h.keys("beautiful ");

        assert_eq!(h.input_text(), "Hello beautiful world");
        assert_snapshot!("e2e_input_step5_insert_middle", h.render());
    }
}

// =============================================================================
// Conversation with Popup Overlay - Popup over existing conversation
// =============================================================================

mod popup_overlay_flow {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    /// Popup appears over existing conversation
    #[test]
    fn popup_over_multi_turn() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_session(sessions::multi_turn())
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        assert_snapshot!("e2e_overlay_multi_turn", h.render());
    }

    /// Popup over conversation with tool calls
    #[test]
    fn popup_over_tool_calls() {
        let h = Harness::new(WIDTH, HEIGHT)
            .with_session(sessions::with_tool_calls())
            .with_popup_items(PopupKind::AgentOrFile, registries::test_agents());

        assert_snapshot!("e2e_overlay_with_tools", h.render());
    }
}

// =============================================================================
// Full Session Flow - Complete multi-step interaction
// =============================================================================

mod full_session_flow {
    use super::*;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    /// Complete flow: type command → get response → ask follow-up
    #[test]
    fn complete_interaction() {
        let mut h = Harness::new(WIDTH, HEIGHT);

        // Step 1: User types a message
        h.keys("What is Rust?");
        assert_snapshot!("e2e_full_step1_typed", h.render());

        // Step 2: Clear input (simulating submit) and add to session
        h = Harness::new(WIDTH, HEIGHT).with_session(vec![
            sessions::user("What is Rust?"),
            sessions::assistant(
                "Rust is a systems programming language focused on safety and performance.",
            ),
        ]);
        assert_snapshot!("e2e_full_step2_response", h.render());

        // Step 3: User types follow-up
        h.keys("How do I install it?");
        assert_snapshot!("e2e_full_step3_followup", h.render());
    }

    /// Session with multiple tool uses
    #[test]
    fn session_with_tools() {
        let h = Harness::new(WIDTH, HEIGHT).with_session(sessions::interleaved_prose_and_tools());

        assert_snapshot!("e2e_full_interleaved", h.render());
    }

    /// Long session with scrolling
    #[test]
    fn long_session_scroll() {
        let h = Harness::new(WIDTH, HEIGHT).with_session(sessions::long_conversation());

        // Should show recent messages, not the beginning
        assert_snapshot!("e2e_full_long_scroll", h.render());
    }
}
