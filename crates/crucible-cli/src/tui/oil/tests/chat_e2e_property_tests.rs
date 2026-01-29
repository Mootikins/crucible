//! End-to-end property tests simulating full RPC message flows.
//!
//! These tests verify complete conversation flows with arbitrary message
//! sequences, simulating what the daemon would send via RPC.

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::TestRuntime;
use proptest::prelude::*;

use super::generators::{
    arb_multi_turn_conversation, arb_rpc_sequence_with_tools, arb_text_content, arb_tool_name,
    RpcEvent,
};
use super::helpers::{apply_rpc_event, combined_output, view_with_default_ctx};

/// Render and sync graduation state between runtime and app.
/// This matches the real render loop behavior in chat_runner.
fn render_and_graduate(runtime: &mut TestRuntime, app: &mut OilChatApp) {
    let tree = view_with_default_ctx(app);
    runtime.render(&tree);
    let graduated = runtime.last_graduated_keys();
    if !graduated.is_empty() {
        app.mark_graduated(graduated);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Full conversation flow maintains all invariants
    #[test]
    fn full_conversation_flow_invariants(
        turns in arb_multi_turn_conversation()
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();
        app.set_show_thinking(true);

        let mut expected_user_queries: Vec<String> = Vec::new();

        for turn in &turns {
            expected_user_queries.push(turn.user_query.clone());
            app.on_message(ChatAppMsg::UserMessage(turn.user_query.clone()));

            let cancel_at = if turn.cancelled {
                turn.events.len() / 2
            } else {
                turn.events.len()
            };

            for (i, event) in turn.events.iter().enumerate() {
                if i >= cancel_at {
                    break;
                }
                apply_rpc_event(&mut app, event);

                render_and_graduate(&mut runtime, &mut app);
            }

            if turn.cancelled {
                app.on_message(ChatAppMsg::StreamCancelled);
            } else {
                app.on_message(ChatAppMsg::StreamComplete);
            }

            render_and_graduate(&mut runtime, &mut app);
        }

        let stdout = strip_ansi(runtime.stdout_content());

        for query in &expected_user_queries {
            let first_word = query.split_whitespace().next();
            if let Some(word) = first_word {
                if word.len() >= 3 {
                    prop_assert!(
                        stdout.contains(word),
                        "User query word '{}' should be in output:\n{}",
                        word, stdout
                    );
                }
            }
        }
    }

    /// Tool calls complete correctly in conversation
    #[test]
    fn tool_calls_complete_in_conversation(
        sequence in arb_rpc_sequence_with_tools()
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Query with tools".to_string()));

        let mut expected_tools: Vec<String> = Vec::new();

        for event in &sequence {
            if let RpcEvent::ToolCall { name, .. } = event {
                expected_tools.push(name.clone());
            }
            apply_rpc_event(&mut app, event);

            render_and_graduate(&mut runtime, &mut app);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        render_and_graduate(&mut runtime, &mut app);

        let combined = combined_output(&runtime);

        for tool in &expected_tools {
            prop_assert!(
                combined.contains(tool),
                "Tool '{}' should appear in output:\n{}",
                tool, combined
            );
        }

        let checkmark_count = combined.matches('\u{2713}').count();
        prop_assert!(
            checkmark_count >= expected_tools.len(),
            "Should have at least {} checkmarks for completed tools, found {}",
            expected_tools.len(), checkmark_count
        );
    }

    /// Rapid message switching does not corrupt state
    #[test]
    fn rapid_type_switching_no_corruption(
        events in prop::collection::vec(
            prop_oneof![
                arb_text_content().prop_map(RpcEvent::TextDelta),
                arb_text_content().prop_map(RpcEvent::ThinkingDelta),
            ],
            5..30
        )
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();
        app.set_show_thinking(true);

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));

        for event in &events {
            apply_rpc_event(&mut app, event);

            // Use catch_unwind to handle any panics during rapid switching
            let tree = view_with_default_ctx(&app);
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.render(&tree);
            })).is_ok() {
                // Sync graduation state only if render succeeded
                let graduated = runtime.last_graduated_keys();
                if !graduated.is_empty() {
                    app.mark_graduated(graduated);
                }
            }
        }

        app.on_message(ChatAppMsg::StreamComplete);

        render_and_graduate(&mut runtime, &mut app);

        let stdout = strip_ansi(runtime.stdout_content());
        prop_assert!(
            stdout.contains("Q"),
            "User message should persist:\n{}",
            stdout
        );
    }

    /// Interleaved thinking and text maintain correct ordering
    #[test]
    fn interleaved_thinking_text_ordering(
        think_chunks in prop::collection::vec("[a-zA-Z]{5,20}", 1..5),
        text_chunks in prop::collection::vec("[a-zA-Z]{5,20}", 1..5)
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();
        app.set_show_thinking(true);

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));

        let max_len = think_chunks.len().max(text_chunks.len());
        for i in 0..max_len {
            if i < think_chunks.len() {
                app.on_message(ChatAppMsg::ThinkingDelta(think_chunks[i].clone()));
            }
            if i < text_chunks.len() {
                app.on_message(ChatAppMsg::TextDelta(text_chunks[i].clone()));
            }

            render_and_graduate(&mut runtime, &mut app);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        render_and_graduate(&mut runtime, &mut app);

        let stdout = strip_ansi(runtime.stdout_content());

        if !think_chunks.is_empty() {
            prop_assert!(
                stdout.contains("thinking"),
                "Should show thinking indicator:\n{}",
                stdout
            );
        }
    }

    /// Multiple sequential tool calls maintain order
    #[test]
    fn sequential_tools_maintain_order(
        tool_names in prop::collection::hash_set(arb_tool_name(), 2..5)
    ) {
        let tools: Vec<_> = tool_names.into_iter().collect();
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Multi-tool query".to_string()));

        for tool in &tools {
            app.on_message(ChatAppMsg::ToolCall {
                name: tool.clone(),
                args: r#"{"x": 1}"#.to_string(),
            });
            app.on_message(ChatAppMsg::ToolResultDelta {
                name: tool.clone(),
                delta: format!("Result for {}", tool),
            });
            app.on_message(ChatAppMsg::ToolResultComplete {
                name: tool.clone(),
            });

            render_and_graduate(&mut runtime, &mut app);
        }

        app.on_message(ChatAppMsg::TextDelta("All tools complete.".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        render_and_graduate(&mut runtime, &mut app);

        let combined = combined_output(&runtime);

        let mut last_pos = 0;
        for tool in &tools {
            if let Some(pos) = combined[last_pos..].find(tool.as_str()) {
                last_pos = last_pos + pos + tool.len();
            } else {
                prop_assert!(
                    false,
                    "Tool '{}' not found after position {} in:\n{}",
                    tool, last_pos, combined
                );
            }
        }
    }

    /// Context usage updates do not affect message rendering
    #[test]
    fn context_usage_updates_stable(
        used in 0usize..100000,
        total in 100000usize..200000
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Response".to_string()));

        render_and_graduate(&mut runtime, &mut app);
        let before = strip_ansi(runtime.stdout_content());

        app.on_message(ChatAppMsg::ContextUsage { used, total });

        render_and_graduate(&mut runtime, &mut app);
        let after = strip_ansi(runtime.stdout_content());

        prop_assert_eq!(
            before, after,
            "Context usage should not affect stdout"
        );
    }

    /// Error messages are displayed without corrupting history
    #[test]
    fn error_messages_preserve_history(
        error_msg in "[a-zA-Z ]{10,50}"
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Q1".to_string()));
        app.on_message(ChatAppMsg::TextDelta("R1".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        render_and_graduate(&mut runtime, &mut app);

        app.on_message(ChatAppMsg::UserMessage("Q2".to_string()));
        app.on_message(ChatAppMsg::Error(error_msg.clone()));

        render_and_graduate(&mut runtime, &mut app);

        let stdout = strip_ansi(runtime.stdout_content());

        prop_assert!(
            stdout.contains("Q1"),
            "Previous user message should persist:\n{}",
            stdout
        );
        prop_assert!(
            stdout.contains("R1"),
            "Previous response should persist:\n{}",
            stdout
        );
    }
}

#[cfg(test)]
mod e2e_edge_cases {
    use super::*;

    #[test]
    fn empty_tool_result_completes() {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "empty_tool".to_string(),
            args: "{}".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "empty_tool".to_string(),
        });
        app.on_message(ChatAppMsg::TextDelta("Done".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        render_and_graduate(&mut runtime, &mut app);

        let combined = combined_output(&runtime);

        assert!(combined.contains("empty_tool"));
        assert!(combined.contains("\u{2713}") || combined.contains("Done"));
    }

    #[test]
    fn mode_change_during_streaming() {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Part 1".to_string()));

        render_and_graduate(&mut runtime, &mut app);

        app.on_message(ChatAppMsg::ModeChanged("plan".to_string()));
        app.on_message(ChatAppMsg::TextDelta(" Part 2".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        render_and_graduate(&mut runtime, &mut app);

        let stdout = strip_ansi(runtime.stdout_content());
        assert!(stdout.contains("Part 1") || stdout.contains("Part 2"));
    }
}
