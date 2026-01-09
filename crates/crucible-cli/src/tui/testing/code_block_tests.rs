//! Tests for code block rendering with syntax highlighting.
//!
//! These tests capture and verify how code blocks are displayed in the TUI,
//! including various languages, untagged blocks, and multi-block scenarios.

use super::fixtures::sessions;
use super::{Harness, TEST_HEIGHT, TEST_WIDTH};
use crate::tui::conversation::ConversationState;
use insta::assert_snapshot;

// =============================================================================
// Snapshot Tests - Code Block Rendering
// =============================================================================

mod snapshots {
    use super::*;
    use crate::tui::components::SessionHistoryWidget;
    use ratatui::backend::TestBackend;
    use ratatui::widgets::Widget;
    use ratatui::Terminal;

    fn render_conversation(state: &ConversationState) -> String {
        let backend = TestBackend::new(TEST_WIDTH, TEST_HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let widget = SessionHistoryWidget::new(state).viewport_height(TEST_HEIGHT);
                f.render_widget(widget, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        buffer_to_string(buffer)
    }

    fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
        let mut output = String::new();
        let area = buffer.area;

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if let Some(cell) = buffer.cell((x, y)) {
                    output.push_str(cell.symbol());
                }
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn rust_code_block_renders() {
        let mut state = ConversationState::new();
        for item in sessions::with_rust_code() {
            state.push(item);
        }
        assert_snapshot!("code_block_rust", render_conversation(&state));
    }

    #[test]
    fn multiple_code_blocks_render() {
        let mut state = ConversationState::new();
        for item in sessions::with_multi_lang_code() {
            state.push(item);
        }
        assert_snapshot!("code_blocks_multi_lang", render_conversation(&state));
    }

    #[test]
    fn long_code_block_renders() {
        let mut state = ConversationState::new();
        for item in sessions::with_long_code() {
            state.push(item);
        }
        assert_snapshot!("code_block_long", render_conversation(&state));
    }

    #[test]
    fn untagged_code_block_renders() {
        let mut state = ConversationState::new();
        for item in sessions::with_untagged_code() {
            state.push(item);
        }
        assert_snapshot!("code_block_untagged", render_conversation(&state));
    }

    #[test]
    fn inline_code_renders() {
        let mut state = ConversationState::new();
        for item in sessions::with_inline_code() {
            state.push(item);
        }
        assert_snapshot!("code_inline", render_conversation(&state));
    }

    /// Test the existing multiline_messages fixture for completeness
    #[test]
    fn multiline_messages_with_code() {
        let mut state = ConversationState::new();
        for item in sessions::multiline_messages() {
            state.push(item);
        }
        assert_snapshot!("code_block_multiline", render_conversation(&state));
    }
}

// =============================================================================
// Integration Tests - Harness
// =============================================================================

mod harness_tests {
    use super::*;

    #[test]
    fn harness_with_rust_code_session() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_rust_code());

        assert_eq!(h.conversation_len(), 2);

        // Render should work without panic
        let output = h.render();
        assert!(!output.is_empty());
    }

    #[test]
    fn harness_renders_code_block_content() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_rust_code());

        let output = h.render();

        // Should contain the code content
        assert!(
            output.contains("fn main()"),
            "Should show fn main(). Got: {}",
            output
        );
        assert!(
            output.contains("println!"),
            "Should show println!. Got: {}",
            output
        );
    }

    #[test]
    fn harness_multi_lang_blocks_all_visible() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_multi_lang_code());

        let output = h.render();

        // Should see both languages' code
        assert!(output.contains("print"), "Should show Python print");
        assert!(output.contains("println!"), "Should show Rust println!");
    }

    #[test]
    fn harness_inline_code_visible() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_inline_code());

        let output = h.render();

        // Should contain the backtick content (rendered somehow)
        assert!(
            output.contains("println!"),
            "Should show println! from inline code. Got: {}",
            output
        );
    }
}
