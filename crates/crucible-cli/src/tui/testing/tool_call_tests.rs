//! Tests for tool call rendering and styling
//!
//! These tests capture and verify how tool calls are displayed in the TUI,
//! including running, complete, and error states.

use super::fixtures::sessions;
use super::Harness;
use crate::tui::conversation::{
    render_item_to_lines, ConversationItem, ConversationState, ToolCallDisplay, ToolStatus,
};
use insta::assert_snapshot;
use ratatui::{backend::TestBackend, Terminal};

// =============================================================================
// Snapshot Tests - Tool Call Rendering
// =============================================================================

mod snapshots {
    use super::*;
    use crate::tui::components::SessionHistoryWidget;
    use ratatui::widgets::Widget;

    const WIDTH: u16 = 80;
    const HEIGHT: u16 = 24;

    fn render_conversation(state: &ConversationState) -> String {
        let backend = TestBackend::new(WIDTH, HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let widget = SessionHistoryWidget::new(state).viewport_height(HEIGHT);
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
    fn tool_call_running_state() {
        let mut state = ConversationState::new();
        for item in sessions::tool_call_running() {
            state.push(item);
        }
        assert_snapshot!("tool_call_running", render_conversation(&state));
    }

    #[test]
    fn tool_call_complete_state() {
        let mut state = ConversationState::new();
        for item in sessions::tool_call_complete() {
            state.push(item);
        }
        assert_snapshot!("tool_call_complete", render_conversation(&state));
    }

    #[test]
    fn tool_call_error_state() {
        let mut state = ConversationState::new();
        for item in sessions::tool_call_error() {
            state.push(item);
        }
        assert_snapshot!("tool_call_error", render_conversation(&state));
    }

    #[test]
    fn tool_call_with_output_lines() {
        let mut state = ConversationState::new();
        for item in sessions::tool_call_with_output() {
            state.push(item);
        }
        assert_snapshot!("tool_call_with_output", render_conversation(&state));
    }

    #[test]
    fn multiple_sequential_tool_calls() {
        let mut state = ConversationState::new();
        for item in sessions::multiple_tool_calls() {
            state.push(item);
        }
        assert_snapshot!("tool_calls_sequential", render_conversation(&state));
    }

    #[test]
    fn interleaved_prose_and_tools() {
        let mut state = ConversationState::new();
        for item in sessions::interleaved_prose_and_tools() {
            state.push(item);
        }
        assert_snapshot!("tool_calls_interleaved", render_conversation(&state));
    }

    #[test]
    fn mixed_success_and_error() {
        let mut state = ConversationState::new();
        for item in sessions::mixed_tool_results() {
            state.push(item);
        }
        assert_snapshot!("tool_calls_mixed_results", render_conversation(&state));
    }
}

// =============================================================================
// Unit Tests - Tool Call Line Rendering
// =============================================================================

mod unit_tests {
    use super::*;
    use crate::tui::styles::indicators;

    #[test]
    fn running_tool_shows_spinner() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "grep".to_string(),
            status: ToolStatus::Running,
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        // Find line containing tool name
        let tool_line = lines
            .iter()
            .find(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.as_ref().contains("grep"))
            })
            .expect("Should find grep line");

        // Should contain spinner character
        let text: String = tool_line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains(indicators::SPINNER_FRAMES[0]),
            "Running tool should show spinner. Got: {}",
            text
        );
    }

    #[test]
    fn complete_tool_shows_checkmark() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "glob".to_string(),
            status: ToolStatus::Complete {
                summary: Some("5 files".to_string()),
            },
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        let tool_line = lines
            .iter()
            .find(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.as_ref().contains("glob"))
            })
            .expect("Should find glob line");

        let text: String = tool_line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains(indicators::COMPLETE),
            "Complete tool should show checkmark. Got: {}",
            text
        );
        assert!(
            text.contains("5 files"),
            "Complete tool should show summary. Got: {}",
            text
        );
    }

    #[test]
    fn error_tool_shows_x_mark() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "read".to_string(),
            status: ToolStatus::Error {
                message: "not found".to_string(),
            },
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        let tool_line = lines
            .iter()
            .find(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.as_ref().contains("read"))
            })
            .expect("Should find read line");

        let text: String = tool_line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains(indicators::ERROR),
            "Error tool should show X mark. Got: {}",
            text
        );
        assert!(
            text.contains("not found"),
            "Error tool should show message. Got: {}",
            text
        );
    }

    #[test]
    fn tool_output_lines_are_indented() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "grep".to_string(),
            status: ToolStatus::Running,
            output_lines: vec!["match 1".to_string(), "match 2".to_string()],
        });

        let lines = render_item_to_lines(&tool, 80);

        // Should have blank line + tool line + 2 output lines = 4 total
        assert!(
            lines.len() >= 3,
            "Should have tool line and output. Got {} lines",
            lines.len()
        );

        // Find output lines (should be indented)
        let output_lines: Vec<_> = lines
            .iter()
            .filter(|l| {
                let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
                text.contains("match")
            })
            .collect();

        assert_eq!(output_lines.len(), 2, "Should have 2 output lines");

        for line in output_lines {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(
                text.starts_with("  "),
                "Output lines should be indented. Got: '{}'",
                text
            );
        }
    }

    #[test]
    fn blank_line_before_tool_call() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "test".to_string(),
            status: ToolStatus::Running,
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        // First line should be blank for spacing
        assert!(!lines.is_empty(), "Should have at least one line");
        let first_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            first_text.trim().is_empty(),
            "First line should be blank for spacing. Got: '{}'",
            first_text
        );
    }
}

// =============================================================================
// Integration Tests - Harness
// =============================================================================

mod harness_tests {
    use super::*;

    #[test]
    fn harness_with_tool_call_session() {
        let h = Harness::new(80, 24).with_session(sessions::with_tool_calls());

        assert_eq!(h.conversation_len(), 4);

        // Render should work without panic
        let output = h.render();
        assert!(!output.is_empty());
    }

    #[test]
    fn harness_renders_tool_indicators() {
        let h = Harness::new(80, 24).with_session(sessions::tool_call_complete());

        let output = h.render();

        // Should contain checkmark indicator
        assert!(
            output.contains("✓") || output.contains(crate::tui::styles::indicators::COMPLETE),
            "Should show complete indicator"
        );
    }

    #[test]
    fn harness_multiple_tools_all_visible() {
        let h = Harness::new(80, 24).with_session(sessions::multiple_tool_calls());

        let output = h.render();

        // Should see both tool names
        assert!(output.contains("glob"), "Should show glob tool");
        assert!(output.contains("read_file"), "Should show read_file tool");
    }
}

// =============================================================================
// Style Verification Tests
// =============================================================================

mod style_tests {
    use super::*;
    use crate::tui::styles::{colors, presets};
    use ratatui::style::Color;

    #[test]
    fn tool_running_uses_yellow() {
        let style = presets::tool_running();
        assert_eq!(style.fg, Some(colors::TOOL_RUNNING));
        assert_eq!(colors::TOOL_RUNNING, Color::Yellow);
    }

    #[test]
    fn tool_complete_uses_green() {
        let style = presets::tool_complete();
        assert_eq!(style.fg, Some(colors::TOOL_COMPLETE));
        assert_eq!(colors::TOOL_COMPLETE, Color::Green);
    }

    #[test]
    fn tool_error_uses_red_bold() {
        let style = presets::tool_error();
        assert_eq!(style.fg, Some(colors::TOOL_ERROR));
        assert_eq!(colors::TOOL_ERROR, Color::Red);
        assert!(
            style
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD),
            "Error style should be bold"
        );
    }

    #[test]
    fn tool_output_is_dim() {
        let style = presets::tool_output();
        assert_eq!(style.fg, Some(colors::DIM));
    }
}
