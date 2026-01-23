use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::Role;
use crate::tui::oil::chat_app::{ChatAppMsg, InkChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::viewport_cache::{CachedChatItem, ViewportCache};
use crate::tui::oil::TestRuntime;

fn render_app(app: &InkChatApp) -> String {
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

mod viewport_cache_ordering {
    use super::*;

    #[test]
    fn items_pushed_during_streaming_maintain_order() {
        let mut cache = ViewportCache::new();

        cache.start_streaming();
        cache.append_streaming("First text ");
        cache.push_streaming_tool_call("tool-1".to_string());
        cache.push_tool_call("tool-1".to_string(), "read_file", r#"{"path":"a.rs"}"#);
        cache.append_streaming("Second text");

        let segments: Vec<_> = cache
            .streaming_segments()
            .unwrap()
            .iter()
            .map(|s| format!("{:?}", s))
            .collect();

        assert!(
            segments.iter().any(|s| s.contains("First text")),
            "Should have first text segment: {:?}",
            segments
        );
        assert!(
            segments.iter().any(|s| s.contains("tool-1")),
            "Should have tool call segment: {:?}",
            segments
        );
    }

    #[test]
    fn complete_streaming_preserves_segment_order() {
        let mut cache = ViewportCache::new();

        cache.start_streaming();
        cache.append_streaming("AAA_FIRST ");
        cache.push_streaming_tool_call("tool-1".to_string());
        cache.push_tool_call("tool-1".to_string(), "my_tool", "{}");
        cache.append_streaming("BBB_SECOND");

        cache.complete_streaming("msg-1".to_string(), Role::Assistant);

        let items: Vec<_> = cache.items().collect();
        let item_ids: Vec<_> = items.iter().map(|i| i.id()).collect();

        assert!(
            items.len() >= 2,
            "Should have at least 2 items (text + tool), got {:?}",
            item_ids
        );

        let mut found_first_text = false;
        let mut found_tool = false;
        let mut found_second_text = false;
        let mut tool_before_second = false;

        for item in &items {
            match item {
                CachedChatItem::Message(m) => {
                    let content = m.content();
                    if content.contains("AAA_FIRST") {
                        found_first_text = true;
                        assert!(
                            !found_tool,
                            "First text should come before tool. Items: {:?}",
                            item_ids
                        );
                    }
                    if content.contains("BBB_SECOND") {
                        found_second_text = true;
                        if found_tool {
                            tool_before_second = true;
                        }
                    }
                }
                CachedChatItem::ToolCall(t) => {
                    found_tool = true;
                    assert!(
                        found_first_text,
                        "Tool should come after first text. Items: {:?}",
                        item_ids
                    );
                    assert_eq!(t.name.as_ref(), "my_tool");
                }
                _ => {}
            }
        }

        assert!(found_first_text, "Should find first text");
        assert!(found_tool, "Should find tool");
        assert!(found_second_text, "Should find second text");
        assert!(tool_before_second, "Tool should be before second text");
    }

    #[test]
    fn multiple_tools_maintain_chronological_order() {
        let mut cache = ViewportCache::new();

        cache.start_streaming();
        cache.append_streaming("TEXT_1 ");

        cache.push_streaming_tool_call("tool-1".to_string());
        cache.push_tool_call("tool-1".to_string(), "first_tool", "{}");

        cache.append_streaming("TEXT_2 ");

        cache.push_streaming_tool_call("tool-2".to_string());
        cache.push_tool_call("tool-2".to_string(), "second_tool", "{}");

        cache.append_streaming("TEXT_3");

        cache.complete_streaming("msg-1".to_string(), Role::Assistant);

        let items: Vec<_> = cache.items().collect();
        let descriptions: Vec<String> = items
            .iter()
            .map(|i| match i {
                CachedChatItem::Message(m) => format!("Msg({})", m.content()),
                CachedChatItem::ToolCall(t) => format!("Tool({})", t.name),
                CachedChatItem::ShellExecution(s) => format!("Shell({})", s.command),
            })
            .collect();

        let first_tool_idx = items.iter().position(
            |i| matches!(i, CachedChatItem::ToolCall(t) if t.name.as_ref() == "first_tool"),
        );
        let second_tool_idx = items.iter().position(
            |i| matches!(i, CachedChatItem::ToolCall(t) if t.name.as_ref() == "second_tool"),
        );

        assert!(
            first_tool_idx.is_some(),
            "first_tool not found. Items: {:?}",
            descriptions
        );
        assert!(
            second_tool_idx.is_some(),
            "second_tool not found. Items: {:?}",
            descriptions
        );
        assert!(
            first_tool_idx.unwrap() < second_tool_idx.unwrap(),
            "first_tool should come before second_tool. Items: {:?}",
            descriptions
        );
    }
}

mod chat_app_message_handling {
    use super::*;

    #[test]
    fn tool_call_message_creates_tool_in_cache() {
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();
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
        let mut app = InkChatApp::default();

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
        let mut app = InkChatApp::default();
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
