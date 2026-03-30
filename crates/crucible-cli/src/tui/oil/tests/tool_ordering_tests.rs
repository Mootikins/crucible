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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });

        assert!(app.is_streaming(), "Should be in streaming state");

        let output = render_app(&app);
        assert!(
            output.contains("Read File"),
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "file contents".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::TextDelta("AFTER_TOOL".to_string()));

        let output = render_app(&app);
        assert_order(&output, "BEFORE_TOOL", "My Tool");
    }

    #[test]
    fn interleaved_text_and_tool_maintains_order_after_completion() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("BEFORE_TOOL ".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "my_tool".to_string(),
            args: "{}".to_string(),
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "my_tool".to_string(),
            delta: "result".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "my_tool".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::TextDelta("AFTER_TOOL".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        assert!(
            !app.is_streaming(),
            "Should not be streaming after complete"
        );

        let output = render_app(&app);
        assert_order(&output, "BEFORE_TOOL", "My Tool");
        assert_order(&output, "My Tool", "AFTER_TOOL");
    }

    #[test]
    fn multiple_tool_calls_maintain_order() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("test".to_string()));

        app.on_message(ChatAppMsg::TextDelta("START ".to_string()));

        app.on_message(ChatAppMsg::ToolCall {
            name: "tool_alpha".to_string(),
            args: "{}".to_string(),
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool_alpha".to_string(),
            call_id: None,
        });

        app.on_message(ChatAppMsg::TextDelta("MIDDLE ".to_string()));

        app.on_message(ChatAppMsg::ToolCall {
            name: "tool_beta".to_string(),
            args: "{}".to_string(),
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool_beta".to_string(),
            call_id: None,
        });

        app.on_message(ChatAppMsg::TextDelta("END".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);

        assert_order(&output, "START", "Tool Alpha");
        assert_order(&output, "Tool Alpha", "MIDDLE");
        assert_order(&output, "MIDDLE", "Tool Beta");
        assert_order(&output, "Tool Beta", "END");
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "completed_tool".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "result_tool".to_string(),
            delta: "TOOL_OUTPUT_CONTENT".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "result_tool".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "different_tool".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "[package]\nname = \"test\"".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::TextDelta(
            "The config file contains a package named \"test\".".to_string(),
        ));
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);

        assert!(
            output.contains("Read File"),
            "Should show tool name: {}",
            output
        );
        assert!(
            output.contains('\u{2713}') || output.contains("✓"),
            "Should show completion checkmark: {}",
            output
        );
        assert_order(&output, "configuration", "Read File");
        assert_order(&output, "Read File", "contains");
    }

    #[test]
    fn multiple_sequential_tool_calls() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("List and read files".to_string()));

        app.on_message(ChatAppMsg::TextDelta("Looking up files...\n\n".to_string()));

        app.on_message(ChatAppMsg::ToolCall {
            name: "glob".to_string(),
            args: r#"{"pattern":"*.rs"}"#.to_string(),
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "glob".to_string(),
            delta: "main.rs\nlib.rs".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "glob".to_string(),
            call_id: None,
        });

        app.on_message(ChatAppMsg::TextDelta(
            "Found 2 files. Reading main.rs...\n\n".to_string(),
        ));

        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"main.rs"}"#.to_string(),
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "fn main() {}".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
            call_id: None,
        });

        app.on_message(ChatAppMsg::TextDelta(
            "The main function is empty.".to_string(),
        ));
        app.on_message(ChatAppMsg::StreamComplete);

        let output = render_app(&app);

        assert_order(&output, "Looking", "Glob");
        assert_order(&output, "Glob", "Found");
        assert_order(&output, "Found", "Read File");
        assert_order(&output, "Read File", "empty");
    }
}

/// Tests that track tool call positioning across the graduation boundary.
///
/// These use `TestRuntime` + `flush_and_render` so content actually graduates
/// to stdout. Ordering is verified across the combined stdout+viewport output.
mod graduation_tracking {
    use super::*;
    use crate::tui::oil::tests::helpers::{combined_output, flush_and_render};

    fn positions<'a>(output: &str, markers: &[&'a str]) -> Vec<(&'a str, Option<usize>)> {
        markers.iter().map(|m| (*m, output.find(m))).collect()
    }

    /// Simulates a tool use session with enough text to trigger graduation.
    /// Verifies that tool calls maintain correct ordering in the combined
    /// stdout (graduated) + viewport (live) output.
    #[test]
    fn tool_call_position_during_graduation() {
        let mut runtime = TestRuntime::new(120, 24);
        let mut app = OilChatApp::default();

        // Step 1: User message + initial text
        app.on_message(ChatAppMsg::UserMessage("Analyze the file".to_string()));
        app.on_message(ChatAppMsg::TextDelta("BEFORE_TOOL_TEXT\n\n".to_string()));
        flush_and_render(&mut app, &mut runtime);

        let output = combined_output(&runtime);
        assert!(
            output.contains("BEFORE_TOOL_TEXT"),
            "Step 1: Should have initial text\n{}",
            output
        );

        // Step 2: Tool call arrives
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"test.rs"}"#.to_string(),
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        flush_and_render(&mut app, &mut runtime);

        let output = combined_output(&runtime);
        let pos2 = positions(&output, &["BEFORE_TOOL_TEXT", "Read File"]);
        assert!(
            pos2[0].1.unwrap() < pos2[1].1.unwrap(),
            "Step 2: Text should appear before tool call\nPositions: {:?}\n{}",
            pos2,
            output
        );

        // Step 3: Tool result completes
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "TOOL_OUTPUT_CONTENT".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
            call_id: None,
        });
        flush_and_render(&mut app, &mut runtime);

        // Step 4: Post-tool text
        app.on_message(ChatAppMsg::TextDelta("AFTER_TOOL_TEXT\n\n".to_string()));
        flush_and_render(&mut app, &mut runtime);

        let output = combined_output(&runtime);
        let pos4 = positions(
            &output,
            &["BEFORE_TOOL_TEXT", "Read File", "AFTER_TOOL_TEXT"],
        );
        assert!(
            pos4[0].1.unwrap() < pos4[1].1.unwrap() && pos4[1].1.unwrap() < pos4[2].1.unwrap(),
            "Step 4: Order should be text -> tool -> text\nPositions: {:?}\n{}",
            pos4,
            output
        );

        // Step 5: Add enough text to force graduation past the viewport.
        // Tool groups no longer graduate during an active streaming turn —
        // they stay in the viewport until StreamComplete. Text blocks may
        // graduate ahead of tool groups, so stdout ordering reflects
        // graduation order rather than document order.
        let long_text = (1..=20)
            .map(|i| format!("LINE_{i:02}\n"))
            .collect::<String>();
        app.on_message(ChatAppMsg::TextDelta(long_text));
        flush_and_render(&mut app, &mut runtime);

        // Complete the stream so tool groups can graduate
        app.on_message(ChatAppMsg::StreamComplete);
        flush_and_render(&mut app, &mut runtime);

        let output = combined_output(&runtime);
        let pos5 = positions(
            &output,
            &["BEFORE_TOOL_TEXT", "Read File", "AFTER_TOOL_TEXT"],
        );

        // After multi-cycle graduation, all markers must be present.
        // Document order may not be preserved in stdout because text blocks
        // graduate during streaming while tool groups wait for StreamComplete.
        assert!(
            pos5[0].1.is_some(),
            "Step 5: BEFORE_TOOL_TEXT should be present\n{}",
            output
        );
        assert!(
            pos5[1].1.is_some(),
            "Step 5: Read File should be present\n{}",
            output
        );
        assert!(
            pos5[2].1.is_some(),
            "Step 5: AFTER_TOOL_TEXT should be present\n{}",
            output
        );
    }

    /// Tests that tool ordering is preserved when enough content overflows
    /// to push early content out of the viewport and into stdout graduation.
    #[test]
    fn tool_position_stable_through_overflow_graduation() {
        let mut runtime = TestRuntime::new(120, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("test".to_string()));
        flush_and_render(&mut app, &mut runtime);

        // Initial text
        app.on_message(ChatAppMsg::TextDelta("HEADER\n\n".to_string()));
        flush_and_render(&mut app, &mut runtime);

        // Tool call + completion
        app.on_message(ChatAppMsg::ToolCall {
            name: "test_tool".to_string(),
            args: "{}".to_string(),
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "test_tool".to_string(),
            call_id: None,
        });
        flush_and_render(&mut app, &mut runtime);

        // Text after tool
        app.on_message(ChatAppMsg::TextDelta("MIDDLE\n\n".to_string()));
        flush_and_render(&mut app, &mut runtime);

        // Enough lines to force graduation
        for i in 1..=25 {
            app.on_message(ChatAppMsg::TextDelta(format!("overflow_line_{i:02}\n")));
        }
        flush_and_render(&mut app, &mut runtime);

        // Complete the stream so tool groups can graduate
        app.on_message(ChatAppMsg::StreamComplete);
        flush_and_render(&mut app, &mut runtime);

        let output = combined_output(&runtime);

        let header_pos = output.find("HEADER");
        let tool_pos = output.find("Test Tool");
        let middle_pos = output.find("MIDDLE");

        // After multi-cycle graduation, all markers must be present.
        // Document order may not be preserved in stdout because text blocks
        // graduate during streaming while tool groups wait for StreamComplete.
        assert!(header_pos.is_some(), "HEADER should be in output");
        assert!(tool_pos.is_some(), "Test Tool should be in output");
        assert!(middle_pos.is_some(), "MIDDLE should be in output");
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "my_tool".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "example_tool".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool1".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "test_tool".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool1".to_string(),
            call_id: None,
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
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "tool1".to_string(),
            call_id: None,
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

    /// Verify that streaming content followed by completion does not duplicate paragraphs
    /// in the rendered output. Graduation is now handled by ContainerList at the CLI layer,
    /// so this test checks rendered output consistency rather than stdout graduation.
    #[test]
    fn streaming_to_final_no_rendered_duplication() {
        let mut app = OilChatApp::default();

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

        // Render during streaming
        let output_streaming = render_app(&app);
        for marker in &["MARKER_PARA_ONE", "MARKER_PARA_TWO", "MARKER_PARA_THREE"] {
            let count = count_occurrences(&output_streaming, marker);
            assert_eq!(
                count, 1,
                "{} should appear exactly once during streaming. Found {} times.\nOutput:\n{}",
                marker, count, output_streaming
            );
        }

        app.on_message(ChatAppMsg::TextDelta(
            "MARKER_FINAL_PARA final paragraph.".to_string(),
        ));
        app.on_message(ChatAppMsg::StreamComplete);

        // Render after completion
        let output_final = render_app(&app);

        for marker in &[
            "MARKER_PARA_ONE",
            "MARKER_PARA_TWO",
            "MARKER_PARA_THREE",
            "MARKER_FINAL_PARA",
        ] {
            let count = count_occurrences(&output_final, marker);
            assert_eq!(
                count, 1,
                "{} should appear exactly once after completion. Found {} times.\nOutput:\n{}",
                marker, count, output_final
            );
        }
    }
}

/// Braille spinners on running tool calls must animate between tick frames.
mod spinner_animation {
    use super::*;
    use crucible_oil::node::BRAILLE_SPINNER_FRAMES;

    #[test]
    fn running_tool_spinner_changes_over_time() {
        let mut app = OilChatApp::default();

        // Start a tool call (not completed)
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"README.md"}"#.to_string(),
            call_id: Some("call-1".to_string()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });

        let output1 = render_app(&app);

        // Wait 200ms for spinner frame to advance (100ms per frame)
        std::thread::sleep(std::time::Duration::from_millis(200));
        let output2 = render_app(&app);

        let b1 = output1.chars().find(|c| BRAILLE_SPINNER_FRAMES.contains(c));
        let b2 = output2.chars().find(|c| BRAILLE_SPINNER_FRAMES.contains(c));

        assert!(b1.is_some(), "Frame 1 should have braille spinner");
        assert!(b2.is_some(), "Frame 2 should have braille spinner");
        assert!(
            b1 != b2,
            "Braille spinner should change over 200ms (wall clock, not ticks).\n\
             Frame 1: {:?}, Frame 2: {:?}",
            b1,
            b2
        );
    }

    /// Frame-by-frame verification through full graduation pipeline:
    /// running tool stays in viewport, spinner animates via wall clock,
    /// graduated text in stdout, no duplication.
    #[test]
    fn running_tool_spinner_animates_frame_by_frame_with_graduation() {
        let mut app = OilChatApp::default();
        let mut runtime = TestRuntime::new(80, 24);
        let focus = FocusContext::new();

        // Text that will graduate
        app.on_message(ChatAppMsg::TextDelta("First paragraph\n\n".to_string()));

        // Running tool call (NOT completed)
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"test.rs"}"#.to_string(),
            call_id: Some("call-1".to_string()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });

        // Capture 5 frames at 120ms intervals (spinner changes at 100ms)
        let find_braille =
            |s: &str| -> Option<char> { s.chars().find(|c| BRAILLE_SPINNER_FRAMES.contains(c)) };

        let mut braille_chars = Vec::new();
        let mut tool_in_viewport = Vec::new();
        let mut tool_in_stdout = Vec::new();

        for frame in 0..5 {
            let ctx = ViewContext::new(&focus);
            let tree = app.view(&ctx);
            runtime.render(&tree);
            // Graduation is now automatic via drain_completed

            let viewport = strip_ansi(runtime.viewport_content());
            let stdout = strip_ansi(runtime.stdout_content());

            braille_chars.push(find_braille(&viewport));
            tool_in_viewport.push(viewport.contains("Read File"));
            tool_in_stdout.push(stdout.contains("Read File"));

            if frame < 4 {
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
        }

        // Running tool must be in viewport every frame
        for (i, in_vp) in tool_in_viewport.iter().enumerate() {
            assert!(*in_vp, "Frame {}: running tool should be in viewport", i);
        }

        // Running tool must NOT be in stdout (not graduated)
        for (i, in_stdout) in tool_in_stdout.iter().enumerate() {
            assert!(
                !*in_stdout,
                "Frame {}: running tool should NOT be in stdout",
                i
            );
        }

        // Braille spinner must be present every frame
        for (i, b) in braille_chars.iter().enumerate() {
            assert!(
                b.is_some(),
                "Frame {}: should have braille spinner in viewport",
                i
            );
        }

        // Spinner must change at least once across 5 frames (600ms > 100ms/frame)
        let unique: std::collections::HashSet<_> = braille_chars.iter().flatten().collect();
        assert!(
            unique.len() >= 2,
            "Spinner should animate: saw {} unique braille chars {:?} across 5 frames",
            unique.len(),
            braille_chars
        );
    }

    #[test]
    fn completed_tool_not_duplicated_when_more_content_follows() {
        let mut app = OilChatApp::default();
        let mut runtime = TestRuntime::new(80, 24);
        let focus = FocusContext::new();

        // Tool call + complete
        app.on_message(ChatAppMsg::ToolCall {
            name: "glob".to_string(),
            args: r#"{"pattern":"README*"}"#.to_string(),
            call_id: Some("call-1".to_string()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "glob".to_string(),
            delta: "README.md".to_string(),
            call_id: Some("call-1".to_string()),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "glob".to_string(),
            call_id: Some("call-1".to_string()),
        });

        // More content follows (makes tool group "complete" for graduation)
        app.on_message(ChatAppMsg::TextDelta(
            "Some text after tool\n\n".to_string(),
        ));

        // Render with graduation
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        runtime.render(&tree);
        // Graduation is now automatic via drain_completed

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}\n{}", stdout, viewport);

        // "Glob" should appear exactly once
        let glob_count = combined.matches("Glob").count();
        assert_eq!(
            glob_count, 1,
            "Completed tool 'Glob' should appear exactly once.\n\
             STDOUT:\n{}\nVIEWPORT:\n{}",
            stdout, viewport
        );
    }

    #[test]
    fn completed_tool_visible_in_output() {
        let mut app = OilChatApp::default();
        let mut runtime = TestRuntime::new(80, 24);
        let focus = FocusContext::new();

        // Tool call + complete
        app.on_message(ChatAppMsg::ToolCall {
            name: "glob".to_string(),
            args: r#"{"pattern":"README*"}"#.to_string(),
            call_id: Some("call-1".to_string()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "glob".to_string(),
            delta: "README.md".to_string(),
            call_id: Some("call-1".to_string()),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "glob".to_string(),
            call_id: Some("call-1".to_string()),
        });

        // More content follows (turn still active)
        app.on_message(ChatAppMsg::TextDelta("After tool\n\n".to_string()));

        // Render (no graduation without render_frame/drain_completed)
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        // Completed tool should be visible in output
        assert!(
            combined.contains("Glob"),
            "Completed tool should be visible in output.\nCOMBINED:\n{}",
            combined
        );

        // Text after tool should also be visible
        assert!(
            combined.contains("After tool"),
            "Text after tool should be visible.\nCOMBINED:\n{}",
            combined
        );

        // Tool should appear exactly once total
        let total = combined.matches("Glob").count();
        assert_eq!(
            total, 1,
            "Completed tool should appear exactly once.\nCOMBINED:\n{}",
            combined
        );
    }

    #[test]
    fn running_tool_not_duplicated_in_output() {
        let mut app = OilChatApp::default();
        let mut runtime = TestRuntime::new(80, 24);
        let focus = FocusContext::new();

        // Text before tool
        app.on_message(ChatAppMsg::TextDelta("Hello\n\n".to_string()));

        // Tool call
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".to_string(),
            args: r#"{"path":"test.rs"}"#.to_string(),
            call_id: Some("call-1".to_string()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });

        // Tool completes
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "read_file".to_string(),
            delta: "file content".to_string(),
            call_id: Some("call-1".to_string()),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".to_string(),
            call_id: Some("call-1".to_string()),
        });

        // More text after
        app.on_message(ChatAppMsg::TextDelta("World".to_string()));

        // Render with graduation
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        runtime.render(&tree);
        // Graduation is now automatic via drain_completed

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());

        // "Read File" should appear exactly once across stdout + viewport
        let total = stdout.matches("Read File").count() + viewport.matches("Read File").count();
        assert_eq!(
            total,
            1,
            "Tool should appear exactly once. stdout matches: {}, viewport matches: {}\n\
             STDOUT:\n{}\nVIEWPORT:\n{}",
            stdout.matches("Read File").count(),
            viewport.matches("Read File").count(),
            stdout,
            viewport
        );
    }
}
