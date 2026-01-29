use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::TestRuntime;

fn render_app(app: &OilChatApp) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    strip_ansi(&render_to_string(&tree, 120))
}

fn find_positions(output: &str, markers: &[&str]) -> Vec<(String, Option<usize>)> {
    markers
        .iter()
        .map(|m| (m.to_string(), output.find(m)))
        .collect()
}

fn assert_order(output: &str, first: &str, second: &str) {
    let pos_first = output.find(first);
    let pos_second = output.find(second);

    assert!(
        pos_first.is_some(),
        "'{}' not found in output:\n{}",
        first,
        output
    );
    assert!(
        pos_second.is_some(),
        "'{}' not found in output:\n{}",
        second,
        output
    );
    assert!(
        pos_first.unwrap() < pos_second.unwrap(),
        "'{}' (pos {}) should appear before '{}' (pos {})\nOutput:\n{}",
        first,
        pos_first.unwrap(),
        second,
        pos_second.unwrap(),
        output
    );
}

mod chat_app_message_handling {
    use super::*;

    #[test]
    fn tool_call_message_creates_tool_in_cache() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"test.rs"}"#.to_string(),
        });

        assert!(app.is_streaming(), "Should be in streaming state");

        let output = render_app(&app);
        assert!(
            output.contains("read_file"),
            "Tool name should appear: {}",
            output
        );
    }

    #[test]
    fn tool_result_complete_marks_tool_done() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "file contents".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
        });
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);
        assert!(
            output.contains('\u{2713}') || output.contains("✓"),
            "Should show checkmark for completed tool: {}",
            output
        );
    }

    #[test]
    fn interleaved_text_and_tool_maintains_order_during_streaming() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("BEFORE_TOOL ".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "my_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("AFTER_TOOL".to_string()));

        let output = render_app(&app);
        assert_order(&output, "BEFORE_TOOL", "my_tool");
    }

    #[test]
    fn interleaved_text_and_tool_maintains_order_after_completion() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("BEFORE_TOOL ".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "my_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "my_tool".to_string(),
            delta: "result".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "my_tool".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("AFTER_TOOL".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        assert!(
            !app.is_streaming(),
            "Should not be streaming after complete"
        );

        let output = render_app(&app);
        assert_order(&output, "BEFORE_TOOL", "my_tool");
        assert_order(&output, "my_tool", "AFTER_TOOL");
    }

    #[test]
    fn multiple_tool_calls_maintain_order() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("START ".to_string()));

        app.on_message(ChatAppMsg::ToolCall {
            name: "tool_alpha".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool_alpha".to_string(),
        });

        app.on_message(ChatAppMsg::TextDelta("MIDDLE ".to_string()));

        app.on_message(ChatAppMsg::ToolCall {
            name: "tool_beta".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool_beta".to_string(),
        });

        app.on_message(ChatAppMsg::TextDelta("END".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);

        assert_order(&output, "START", "tool_alpha");
        assert_order(&output, "tool_alpha", "MIDDLE");
        assert_order(&output, "MIDDLE", "tool_beta");
        assert_order(&output, "tool_beta", "END");
    }
}

mod tool_completion_visibility {
    use super::*;

    #[test]
    fn incomplete_tool_shows_pending_indicator() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "pending_tool".to_string(),
            args: "{}".to_string(),
        });

        let output = render_app(&app);
        assert!(
            !output.contains('\u{2713}'),
            "Incomplete tool should NOT show checkmark: {}",
            output
        );
    }

    #[test]
    fn completed_tool_shows_checkmark() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "completed_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "completed_tool".to_string(),
        });
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);
        assert!(
            output.contains('\u{2713}') || output.contains("✓"),
            "Completed tool should show checkmark: {}",
            output
        );
    }

    #[test]
    fn tool_result_delta_followed_by_complete_shows_result() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "result_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "result_tool".to_string(),
            delta: "TOOL_OUTPUT_CONTENT".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "result_tool".to_string(),
        });
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);
        assert!(
            output.contains("TOOL_OUTPUT_CONTENT"),
            "Should show tool result content: {}",
            output
        );
    }

    #[test]
    fn mismatched_tool_name_does_not_complete_wrong_tool() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "actual_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "different_tool".to_string(),
        });
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);
        let has_checkmark = output.contains('\u{2713}') || output.contains("✓");

        assert!(
            !has_checkmark,
            "Tool with mismatched name should NOT show checkmark: {}",
            output
        );
    }
}

mod realistic_scenarios {
    use super::*;

    #[test]
    fn typical_tool_use_flow() {
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Read the config file".to_string()));

        app.on_message(ChatAppMsg::TextDelta(
            "I'll read the configuration file for you.\n\n".to_string(),
        ));
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"config.toml"}"#.to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "[package]\nname = \"test\"".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta(
            "The config file contains a package named \"test\".".to_string(),
        ));
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);

        assert!(
            output.contains("read_file"),
            "Should show tool name: {}",
            output
        );
        assert!(
            output.contains('\u{2713}') || output.contains("✓"),
            "Should show completion checkmark: {}",
            output
        );
        assert_order(&output, "configuration", "read_file");
        assert_order(&output, "read_file", "contains");
    }

    #[test]
    fn multiple_sequential_tool_calls() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("List and read files".to_string()));

        app.on_message(ChatAppMsg::TextDelta("Looking up files...\n\n".to_string()));

        app.on_message(ChatAppMsg::ToolCall {
            name: "glob".to_string(),
            args: r#"{"pattern":"*.rs"}"#.to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "glob".to_string(),
            delta: "main.rs\nlib.rs".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "glob".to_string(),
        });

        app.on_message(ChatAppMsg::TextDelta(
            "Found 2 files. Reading main.rs...\n\n".to_string(),
        ));

        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"main.rs"}"#.to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "fn main() {}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
        });

        app.on_message(ChatAppMsg::TextDelta(
            "The main function is empty.".to_string(),
        ));
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);

        assert_order(&output, "Looking", "glob");
        assert_order(&output, "glob", "Found");
        assert_order(&output, "Found", "read_file");
        assert_order(&output, "read_file", "empty");
    }
}

/// Tests that track tool call positioning during text graduation.
/// These simulate real RPC message flows and capture snapshots at each step.
mod graduation_tracking {
    use super::*;

    fn positions<'a>(output: &str, markers: &[&'a str]) -> Vec<(&'a str, Option<usize>)> {
        markers.iter().map(|m| (*m, output.find(m))).collect()
    }

    /// Simulates a realistic tool use session with large text blocks that trigger graduation.
    /// Captures snapshots at each message to track how tool calls move relative to text.
    #[test]
    fn tool_call_position_during_graduation() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Analyze the file".to_string()));

        // Step 1: Initial text (short, won't graduate)
        app.on_message(ChatAppMsg::TextDelta("BEFORE_TOOL_TEXT\n\n".to_string()));
        let snap1 = render_app(&app);
        assert!(
            snap1.contains("BEFORE_TOOL_TEXT"),
            "Step 1: Should have initial text\n{}",
            snap1
        );

        // Step 2: Tool call arrives
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"test.rs"}"#.to_string(),
        });
        let snap2 = render_app(&app);
        let pos2 = positions(&snap2, &["BEFORE_TOOL_TEXT", "read_file"]);
        assert!(
            pos2[0].1.unwrap() < pos2[1].1.unwrap(),
            "Step 2: Text should appear before tool call\nPositions: {:?}\n{}",
            pos2,
            snap2
        );

        // Step 3: Tool result arrives
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "TOOL_OUTPUT_CONTENT".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
        });
        let snap3 = render_app(&app);
        assert!(
            snap3.contains("✓") || snap3.contains("\u{2713}"),
            "Step 3: Tool should be marked complete\n{}",
            snap3
        );

        // Step 4: Post-tool text (short)
        app.on_message(ChatAppMsg::TextDelta("AFTER_TOOL_TEXT\n\n".to_string()));
        let snap4 = render_app(&app);
        let pos4 = positions(
            &snap4,
            &["BEFORE_TOOL_TEXT", "read_file", "AFTER_TOOL_TEXT"],
        );
        assert!(
            pos4[0].1.unwrap() < pos4[1].1.unwrap() && pos4[1].1.unwrap() < pos4[2].1.unwrap(),
            "Step 4: Order should be text -> tool -> text\nPositions: {:?}\n{}",
            pos4,
            snap4
        );

        // Step 5: Add lots of text to trigger graduation (>15 lines)
        let long_text = (1..=20)
            .map(|i| format!("LINE_{:02}\n", i))
            .collect::<String>();
        app.on_message(ChatAppMsg::TextDelta(long_text));
        let snap5 = render_app(&app);

        // Check that tool call is still in correct position relative to markers
        let pos5 = positions(
            &snap5,
            &["BEFORE_TOOL_TEXT", "read_file", "AFTER_TOOL_TEXT"],
        );
        eprintln!("Step 5 positions: {:?}", pos5);
        eprintln!("Step 5 output:\n{}", snap5);

        // The tool should still be between BEFORE and AFTER markers
        if let (Some(before), Some(tool), Some(after)) = (pos5[0].1, pos5[1].1, pos5[2].1) {
            assert!(
                before < tool && tool < after,
                "Step 5: Tool should remain between text markers after graduation\n\
                 Positions: before={}, tool={}, after={}\n{}",
                before,
                tool,
                after,
                snap5
            );
        } else {
            panic!(
                "Step 5: Missing markers in output\nPositions: {:?}\n{}",
                pos5, snap5
            );
        }

        // Step 6: Complete streaming
        app.on_message(ChatAppMsg::StreamComplete);
        let snap6 = render_app(&app);
        let pos6 = positions(
            &snap6,
            &["BEFORE_TOOL_TEXT", "read_file", "AFTER_TOOL_TEXT"],
        );

        if let (Some(before), Some(tool), Some(after)) = (pos6[0].1, pos6[1].1, pos6[2].1) {
            assert!(
                before < tool && tool < after,
                "Step 6 (final): Tool should remain between text markers\n\
                 Positions: before={}, tool={}, after={}\n{}",
                before,
                tool,
                after,
                snap6
            );
        } else {
            panic!(
                "Step 6 (final): Missing markers in output\nPositions: {:?}\n{}",
                pos6, snap6
            );
        }
    }

    /// Test specifically for the overflow graduation scenario.
    /// When text overflows 15 lines, older lines should graduate to scrollback
    /// but tool calls should maintain their chronological position.
    #[test]
    fn tool_position_stable_through_overflow_graduation() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        // Add initial text that will be visible
        app.on_message(ChatAppMsg::TextDelta("HEADER\n\n".to_string()));

        // Add tool call
        app.on_message(ChatAppMsg::ToolCall {
            name: "test_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "test_tool".to_string(),
        });

        // Add text after tool
        app.on_message(ChatAppMsg::TextDelta("MIDDLE\n\n".to_string()));

        // Now add enough lines to trigger overflow graduation
        for i in 1..=25 {
            app.on_message(ChatAppMsg::TextDelta(format!("overflow_line_{:02}\n", i)));
        }

        let output = render_app(&app);
        eprintln!("Overflow test output:\n{}", output);

        // The order should still be: HEADER -> tool -> MIDDLE -> overflow lines
        // Even if some content has graduated, relative order must be preserved
        let header_pos = output.find("HEADER");
        let tool_pos = output.find("test_tool");
        let middle_pos = output.find("MIDDLE");

        eprintln!(
            "Positions: header={:?}, tool={:?}, middle={:?}",
            header_pos, tool_pos, middle_pos
        );

        // All markers should exist
        assert!(header_pos.is_some(), "HEADER should be in output");
        assert!(tool_pos.is_some(), "test_tool should be in output");
        assert!(middle_pos.is_some(), "MIDDLE should be in output");

        // Order should be preserved
        let header = header_pos.unwrap();
        let tool = tool_pos.unwrap();
        let middle = middle_pos.unwrap();

        assert!(
            header < tool,
            "HEADER ({}) should come before tool ({})",
            header,
            tool
        );
        assert!(
            tool < middle,
            "tool ({}) should come before MIDDLE ({})",
            tool,
            middle
        );
    }
}

/// Tests specifically for content duplication bugs.
/// Ensures that content is never rendered twice in the output.
mod duplicate_content_prevention {
    use super::*;

    /// Helper to count occurrences of a substring
    fn count_occurrences(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    /// Test that text before a tool call is not duplicated
    /// when more text comes after the tool call.
    #[test]
    fn text_before_tool_not_duplicated() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        // Send text, then tool call, then more text
        app.on_message(ChatAppMsg::TextDelta("UNIQUE_MARKER_XYZ ".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "my_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "my_tool".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("AFTER_TOOL".to_string()));

        let output = render_app(&app);
        let count = count_occurrences(&output, "UNIQUE_MARKER_XYZ");
        assert_eq!(
            count, 1,
            "UNIQUE_MARKER_XYZ should appear exactly once, but found {} times.\nOutput:\n{}",
            count, output
        );
    }

    #[test]
    fn no_duplicate_content_during_streaming() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta(
            "Here are the tools I can use:\n\n".to_string(),
        ));
        app.on_message(ChatAppMsg::TextDelta(
            "| Tool | Description |\n".to_string(),
        ));
        app.on_message(ChatAppMsg::TextDelta(
            "|------|-------------|\n".to_string(),
        ));
        app.on_message(ChatAppMsg::TextDelta(
            "| read | Read files  |\n\n".to_string(),
        ));

        app.on_message(ChatAppMsg::ToolCall {
            name: "example_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "example_tool".to_string(),
        });

        app.on_message(ChatAppMsg::TextDelta(
            "Let me know what you'd like to do next!".to_string(),
        ));

        let output = render_app(&app);

        let table_count = count_occurrences(&output, "Here are the tools");
        assert_eq!(
            table_count, 1,
            "Table intro should appear exactly once, but found {} times.\nOutput:\n{}",
            table_count, output
        );

        let tool_count = count_occurrences(&output, "Tool");
        assert!(
            tool_count >= 1,
            "Tool column header should appear at least once.\nOutput:\n{}",
            output
        );

        let desc_count = count_occurrences(&output, "Description");
        assert_eq!(
            desc_count, 1,
            "Description should appear exactly once.\nOutput:\n{}",
            output
        );
    }

    /// Test that content is not duplicated after stream completion.
    #[test]
    fn no_duplicate_content_after_completion() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("FIRST_BLOCK\n\n".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "tool1".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool1".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("SECOND_BLOCK".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);

        assert_eq!(
            count_occurrences(&output, "FIRST_BLOCK"),
            1,
            "FIRST_BLOCK should appear exactly once.\nOutput:\n{}",
            output
        );
        assert_eq!(
            count_occurrences(&output, "SECOND_BLOCK"),
            1,
            "SECOND_BLOCK should appear exactly once.\nOutput:\n{}",
            output
        );
    }

    /// Test with subagent events to ensure they don't cause duplication.
    #[test]
    fn subagent_events_dont_cause_duplication() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("INTRO_TEXT\n\n".to_string()));
        app.on_message(ChatAppMsg::SubagentSpawned {
            id: "sub-1".to_string(),
            prompt: "test subagent".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("MIDDLE_TEXT\n\n".to_string()));
        app.on_message(ChatAppMsg::SubagentCompleted {
            id: "sub-1".to_string(),
            summary: "Completed successfully".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("FINAL_TEXT".to_string()));

        let output = render_app(&app);

        assert_eq!(
            count_occurrences(&output, "INTRO_TEXT"),
            1,
            "INTRO_TEXT should appear exactly once.\nOutput:\n{}",
            output
        );
        assert_eq!(
            count_occurrences(&output, "MIDDLE_TEXT"),
            1,
            "MIDDLE_TEXT should appear exactly once.\nOutput:\n{}",
            output
        );
        assert_eq!(
            count_occurrences(&output, "FINAL_TEXT"),
            1,
            "FINAL_TEXT should appear exactly once.\nOutput:\n{}",
            output
        );
    }

    /// Test with longer content that triggers graduation.
    #[test]
    fn graduation_doesnt_cause_duplication() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        // Add text that will graduate (multiple paragraphs)
        app.on_message(ChatAppMsg::TextDelta("PARA_ONE\n\n".to_string()));
        app.on_message(ChatAppMsg::TextDelta("PARA_TWO\n\n".to_string()));
        app.on_message(ChatAppMsg::TextDelta("PARA_THREE\n\n".to_string()));

        // Tool call
        app.on_message(ChatAppMsg::ToolCall {
            name: "test_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "test_tool".to_string(),
        });

        // More text after
        app.on_message(ChatAppMsg::TextDelta("PARA_FOUR\n\n".to_string()));
        app.on_message(ChatAppMsg::TextDelta("PARA_FIVE".to_string()));

        let output = render_app(&app);

        for marker in &[
            "PARA_ONE",
            "PARA_TWO",
            "PARA_THREE",
            "PARA_FOUR",
            "PARA_FIVE",
        ] {
            let count = count_occurrences(&output, marker);
            assert_eq!(
                count, 1,
                "{} should appear exactly once, but found {} times.\nOutput:\n{}",
                marker, count, output
            );
        }
    }

    #[test]
    fn only_one_bullet_per_assistant_response() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta(
            "First part of response\n\n".to_string(),
        ));
        app.on_message(ChatAppMsg::ToolCall {
            name: "tool1".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool1".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("Second part of response".to_string()));

        let output = render_app(&app);

        let bullet_count = count_occurrences(&output, "●");
        assert_eq!(
            bullet_count, 1,
            "Should have exactly one bullet for assistant response, found {}.\nOutput:\n{}",
            bullet_count, output
        );
    }

    #[test]
    fn only_one_bullet_after_stream_complete() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("First part\n\n".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "tool1".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool1".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("Second part".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);

        let bullet_count = count_occurrences(&output, "●");
        assert_eq!(
            bullet_count, 1,
            "Should have exactly one bullet for assistant response after completion, found {}.\nOutput:\n{}",
            bullet_count, output
        );
    }

    #[test]
    fn only_one_bullet_with_subagent() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("Before subagent\n\n".to_string()));
        app.on_message(ChatAppMsg::SubagentSpawned {
            id: "sub-1".to_string(),
            prompt: "test".to_string(),
        });
        app.on_message(ChatAppMsg::SubagentCompleted {
            id: "sub-1".to_string(),
            summary: "done".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("After subagent".to_string()));

        let output = render_app(&app);

        let bullet_count = count_occurrences(&output, "●");
        assert_eq!(
            bullet_count, 1,
            "Should have exactly one bullet with subagent, found {}.\nOutput:\n{}",
            bullet_count, output
        );
    }

    #[test]
    fn streaming_to_final_no_stdout_duplication() {
        use crate::tui::oil::app::{App, ViewContext};
        use crate::tui::oil::focus::FocusContext;

        let mut app = OilChatApp::default();
        let mut runtime = TestRuntime::new(120, 40);

        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta(
            "MARKER_PARA_ONE first paragraph content.\n\n".to_string(),
        ));
        app.on_message(ChatAppMsg::TextDelta(
            "MARKER_PARA_TWO second paragraph content.\n\n".to_string(),
        ));
        app.on_message(ChatAppMsg::TextDelta(
            "MARKER_PARA_THREE third paragraph content.\n\n".to_string(),
        ));

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        runtime.render(&tree);

        let stdout_during_streaming = runtime.stdout_content().to_string();
        assert!(
            stdout_during_streaming.contains("MARKER_PARA_ONE"),
            "Graduated content should be in stdout during streaming"
        );

        app.on_message(ChatAppMsg::TextDelta(
            "MARKER_FINAL_PARA final paragraph.".to_string(),
        ));
        app.on_message(ChatAppMsg::StreamComplete);

        // With container-based rendering, we don't need pre_graduate_keys.
        // Container blocks have stable IDs that the runtime tracks - once graduated,
        // they won't be output again.

        let tree2 = app.view(&ctx);
        runtime.render(&tree2);

        let final_stdout = runtime.stdout_content();

        for marker in &["MARKER_PARA_ONE", "MARKER_PARA_TWO", "MARKER_PARA_THREE"] {
            let count = count_occurrences(final_stdout, marker);
            assert_eq!(
                count, 1,
                "{} should appear exactly once in stdout (was graduated during streaming, \
                 should be skipped after completion). Found {} times.\n\
                 stdout:\n{}",
                marker, count, final_stdout
            );
        }

        let final_para_count = count_occurrences(final_stdout, "MARKER_FINAL_PARA");
        assert_eq!(
            final_para_count, 1,
            "Final paragraph should appear exactly once. Found {} times.\nstdout:\n{}",
            final_para_count, final_stdout
        );
    }
}
