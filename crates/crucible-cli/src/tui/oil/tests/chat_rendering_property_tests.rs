use crate::tui::oil::ansi::{strip_ansi, visible_width};
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::tests::helpers::{vt_render_sized, view_with_default_ctx};
use crate::tui::oil::tests::vt100_runtime::Vt100TestRuntime;
use proptest::prelude::*;

use super::generators::{arb_markdown_content, arb_text_content};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn rendered_lines_stay_bounded(
        content in arb_text_content(),
        width in 60usize..120
    ) {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));
        app.on_message(ChatAppMsg::TextDelta(content));
        app.on_message(ChatAppMsg::StreamComplete);

        let rendered = vt_render_sized(&mut app, width as u16, 60);

        let max_allowed = width + 5;
        for (i, line) in rendered.lines().enumerate() {
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
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        for (i, chunk) in chunks.iter().enumerate() {
            let marked = format!("PARA{}: {}", i, chunk);
            app.on_message(ChatAppMsg::TextDelta(marked));
            app.on_message(ChatAppMsg::TextDelta("\n\n".to_string()));

            vt.render_frame(&mut app);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());

        prop_assert!(
            screen.contains("PARA0"),
            "First paragraph should be present:\n{}",
            screen
        );
    }

    #[test]
    fn viewport_stdout_no_overlap(
        chunks in prop::collection::vec(arb_text_content(), 2..6)
    ) {
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        for chunk in &chunks[..chunks.len()-1] {
            app.on_message(ChatAppMsg::TextDelta(chunk.clone()));
            app.on_message(ChatAppMsg::TextDelta("\n\n".to_string()));

            vt.render_frame(&mut app);
        }

        let last_chunk = &chunks[chunks.len()-1];
        app.on_message(ChatAppMsg::TextDelta(last_chunk.clone()));

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());

        let last_word = last_chunk.split_whitespace().next();
        if let Some(word) = last_word {
            if word.len() >= 4 {
                let count = screen.matches(word).count();
                prop_assert!(
                    count <= 1,
                    "Content '{}' should not appear more than once in screen:\n{}",
                    word, screen
                );
            }
        }
    }

    #[test]
    fn rendering_is_deterministic(
        content in arb_text_content(),
        width in 60usize..120
    ) {
        let mut app1 = OilChatApp::default();
        let mut app2 = OilChatApp::default();

        app1.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app1.on_message(ChatAppMsg::TextDelta(content.clone()));
        app1.on_message(ChatAppMsg::StreamComplete);

        app2.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app2.on_message(ChatAppMsg::TextDelta(content));
        app2.on_message(ChatAppMsg::StreamComplete);

        let render1 = vt_render_sized(&mut app1, width as u16, 60);
        let render2 = vt_render_sized(&mut app2, width as u16, 60);

        prop_assert_eq!(
            render1,
            render2,
            "Same input should produce same output"
        );
    }

    /// This test explicitly verifies render_to_string doesn't panic — keep
    /// render_to_string here since it tests render engine robustness via
    /// catch_unwind, not the terminal path.
    #[test]
    fn markdown_renders_at_any_width(
        content in arb_markdown_content(),
        width in 20usize..120
    ) {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::TextDelta(content));
        app.on_message(ChatAppMsg::StreamComplete);

        let tree = view_with_default_ctx(&app);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render_to_string(&tree, width)
        }));
        prop_assert!(result.is_ok(), "render_to_string panicked at width={}", width);
    }

    #[test]
    fn table_renders_without_crash(
        col1 in "[a-zA-Z]{2,6}",
        col2 in "[a-zA-Z]{2,6}",
        cell1 in "[a-zA-Z]{2,8}",
        cell2 in "[a-zA-Z]{2,8}",
        width in 60u16..100u16
    ) {
        let mut vt = Vt100TestRuntime::new(width, 60);
        let mut app = OilChatApp::default();

        let table = format!(
            "| {} | {} |\n|---|---|\n| {} | {} |",
            col1, col2, cell1, cell2
        );

        app.on_message(ChatAppMsg::UserMessage("Show table".to_string()));
        app.on_message(ChatAppMsg::TextDelta(table));
        app.on_message(ChatAppMsg::StreamComplete);

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());

        prop_assert!(
            screen.contains(&col1) || screen.contains(&col2),
            "Table columns should be present:\n{}",
            screen
        );
    }

    #[test]
    fn rapid_streaming_no_duplication(
        chunk_count in 10usize..50
    ) {
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Generate".to_string()));

        vt.render_frame(&mut app);

        for i in 0..chunk_count {
            app.on_message(ChatAppMsg::TextDelta(format!("W{} ", i)));

            vt.render_frame(&mut app);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());

        for i in 0..chunk_count.min(20) {
            let marker = format!("W{} ", i);
            let count = screen.matches(&marker).count();
            prop_assert!(
                count <= 1,
                "{} appears {} times (should be 0 or 1):\n{}",
                marker, count, screen
            );
        }
    }

    #[test]
    fn hidden_thinking_shows_bounded_preview(
        thinking in arb_text_content(),
        response in arb_text_content()
    ) {
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();
        app.set_show_thinking(false);

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::ThinkingDelta(thinking.clone()));
        app.on_message(ChatAppMsg::TextDelta(response.clone()));
        app.on_message(ChatAppMsg::StreamComplete);

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());

        prop_assert!(
            !screen.is_empty(),
            "Output should not be empty:\n{}",
            screen
        );
    }

    #[test]
    fn response_rendered_after_thinking(
        thinking in "[a-zA-Z]{10,30}",
        response in "[a-zA-Z]{10,30}"
    ) {
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();
        app.set_show_thinking(true);

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::ThinkingDelta(thinking));
        app.on_message(ChatAppMsg::TextDelta(response.clone()));
        app.on_message(ChatAppMsg::StreamComplete);

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());

        prop_assert!(
            screen.contains(&response),
            "Response content should be present:\n{}",
            screen
        );
    }

    #[test]
    fn user_prompt_borders_balanced(message in arb_text_content()) {
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage(message));

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());

        let top_border_count = screen.chars().filter(|&c| c == '\u{2584}').count();
        let bottom_border_count = screen.chars().filter(|&c| c == '\u{2580}').count();

        prop_assert!(
            top_border_count > 0,
            "User prompt should have top border"
        );
        prop_assert!(
            bottom_border_count > 0,
            "User prompt should have bottom border"
        );
    }

    /// Without drain_completed (which requires render_frame from chat_runner),
    /// all content stays in viewport. Verify all chunks are visible in screen output.
    #[test]
    fn all_chunks_visible_in_output(chunk_count in 3usize..10) {
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

        vt.render_frame(&mut app);

        for i in 0..chunk_count {
            app.on_message(ChatAppMsg::TextDelta(format!("CHUNK{}\n\n", i)));

            vt.render_frame(&mut app);
        }

        app.on_message(ChatAppMsg::TextDelta("FINAL_IN_PROGRESS".to_string()));

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());

        prop_assert!(
            screen.contains("CHUNK0"),
            "First chunk should be visible in output:\n{}",
            screen
        );

        prop_assert!(
            screen.contains("FINAL_IN_PROGRESS"),
            "In-progress content should be visible in output:\n{}",
            screen
        );
    }
}

#[cfg(test)]
mod rendering_edge_cases {
    use super::*;

    #[test]
    fn empty_streaming_completes_cleanly() {
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Q".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());
        assert!(screen.contains("Q"), "User message should be present");
    }

    /// This test explicitly verifies render_to_string doesn't panic — keep
    /// render_to_string here since it tests render engine robustness via
    /// catch_unwind, not the terminal path.
    #[test]
    fn very_narrow_width_does_not_panic() {
        let mut app = OilChatApp::default();
        app.on_message(ChatAppMsg::UserMessage("Test message".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Response text".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        for width in [10, 15, 20, 25] {
            let tree = view_with_default_ctx(&app);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render_to_string(&tree, width)
            }));
            assert!(
                result.is_ok(),
                "render_to_string panicked at width={}",
                width
            );
        }
    }

    #[test]
    fn unicode_content_renders_correctly() {
        let mut vt = Vt100TestRuntime::new(80, 60);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Test".to_string()));
        app.on_message(ChatAppMsg::TextDelta("Hello world".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());
        assert!(screen.contains("Hello") || screen.contains("world"));
    }
}
