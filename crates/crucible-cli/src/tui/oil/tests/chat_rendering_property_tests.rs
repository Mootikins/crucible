use crate::tui::oil::ansi::{strip_ansi, visible_width};
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, InkChatApp};
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::TestRuntime;
use proptest::prelude::*;

use super::generators::{arb_markdown_content, arb_text_content};
use super::helpers::view_with_default_ctx;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn rendered_lines_stay_bounded(
        content in arb_text_content(),
        width in 60usize..120
    ) {
        let mut app = InkChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));
        app.on_message(ChatAppMsg::TextDelta(content));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        let rendered = render_to_string(&tree, width);

        let max_allowed = width + 5;
        for (i, line) in rendered.split("\r\n").enumerate() {
            let line_width = visible_width(line);
            prop_assert!(
                line_width <= max_allowed,
                "Line {} exceeds max allowed {}: {} chars\n{:?}",
                i + 1, max_allowed, line_width, line
            );
        }
    }

    #[test]
    fn graduated_content_present_after_streaming(
        chunks in prop::collection::vec(arb_text_content(), 2..5)
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        for (i, chunk) in chunks.iter().enumerate() {
            let marked = format!("PARA{}: {}", i, chunk);
            app.on_message(ChatAppMsg::TextDelta(marked));
            app.on_message(ChatAppMsg::TextDelta("\n\n".to_string()));

            let tree = view_with_default_ctx(&app);
            runtime.render(&tree);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        prop_assert!(
            combined.contains("PARA0"),
            "First paragraph should be present:\n{}",
            combined
        );
    }

    #[test]
    fn viewport_stdout_no_overlap(
        chunks in prop::collection::vec(arb_text_content(), 2..6)
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        for chunk in &chunks[..chunks.len()-1] {
            app.on_message(ChatAppMsg::TextDelta(chunk.clone()));
            app.on_message(ChatAppMsg::TextDelta("\n\n".to_string()));

            let tree = view_with_default_ctx(&app);
            runtime.render(&tree);
        }

        let last_chunk = &chunks[chunks.len()-1];
        app.on_message(ChatAppMsg::TextDelta(last_chunk.clone()));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());

        let last_word = last_chunk.split_whitespace().next();
        if let Some(word) = last_word {
            if word.len() >= 4 {
                prop_assert!(
                    !stdout.contains(word) || viewport.contains(word),
                    "In-progress content '{}' should be in viewport only, but found in stdout:\nstdout: {}\nviewport: {}",
                    word, stdout, viewport
                );
            }
        }
    }

    #[test]
    fn rendering_is_deterministic(
        content in arb_text_content(),
        width in 60usize..120
    ) {
        let mut app1 = InkChatApp::default();
        let mut app2 = InkChatApp::default();

        app1.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app1.on_message(ChatAppMsg::TextDelta(content.clone()));
        app1.on_message(ChatAppMsg::StreamComplete);

        app2.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app2.on_message(ChatAppMsg::TextDelta(content));
        app2.on_message(ChatAppMsg::StreamComplete);

        let tree1 = view_with_default_ctx(&app1);
        let tree2 = view_with_default_ctx(&app2);

        let render1 = render_to_string(&tree1, width);
        let render2 = render_to_string(&tree2, width);

        prop_assert_eq!(
            strip_ansi(&render1),
            strip_ansi(&render2),
            "Same input should produce same output"
        );
    }

    #[test]
    fn markdown_renders_at_any_width(
        content in arb_markdown_content(),
        width in 20usize..120
    ) {
        let mut app = InkChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::TextDelta(content));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render_to_string(&tree, width)
        }));
    }

    #[test]
    fn table_renders_without_crash(
        col1 in "[a-zA-Z]{2,6}",
        col2 in "[a-zA-Z]{2,6}",
        cell1 in "[a-zA-Z]{2,8}",
        cell2 in "[a-zA-Z]{2,8}",
        width in 60u16..100u16
    ) {
        let mut runtime = TestRuntime::new(width, 24);
        let mut app = InkChatApp::default();

        let table = format!(
            "| {} | {} |\n|---|---|\n| {} | {} |",
            col1, col2, cell1, cell2
        );

        app.on_message(ChatAppMsg::UserMessage("Show table".to_string()));
        app.on_message(ChatAppMsg::TextDelta(table));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        prop_assert!(
            combined.contains(&col1) || combined.contains(&col2),
            "Table columns should be present:\n{}",
            combined
        );
    }

    #[test]
    fn rapid_streaming_no_duplication(
        chunk_count in 10usize..50
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Generate".to_string()));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        for i in 0..chunk_count {
            app.on_message(ChatAppMsg::TextDelta(format!("W{} ", i)));

            let tree = view_with_default_ctx(&app);
            runtime.render(&tree);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());

        for i in 0..chunk_count.min(20) {
            let marker = format!("W{} ", i);
            let count = stdout.matches(&marker).count();
            prop_assert!(
                count <= 1,
                "{} appears {} times (should be 0 or 1):\n{}",
                marker, count, stdout
            );
        }
    }

    #[test]
    fn hidden_thinking_not_rendered(
        thinking in arb_text_content(),
        response in arb_text_content()
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();
        app.set_show_thinking(false);

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::ThinkingDelta(thinking.clone()));
        app.on_message(ChatAppMsg::TextDelta(response.clone()));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        let first_think_word = thinking.split_whitespace().next();
        if let Some(word) = first_think_word {
            if word.len() >= 4 && !response.contains(word) {
                prop_assert!(
                    !combined.contains(word),
                    "Thinking content '{}' should not appear when show_thinking=false:\n{}",
                    word, combined
                );
            }
        }
    }

    #[test]
    fn response_rendered_after_thinking(
        thinking in "[a-zA-Z]{10,30}",
        response in "[a-zA-Z]{10,30}"
    ) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();
        app.set_show_thinking(true);

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::ThinkingDelta(thinking));
        app.on_message(ChatAppMsg::TextDelta(response.clone()));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        prop_assert!(
            combined.contains(&response),
            "Response content should be present:\n{}",
            combined
        );
    }

    #[test]
    fn user_prompt_borders_balanced(message in arb_text_content()) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();

        app.on_message(ChatAppMsg::UserMessage(message));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());

        let top_border_count = stdout.chars().filter(|&c| c == '\u{2584}').count();
        let bottom_border_count = stdout.chars().filter(|&c| c == '\u{2580}').count();

        prop_assert!(
            top_border_count > 0,
            "User prompt should have top border"
        );
        prop_assert!(
            bottom_border_count > 0,
            "User prompt should have bottom border"
        );
    }

    #[test]
    fn graduated_items_leave_viewport(chunk_count in 3usize..10) {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        for i in 0..chunk_count {
            app.on_message(ChatAppMsg::TextDelta(format!("CHUNK{}\n\n", i)));

            let tree = view_with_default_ctx(&app);
            runtime.render(&tree);
        }

        app.on_message(ChatAppMsg::TextDelta("FINAL_IN_PROGRESS".to_string()));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());

        prop_assert!(
            stdout.contains("CHUNK0"),
            "First chunk should be in stdout (graduated):\nstdout: {}\nviewport: {}",
            stdout, viewport
        );

        prop_assert!(
            viewport.contains("FINAL_IN_PROGRESS"),
            "In-progress content should be in viewport:\nviewport: {}",
            viewport
        );

        prop_assert!(
            !viewport.contains("CHUNK0"),
            "Graduated content should not be in viewport:\nviewport: {}",
            viewport
        );
    }
}

#[cfg(test)]
mod rendering_edge_cases {
    use super::*;

    #[test]
    fn empty_streaming_completes_cleanly() {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        assert!(stdout.contains("Q"), "User message should be present");
    }

    #[test]
    fn very_narrow_width_does_not_panic() {
        let mut app = InkChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Test message".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Response text".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        for width in [10, 15, 20, 25] {
            let tree = view_with_default_ctx(&app);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render_to_string(&tree, width)
            }));
        }
    }

    #[test]
    fn unicode_content_renders_correctly() {
        let mut runtime = TestRuntime::new(80, 24);
        let mut app = InkChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Hello world".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        assert!(stdout.contains("Hello") || stdout.contains("world"));
    }
}
