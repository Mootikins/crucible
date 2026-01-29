use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::TestRuntime;
use proptest::prelude::*;

use super::generators::{
    arb_text_content, arb_valid_stream_sequence, StreamEvent, TextStreamEvent,
};
use super::helpers::view_with_default_ctx;

fn arb_text_stream_event() -> impl Strategy<Value = TextStreamEvent> {
    prop_oneof![
        arb_text_content().prop_map(TextStreamEvent::TextDelta),
        arb_text_content().prop_map(TextStreamEvent::ThinkingDelta),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn user_messages_preserve_order(
        messages in prop::collection::vec(arb_text_content(), 2..10)
    ) {
        let mut app = OilChatApp::default();

        for (i, msg) in messages.iter().enumerate() {
            app.on_message(ChatAppMsg::UserMessage(format!("USER_{}: {}", i, msg)));
            app.on_message(ChatAppMsg::TextDelta(format!("Response to {}", i)));
            app.on_message(ChatAppMsg::StreamComplete);
        }

        let mut runtime = TestRuntime::new(80, 24);
        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);
        let stdout = strip_ansi(runtime.stdout_content());

        for i in 0..(messages.len() - 1) {
            let pos_i = stdout.find(&format!("USER_{}", i));
            let pos_next = stdout.find(&format!("USER_{}", i + 1));

            prop_assert!(
                pos_i.is_some() && pos_next.is_some(),
                "Both USER_{} and USER_{} should be in output:\n{}",
                i, i + 1, stdout
            );

            prop_assert!(
                pos_i.unwrap() < pos_next.unwrap(),
                "USER_{} (pos {}) should appear before USER_{} (pos {})\n{}",
                i, pos_i.unwrap(), i + 1, pos_next.unwrap(), stdout
            );
        }
    }

    #[test]
    fn streaming_segments_preserve_insertion_order(
        events in prop::collection::vec(arb_text_stream_event(), 2..15)
    ) {
        let mut app = OilChatApp::default();
        app.set_show_thinking(true);

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        for event in &events {
            match event {
                TextStreamEvent::TextDelta(text) => {
                    app.on_message(ChatAppMsg::TextDelta(text.clone()));
                }
                TextStreamEvent::ThinkingDelta(text) => {
                    app.on_message(ChatAppMsg::ThinkingDelta(text.clone()));
                }
            }
        }

        app.on_message(ChatAppMsg::StreamComplete);

        let mut runtime = TestRuntime::new(80, 24);
        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        prop_assert!(
            stdout.contains("Question"),
            "User message should be present: {}",
            stdout
        );
    }

    #[test]
    fn tool_calls_maintain_chronological_order(
        sequence in arb_valid_stream_sequence()
    ) {
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        for event in &sequence {
            match event {
                StreamEvent::TextDelta(text) => {
                    app.on_message(ChatAppMsg::TextDelta(text.clone()));
                }
                StreamEvent::ThinkingDelta(text) => {
                    app.on_message(ChatAppMsg::ThinkingDelta(text.clone()));
                }
                StreamEvent::ToolCall { name, args } => {
                    app.on_message(ChatAppMsg::ToolCall {
                        name: name.clone(),
                        args: args.clone(),
                call_id: None,
                    });
                }
                StreamEvent::ToolResultDelta { name, delta } => {
                    app.on_message(ChatAppMsg::ToolResultDelta {
                        name: name.clone(),
                        delta: delta.clone(),
                call_id: None,
                    });
                }
                StreamEvent::ToolResultComplete { name } => {
                    app.on_message(ChatAppMsg::ToolResultComplete { name: name.clone() , call_id: None });
                }
            }
        }

        app.on_message(ChatAppMsg::StreamComplete);

        let mut runtime = TestRuntime::new(80, 24);
        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        let tool_names: Vec<&str> = sequence.iter().filter_map(|e| {
            if let StreamEvent::ToolCall { name, .. } = e {
                Some(name.as_str())
            } else {
                None
            }
        }).collect();

        for name in &tool_names {
            prop_assert!(
                combined.contains(name),
                "Tool '{}' should appear in output:\n{}",
                name, combined
            );
        }
    }

    #[test]
    fn multiple_turns_maintain_order(turn_count in 2usize..8) {
        let mut app = OilChatApp::default();

        for i in 0..turn_count {
            app.on_message(ChatAppMsg::UserMessage(format!("TURN_{}_USER", i)));
            app.on_message(ChatAppMsg::TextDelta(format!("TURN_{}_RESPONSE", i)));
            app.on_message(ChatAppMsg::StreamComplete);
        }

        let mut runtime = TestRuntime::new(80, 24);
        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());

        let mut last_response_pos = 0;

        for i in 0..turn_count {
            let user_marker = format!("TURN_{}_USER", i);
            let response_marker = format!("TURN_{}_RESPONSE", i);

            let user_pos = stdout.find(&user_marker);
            let response_pos = stdout.find(&response_marker);

            prop_assert!(
                user_pos.is_some(),
                "Turn {} user message should be present:\n{}",
                i, stdout
            );
            prop_assert!(
                response_pos.is_some(),
                "Turn {} response should be present:\n{}",
                i, stdout
            );

            let user_pos = user_pos.unwrap();
            let response_pos = response_pos.unwrap();

            prop_assert!(
                user_pos < response_pos,
                "Turn {}: user (pos {}) should come before response (pos {})",
                i, user_pos, response_pos
            );

            if i > 0 {
                prop_assert!(
                    user_pos > last_response_pos,
                    "Turn {} user (pos {}) should come after turn {} response (pos {})",
                    i, user_pos, i - 1, last_response_pos
                );
            }

            last_response_pos = response_pos;
        }
    }

    #[test]
    fn cancelled_stream_preserves_content(
        chunks in prop::collection::vec(arb_text_content(), 1..10),
        cancel_at in 0usize..10
    ) {
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        let cancel_at = cancel_at.min(chunks.len());

        for (i, chunk) in chunks.iter().enumerate() {
            if i >= cancel_at {
                break;
            }
            app.on_message(ChatAppMsg::TextDelta(chunk.clone()));
        }

        app.on_message(ChatAppMsg::StreamCancelled);

        let mut runtime = TestRuntime::new(80, 24);
        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let all_content: String = chunks.iter().take(cancel_at).cloned().collect();
        let first_word = all_content.split_whitespace().next();
        if let Some(word) = first_word {
            if word.len() >= 3 {
                let prefix = &word[..3.min(word.len())];
                prop_assert!(
                    stdout.contains(prefix),
                    "Cancelled content should have prefix '{}' in stdout:\n{}",
                    prefix, stdout
                );
            }
        }
    }

}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn many_messages_render_stable(message_count in 10usize..50) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        for i in 0..message_count {
            app.on_message(ChatAppMsg::UserMessage(format!("Message {}", i)));
            app.on_message(ChatAppMsg::TextDelta(format!("Response {}", i)));
            app.on_message(ChatAppMsg::StreamComplete);

            let tree = view_with_default_ctx(&app);
            runtime.render(&tree);

            let graduated = runtime.last_graduated_keys();
            if !graduated.is_empty() {
                app.mark_graduated(graduated.iter().cloned());
            }
        }

        let stdout = strip_ansi(runtime.stdout_content());
        prop_assert!(
            !stdout.is_empty(),
            "Should have some output after {} messages",
            message_count
        );

        prop_assert!(
            runtime.graduated_count() > 0,
            "Should have graduated some messages"
        );
    }
}

#[cfg(test)]
mod segment_ordering_tests {
    use super::*;

    #[test]
    fn alternating_content_types_render_in_order() {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();
        app.set_show_thinking(true);

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::ThinkingDelta(
            "THINK_MARKER_CONTENT".to_string(),
        ));
        app.on_message(ChatAppMsg::TextDelta("TEXT_MARKER_CONTENT".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        assert!(
            combined.contains("TEXT_MARKER_CONTENT"),
            "Should show text content:\n{}",
            combined
        );
    }
}
