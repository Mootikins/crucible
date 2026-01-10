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
use super::{Harness, StreamingHarness, TEST_HEIGHT, TEST_WIDTH};
use crate::tui::state::PopupKind;
use crossterm::event::KeyCode;
use insta::assert_snapshot;

// =============================================================================
// Command Popup Flow - Type / → navigate → select
// =============================================================================

mod command_popup_flow {
    use super::*;

    /// User types `/` to open command popup, sees it appear
    #[test]
    fn step1_slash_opens_popup() {
        // with_popup_items pre-populates the popup but doesn't set input_buffer
        // This simulates the popup already being open with commands loaded
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        assert!(h.has_popup());
        assert_snapshot!("e2e_command_step1_opened", h.render());
    }

    /// User types search query to filter commands
    #[test]
    fn step2_filter_commands() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        // with_popup_items sets "/" in input, so we just type the filter text
        h.keys("sea");

        assert_eq!(h.popup_query(), Some("sea"));
        assert_eq!(h.input_text(), "/sea");
        assert_snapshot!("e2e_command_step2_filtered", h.render());
    }

    /// User navigates down to second item
    #[test]
    fn step3_navigate_selection() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        h.key(KeyCode::Down);
        h.key(KeyCode::Down);

        assert_eq!(h.popup_selected(), Some(2));
        assert_snapshot!("e2e_command_step3_navigated", h.render());
    }

    /// User presses Enter to select and insert command
    #[test]
    fn step4_select_command() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
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
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
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

    /// User types `@` to open agent/file popup
    #[test]
    fn step1_at_opens_popup() {
        // with_popup_items pre-populates the popup for testing
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_popup_items(PopupKind::AgentOrFile, registries::mixed_agent_file_items());

        assert!(h.has_popup());
        assert_snapshot!("e2e_agent_step1_opened", h.render());
    }

    /// User navigates through mixed agents/files/notes
    #[test]
    fn step2_navigate_mixed_items() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
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
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
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

    /// Initial state with user message, assistant starts responding
    #[test]
    fn step1_initial_response() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        // Inject start of streaming response
        h.events(vec![events::streaming_chunks("I understand")[0].clone()]);

        assert_snapshot!("e2e_streaming_step1_initial", h.render());
    }

    /// Response streams in word by word
    #[test]
    fn step2_streaming_progress() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

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
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        // Inject complete streaming response
        h.events(events::streaming_chunks(
            "I can help you with that request.",
        ));

        assert_snapshot!("e2e_streaming_step3_complete", h.render());
    }

    /// Error during streaming
    #[test]
    fn error_during_stream() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

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
// Graduation Flow - Content overflows viewport → graduates to scrollback
// =============================================================================

mod graduation_flow {
    use super::*;

    /// Content that fits in viewport should not graduate
    #[test]
    fn no_graduation_when_content_fits() {
        let mut h = StreamingHarness::inline();

        h.user_message("Hello");
        h.start_streaming();
        h.chunk("Short response.");
        h.complete();

        assert_eq!(h.graduated_line_count(), 0, "nothing should graduate");
        assert_snapshot!("e2e_graduation_fits", h.full_state());
    }

    /// Content that overflows during streaming should graduate progressively
    #[test]
    fn overflow_graduates_during_streaming() {
        let mut h = StreamingHarness::inline().with_timeline();

        h.user_message("Tell me a long story");
        h.start_streaming();

        // First chunk - should fit
        h.chunk("First paragraph of the response.\n\n");
        let after_first = h.graduated_line_count();

        // Keep adding paragraphs until we overflow (need lots of content due to wrapping)
        for i in 1..=20 {
            h.chunk(&format!(
                "Paragraph {} with enough text to take up space in the viewport.\n\n",
                i
            ));
        }

        assert!(
            h.graduated_line_count() > after_first,
            "overflow should cause graduation: got {} graduated lines",
            h.graduated_line_count()
        );
        assert_snapshot!("e2e_graduation_overflow_mid_stream", h.full_state());
    }

    /// KEY TEST: Completing stream should NOT graduate additional content
    #[test]
    fn complete_does_not_graduate_all() {
        let mut h = StreamingHarness::inline().with_timeline();

        h.user_message("Hello");
        h.start_streaming();

        // Add content that definitely overflows (same pattern as overflow test)
        for i in 1..=20 {
            h.chunk(&format!(
                "Paragraph {} with enough text to take up space in the viewport.\n\n",
                i
            ));
        }

        // Verify we have graduated content before completing
        assert!(
            h.graduated_line_count() > 0,
            "should have graduated content before complete"
        );

        let graduated_before_complete = h.graduated_line_count();

        // Complete streaming
        h.complete();

        // KEY ASSERTION: completing should NOT graduate more content
        assert_eq!(
            h.graduated_line_count(),
            graduated_before_complete,
            "completing should not graduate additional content - only overflow graduates"
        );

        // Viewport should still show recent content, not be empty
        let viewport = h.harness.render();
        assert!(
            viewport.contains("Paragraph 20") || viewport.contains("Paragraph 19"),
            "recent content should remain visible in viewport"
        );

        assert_snapshot!("e2e_graduation_after_complete", h.full_state());
    }

    /// Scrollback should contain graduated content in order
    #[test]
    fn scrollback_preserves_order() {
        let mut h = StreamingHarness::inline();

        h.user_message("Test ordering");
        h.start_streaming();

        // Generate enough content to overflow (same pattern as other tests)
        for i in 1..=20 {
            h.chunk(&format!(
                "Paragraph {} with enough text to take up space in the viewport.\n\n",
                i
            ));
        }
        h.complete();

        let scrollback = h.scrollback();
        assert!(!scrollback.is_empty(), "should have graduated content");

        // Find first non-empty line in scrollback
        let first_content = scrollback
            .iter()
            .find(|s| !s.trim().is_empty())
            .expect("should have content in scrollback");

        // First content should be early content (user message or paragraph 1)
        assert!(
            first_content.contains("Test") || first_content.contains("Paragraph 1"),
            "scrollback should start with early content, got: {}",
            first_content
        );

        assert_snapshot!("e2e_graduation_scrollback_order", h.full_state());
    }
}

// =============================================================================
// Rendering Bug Reproductions - Issues from small-errors.txt
// =============================================================================

mod rendering_bugs {
    use super::*;

    /// BUG: User message appears twice in viewport
    /// From small-errors.txt lines 1,3: "> hi!" appears twice
    #[test]
    fn bug_duplicated_user_message() {
        let mut h = StreamingHarness::inline().with_timeline();

        // User sends single message
        h.user_message("hi!");

        // Start and complete a response
        h.start_streaming();
        h.chunk("Hello! I'm Claude.");
        h.complete();

        // Verify user message appears exactly once
        let rendered = h.harness.render();
        let hi_count = rendered.matches("> hi!").count();
        assert_eq!(hi_count, 1, "user message should appear exactly once, found {}", hi_count);

        assert_snapshot!("bug_duplicated_user_message", h.full_state());
    }

    /// BUG: Tool output appears interleaved mid-prose
    /// From small-errors.txt: tool result hash appears in middle of bullet list
    #[test]
    fn bug_interleaved_tool_output() {
        let mut h = StreamingHarness::inline().with_timeline();

        h.user_message("hi!");
        h.start_streaming();

        // Stream some prose
        h.chunk("Hello! I can help you with:\n");
        h.chunk("- Understanding the project\n");

        // Tool call happens mid-stream
        h.harness.event(events::tool_call_with_args(
            "read_file",
            serde_json::json!({"path": "README.md"}),
        ));
        h.harness.event(events::tool_completed_event(
            "read_file",
            "# Crucible\n...",
        ));

        // More prose after tool
        h.chunk("- Assisting with development\n");
        h.chunk("- Explaining systems\n");
        h.complete();

        // Tool output should NOT appear mid-prose
        let rendered = h.harness.render();

        // The prose bullet points should be contiguous
        assert_snapshot!("bug_interleaved_tool_output", h.full_state());
    }

    /// BUG: Tool call indicator appears during prose stream
    /// From small-errors.txt line 19: "◐ read_file(...)" in middle of response
    #[test]
    fn bug_tool_indicator_mid_stream() {
        let mut h = StreamingHarness::inline().with_timeline();

        h.user_message("hi!");
        h.start_streaming();

        // Stream initial response
        h.chunk("Hello! I'm Claude.\n\n");
        h.chunk("I can help you with:\n");

        // Tool call starts (should show indicator properly, not mid-prose)
        h.harness.event(events::tool_call_with_args(
            "read_file",
            serde_json::json!({"path": "README.md"}),
        ));

        // More streaming after tool call starts
        h.chunk("- Project architecture\n");

        // Tool completes
        h.harness.event(events::tool_completed_event(
            "read_file",
            "# Crucible README",
        ));

        h.complete();

        // Verify proper rendering order
        assert_snapshot!("bug_tool_indicator_mid_stream", h.full_state());
    }
}

// =============================================================================
// Tool Call Lifecycle - User asks → assistant uses tools → responds
// =============================================================================

mod tool_call_lifecycle {
    use super::*;

    /// Tool call starts running
    #[test]
    fn step1_tool_running() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

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
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

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
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

        // Inject multi-tool sequence
        h.events(events::multi_tool_sequence());

        assert_snapshot!("e2e_tool_step3_multi", h.render());
    }

    /// Tool call with error
    #[test]
    fn tool_error_handling() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::basic_exchange());

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

    /// Type a message, cursor at end
    #[test]
    fn step1_type_message() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.keys("Hello, how are you?");

        assert_eq!(h.input_text(), "Hello, how are you?");
        assert_eq!(h.cursor_position(), 19);
        assert_snapshot!("e2e_input_step1_typed", h.render());
    }

    /// Move cursor to start with Ctrl+A
    #[test]
    fn step2_cursor_to_start() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.keys("Hello world");
        h.key_ctrl('a');

        assert_eq!(h.cursor_position(), 0);
        assert_snapshot!("e2e_input_step2_cursor_start", h.render());
    }

    /// Delete word with Ctrl+W
    #[test]
    fn step3_delete_word() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.keys("one two three");
        h.key_ctrl('w');

        assert_eq!(h.input_text(), "one two ");
        assert_snapshot!("e2e_input_step3_delete_word", h.render());
    }

    /// Clear line with Ctrl+U
    #[test]
    fn step4_clear_line() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        h.keys("This will be cleared");
        h.key_ctrl('u');

        assert_eq!(h.input_text(), "");
        assert_snapshot!("e2e_input_step4_cleared", h.render());
    }

    /// Navigate and insert in middle
    #[test]
    fn step5_insert_in_middle() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

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

    /// Popup appears over existing conversation
    #[test]
    fn popup_over_multi_turn() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_session(sessions::multi_turn())
            .with_popup_items(PopupKind::Command, registries::standard_commands());

        assert_snapshot!("e2e_overlay_multi_turn", h.render());
    }

    /// Popup over conversation with tool calls
    #[test]
    fn popup_over_tool_calls() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
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

    /// Complete flow: type command -> get response -> ask follow-up
    #[test]
    fn complete_interaction() {
        let mut h = Harness::new(TEST_WIDTH, TEST_HEIGHT);

        // Step 1: User types a message
        h.keys("What is Rust?");
        assert_snapshot!("e2e_full_step1_typed", h.render());

        // Step 2: Clear input (simulating submit) and add to session
        h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(vec![
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
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT)
            .with_session(sessions::interleaved_prose_and_tools());

        assert_snapshot!("e2e_full_interleaved", h.render());
    }

    /// Long session with scrolling
    #[test]
    fn long_session_scroll() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::long_conversation());

        // Should show recent messages, not the beginning
        assert_snapshot!("e2e_full_long_scroll", h.render());
    }
}
