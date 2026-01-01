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

    /// Test batched tool calls (multiple tools in single agent turn, no prose between)
    #[test]
    fn batched_tool_calls_single_turn() {
        let mut state = ConversationState::new();
        for item in sessions::batched_tool_calls() {
            state.push(item);
        }
        assert_snapshot!("tool_calls_batched", render_conversation(&state));
    }

    /// Test tool call argument formatting with various argument types
    #[test]
    fn tool_call_argument_formatting() {
        let mut state = ConversationState::new();

        // Tool with no arguments (empty object)
        state.push(ConversationItem::ToolCall(ToolCallDisplay {
            name: "ls".to_string(),
            args: serde_json::json!({}),
            status: ToolStatus::Complete {
                summary: Some("15 files".to_string()),
            },
            output_lines: vec![],
        }));

        // Tool with single string argument
        state.push(ConversationItem::ToolCall(ToolCallDisplay {
            name: "grep".to_string(),
            args: serde_json::json!({"pattern": "TODO"}),
            status: ToolStatus::Complete {
                summary: Some("5 matches".to_string()),
            },
            output_lines: vec![],
        }));

        // Tool with multiple arguments
        state.push(ConversationItem::ToolCall(ToolCallDisplay {
            name: "read".to_string(),
            args: serde_json::json!({"path": "/src/main.rs", "limit": 100}),
            status: ToolStatus::Complete {
                summary: Some("100 lines".to_string()),
            },
            output_lines: vec![],
        }));

        // Tool with long string that gets truncated
        state.push(ConversationItem::ToolCall(ToolCallDisplay {
            name: "search".to_string(),
            args: serde_json::json!({"query": "This is a very long search query that should definitely be truncated when displayed to avoid cluttering the interface"}),
            status: ToolStatus::Complete {
                summary: Some("3 results".to_string()),
            },
            output_lines: vec![],
        }));

        // Tool with boolean argument
        state.push(ConversationItem::ToolCall(ToolCallDisplay {
            name: "build".to_string(),
            args: serde_json::json!({"release": true, "verbose": false}),
            status: ToolStatus::Complete {
                summary: Some("success".to_string()),
            },
            output_lines: vec![],
        }));

        // Tool with array argument (shows count)
        state.push(ConversationItem::ToolCall(ToolCallDisplay {
            name: "batch".to_string(),
            args: serde_json::json!({"files": ["a.rs", "b.rs", "c.rs"]}),
            status: ToolStatus::Complete {
                summary: Some("3 files processed".to_string()),
            },
            output_lines: vec![],
        }));

        // Tool with nested object argument (shows {...})
        state.push(ConversationItem::ToolCall(ToolCallDisplay {
            name: "config".to_string(),
            args: serde_json::json!({"options": {"nested": true}}),
            status: ToolStatus::Complete {
                summary: Some("configured".to_string()),
            },
            output_lines: vec![],
        }));

        assert_snapshot!(
            "tool_calls_argument_formatting",
            render_conversation(&state)
        );
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
            args: serde_json::json!({"pattern": "test"}),
            status: ToolStatus::Running,
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        // Find line containing tool name
        let tool_line = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.as_ref().contains("grep")))
            .expect("Should find grep line");

        // Should contain spinner character with alignment prefix
        let text: String = tool_line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains(indicators::SPINNER_FRAMES[0]),
            "Running tool should show spinner. Got: {}",
            text
        );
        // Should be aligned with " X " prefix pattern
        assert!(
            text.starts_with(" "),
            "Tool call should start with space for alignment. Got: '{}'",
            text
        );
    }

    #[test]
    fn complete_tool_shows_green_dot() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "glob".to_string(),
            args: serde_json::json!({"pattern": "**/*.rs"}),
            status: ToolStatus::Complete {
                summary: Some("5 files".to_string()),
            },
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        let tool_line = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.as_ref().contains("glob")))
            .expect("Should find glob line");

        let text: String = tool_line.spans.iter().map(|s| s.content.as_ref()).collect();
        // Complete tool shows green dot (●) not checkmark
        assert!(
            text.contains(indicators::TOOL_COMPLETE),
            "Complete tool should show green dot (●). Got: {}",
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
            args: serde_json::json!({"path": "/missing"}),
            status: ToolStatus::Error {
                message: "not found".to_string(),
            },
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        let tool_line = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.as_ref().contains("read")))
            .expect("Should find read line");

        let text: String = tool_line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains(indicators::TOOL_ERROR),
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
    fn tool_output_only_shown_while_running() {
        // Running tool should show output
        let running_tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "grep".to_string(),
            args: serde_json::json!({"pattern": "TODO"}),
            status: ToolStatus::Running,
            output_lines: vec!["match 1".to_string(), "match 2".to_string()],
        });

        let running_lines = render_item_to_lines(&running_tool, 80);
        let running_has_output = running_lines.iter().any(|l| {
            let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            text.contains("match")
        });
        assert!(running_has_output, "Running tool should show output");

        // Complete tool should NOT show output (shrinks)
        let complete_tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "grep".to_string(),
            args: serde_json::json!({"pattern": "TODO"}),
            status: ToolStatus::Complete {
                summary: Some("2 matches".to_string()),
            },
            output_lines: vec!["match 1".to_string(), "match 2".to_string()],
        });

        let complete_lines = render_item_to_lines(&complete_tool, 80);
        let complete_has_output = complete_lines.iter().any(|l| {
            let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            text.contains("match 1") || text.contains("match 2")
        });
        assert!(
            !complete_has_output,
            "Complete tool should hide output (shrink)"
        );
    }

    #[test]
    fn tool_output_max_3_lines() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "grep".to_string(),
            args: serde_json::json!({"pattern": "x"}),
            status: ToolStatus::Running,
            output_lines: vec![
                "line 1".to_string(),
                "line 2".to_string(),
                "line 3".to_string(),
                "line 4".to_string(),
                "line 5".to_string(),
            ],
        });

        let lines = render_item_to_lines(&tool, 80);

        // Count output lines (those containing "line")
        let output_count = lines
            .iter()
            .filter(|l| {
                let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
                text.contains("line")
            })
            .count();

        assert_eq!(
            output_count, 3,
            "Should show max 3 output lines. Got: {}",
            output_count
        );

        // Should show the LAST 3 lines (most recent)
        let has_line_5 = lines.iter().any(|l| {
            let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            text.contains("line 5")
        });
        assert!(has_line_5, "Should show most recent line (line 5)");
    }

    #[test]
    fn tool_output_lines_are_indented() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "grep".to_string(),
            args: serde_json::json!({}),
            status: ToolStatus::Running,
            output_lines: vec!["match 1".to_string()],
        });

        let lines = render_item_to_lines(&tool, 80);

        // Find output line
        let output_line = lines
            .iter()
            .find(|l| {
                let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
                text.contains("match")
            })
            .expect("Should find output line");

        let text: String = output_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        // Should be indented with 4 spaces to align under tool name
        assert!(
            text.starts_with("    "),
            "Output lines should be indented 4 spaces. Got: '{}'",
            text
        );
    }

    /// Tool call rendering no longer includes blank line - spacing is now handled
    /// at the ConversationWidget level to allow consecutive tool calls to be grouped.
    #[test]
    fn tool_call_renders_without_leading_blank() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "test".to_string(),
            args: serde_json::json!({}),
            status: ToolStatus::Running,
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        // First line should be the tool content, not blank
        // (spacing is handled by ConversationWidget::render_to_lines)
        assert!(!lines.is_empty(), "Should have at least one line");
        let first_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            first_text.contains("test"),
            "First line should contain tool name. Got: '{}'",
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

        // Should contain green dot indicator (●)
        assert!(
            output.contains("●") || output.contains(crate::tui::styles::indicators::TOOL_COMPLETE),
            "Should show complete indicator (green dot). Got: {}",
            output
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
// Regression Tests
// =============================================================================

mod event_based_tests {
    use super::*;
    use crate::tui::testing::fixtures::events;

    /// Snapshot test: Multi-tool conversation via streaming events
    ///
    /// Tests that tool call and completion events are properly rendered,
    /// including tool arguments, completion summaries, and error states.
    #[test]
    fn multi_tool_sequence_via_events() {
        let mut h = Harness::new(80, 24);

        // Add user message first
        h.view
            .state_mut()
            .conversation
            .push_user_message("Find all Rust files and read main.rs");

        // Inject the multi-tool sequence
        h.events(events::multi_tool_sequence());

        // Should have 3 tools tracked
        assert_eq!(
            h.state.pending_tools.len(),
            3,
            "Should track 3 tool calls in state"
        );

        // All should be completed
        assert!(
            h.state.pending_tools.iter().all(|t| t.completed),
            "All tools should be marked completed"
        );

        // Render and snapshot
        assert_snapshot!("multi_tool_via_events", h.render());
    }

    /// Test single tool lifecycle via events
    #[test]
    fn single_tool_lifecycle() {
        let mut h = Harness::new(80, 24);

        // Tool starts running
        h.event(events::tool_call_with_args(
            "glob",
            serde_json::json!({"pattern": "**/*.md"}),
        ));

        // Render while running
        let running_output = h.render();
        assert!(
            running_output.contains("◐"),
            "Running tool should show spinner"
        );
        assert!(
            running_output.contains("glob(pattern="),
            "Should show tool name with args"
        );

        // Complete the tool
        h.event(events::tool_completed_event(
            "glob",
            "Found 25 markdown files",
        ));

        // Render after completion
        let complete_output = h.render();
        assert!(
            complete_output.contains("●"),
            "Completed tool should show green dot"
        );
        assert!(
            complete_output.contains("Found 25 markdown files"),
            "Should show result summary"
        );
        assert!(
            !complete_output.contains("◐"),
            "Should not show spinner after completion"
        );

        assert_snapshot!("single_tool_lifecycle", complete_output);
    }

    /// Test tool error via events
    #[test]
    fn tool_error_via_events() {
        let mut h = Harness::new(80, 24);

        h.event(events::tool_call_with_args(
            "read_file",
            serde_json::json!({"path": "/nonexistent.txt"}),
        ));
        h.event(events::tool_error_event("read_file", "File not found"));

        let output = h.render();
        assert!(
            output.contains("✗") || output.contains("×"),
            "Error tool should show error indicator"
        );
        assert!(
            output.contains("File not found"),
            "Should show error message"
        );

        assert_snapshot!("tool_error_via_events", output);
    }
}

mod regression_tests {
    use super::*;
    use crate::tui::conversation_view::ConversationView;
    use crate::tui::streaming_channel::StreamingEvent;

    /// Regression test: Tool calls should show tool name, never empty
    /// Bug: Orphan spinner " ◐" appeared with no tool name
    /// Fix: Empty tool names now render nothing (no lines at all)
    #[test]
    fn tool_call_with_empty_name_not_rendered() {
        let tool = ConversationItem::ToolCall(ToolCallDisplay {
            name: "".to_string(),
            args: serde_json::json!({}),
            status: ToolStatus::Running,
            output_lines: vec![],
        });

        let lines = render_item_to_lines(&tool, 80);

        // Empty name should render NO lines (not even blank spacing line)
        assert!(
            lines.is_empty(),
            "Tool call with empty name should render nothing. Got {} lines",
            lines.len()
        );

        // Double-check no spinner appears
        let has_spinner = lines.iter().any(|l| {
            let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
            text.contains("◐")
        });
        assert!(!has_spinner, "Empty tool name should not render spinner");
    }

    /// Regression test: Simulate the exact sequence from the bug report
    /// Tool runs, then there's an orphan spinner, then assistant message
    #[test]
    fn tool_completion_flow_no_orphan_spinner() {
        let mut h = Harness::new(80, 24);

        // User asks a question
        h.view
            .state_mut()
            .conversation
            .push_user_message("what folder are you in?");

        // Tool starts running
        h.view
            .push_tool_running("crucible_get_kiln_info", serde_json::json!({}));

        // Render and check - should have exactly one tool running
        let output = h.render();
        let spinner_count = output.matches(" ◐ ").count();
        assert_eq!(
            spinner_count, 1,
            "Should have exactly one running tool spinner. Got {} in:\n{}",
            spinner_count, output
        );

        // Tool completes
        h.view.complete_tool(
            "crucible_get_kiln_info",
            Some("kiln path returned".to_string()),
        );

        // Render again - should have green dot, no spinners
        let output = h.render();
        let spinner_count = output.matches(" ◐ ").count();
        assert_eq!(
            spinner_count, 0,
            "Completed tool should have no spinners. Got {} in:\n{}",
            spinner_count, output
        );

        // Should have green dot
        assert!(
            output.contains(" ● crucible_get_kiln_info"),
            "Completed tool should show green dot. Got:\n{}",
            output
        );
    }

    /// Test that harness properly injects ToolCall events
    #[test]
    fn harness_tool_call_event_creates_running_tool() {
        let mut h = Harness::new(80, 24);

        // Inject tool call event (simulating StreamingEvent::ToolCall)
        h.event(StreamingEvent::ToolCall {
            id: Some("call_123".to_string()),
            name: "test_tool".to_string(),
            args: serde_json::Value::Null,
        });

        // Verify tool is tracked in state
        assert_eq!(h.state.pending_tools.len(), 1);
        assert_eq!(h.state.pending_tools[0].name, "test_tool");
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
    fn tool_running_uses_white() {
        let style = presets::tool_running();
        assert_eq!(style.fg, Some(colors::TOOL_RUNNING));
        assert_eq!(colors::TOOL_RUNNING, Color::White);
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
            style.add_modifier.contains(ratatui::style::Modifier::BOLD),
            "Error style should be bold"
        );
    }

    #[test]
    fn tool_output_is_dim() {
        let style = presets::tool_output();
        assert_eq!(style.fg, Some(colors::DIM));
    }
}
