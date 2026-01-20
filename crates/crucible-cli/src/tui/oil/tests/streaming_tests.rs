use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, InkChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::terminal::Terminal;
use crate::tui::oil::*;

fn view_with_default_ctx(app: &InkChatApp) -> Node {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    app.view(&ctx)
}

#[test]
fn streaming_content_grows_incrementally() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));

    let chunks = ["The ", "answer ", "is ", "42."];
    for chunk in chunks {
        app.on_message(ChatAppMsg::TextDelta(chunk.to_string()));

        let tree = view_with_default_ctx(&app);
        let rendered = render_to_string(&tree, 80);
        let stripped = strip_ansi(&rendered);

        assert!(
            stripped.contains(chunk.trim()),
            "Should contain chunk '{}', got: {}",
            chunk,
            stripped
        );
    }

    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    let rendered = render_to_string(&tree, 80);
    let stripped = strip_ansi(&rendered);

    assert!(
        stripped.contains("The answer is 42."),
        "Final render should contain full message: {}",
        stripped
    );
}

#[test]
fn streaming_viewport_does_not_duplicate_content() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    for i in 0..10 {
        app.on_message(ChatAppMsg::TextDelta(format!("word{} ", i)));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let viewport = strip_ansi(runtime.viewport_content());

        for j in 0..=i {
            let word = format!("word{}", j);
            let count = viewport.matches(&word).count();
            assert!(
                count <= 1,
                "word{} appears {} times in viewport (should be 0 or 1): {}",
                j,
                count,
                viewport
            );
        }
    }
}

#[test]
fn completed_message_graduates_streaming_stays() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("First question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("First answer".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    assert_eq!(
        runtime.graduated_count(),
        2,
        "User + Assistant should graduate"
    );
    assert!(
        strip_ansi(runtime.stdout_content()).contains("First question"),
        "User message should be in stdout"
    );
    assert!(
        strip_ansi(runtime.stdout_content()).contains("First answer"),
        "Assistant message should be in stdout"
    );

    app.on_message(ChatAppMsg::UserMessage("Second question".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Second ".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    assert_eq!(runtime.graduated_count(), 3, "Second user should graduate");

    let viewport = strip_ansi(runtime.viewport_content());
    assert!(
        viewport.contains("Second"),
        "Streaming content should be in viewport: {}",
        viewport
    );
    assert!(
        !viewport.contains("First"),
        "Completed content should not be in viewport: {}",
        viewport
    );
}

#[test]
fn multiline_streaming_renders_correctly() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Tell me about rust".to_string()));

    let multiline_chunks = [
        "Rust is a systems programming language.\n",
        "It focuses on:\n",
        "- Memory safety\n",
        "- Concurrency\n",
        "- Performance\n",
    ];

    for chunk in multiline_chunks {
        app.on_message(ChatAppMsg::TextDelta(chunk.to_string()));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        let viewport = strip_ansi(runtime.viewport_content());
        let line_count = viewport.lines().count();

        assert!(
            line_count < 30,
            "Viewport should not explode in size, got {} lines",
            line_count
        );
    }
}

#[test]
fn rapid_streaming_does_not_corrupt_output() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Generate text".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let initial_graduated = runtime.graduated_count();
    assert_eq!(initial_graduated, 1, "User message should graduate");

    for i in 0..50 {
        app.on_message(ChatAppMsg::TextDelta(format!("w{} ", i)));

        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);

        assert_eq!(
            runtime.graduated_count(),
            1,
            "Graduation count should stay at 1 during streaming, but is {} after chunk {}",
            runtime.graduated_count(),
            i
        );
    }

    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    assert_eq!(
        runtime.graduated_count(),
        2,
        "Should have 2 graduated messages after complete"
    );

    let stdout = strip_ansi(runtime.stdout_content());

    let unique_word = "w49";
    let count = stdout.matches(unique_word).count();
    assert_eq!(
        count, 1,
        "{} appears {} times in stdout (should be exactly 1): {}",
        unique_word, count, stdout
    );
}

#[test]
fn viewport_size_stays_bounded_during_streaming() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    let mut max_viewport_lines = 0;

    for i in 0..100 {
        if i % 10 == 0 {
            app.on_message(ChatAppMsg::TextDelta("\n".to_string()));
        }
        app.on_message(ChatAppMsg::TextDelta(format!("word{} ", i)));

        let tree = view_with_default_ctx(&app);
        let rendered = render_to_string(&tree, 80);
        let line_count = rendered.lines().count();
        max_viewport_lines = max_viewport_lines.max(line_count);
    }

    assert!(
        max_viewport_lines < 50,
        "Viewport grew to {} lines during streaming",
        max_viewport_lines
    );
}

#[test]
fn table_in_completed_message_renders_correctly() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Show me a table".to_string()));
    app.on_message(ChatAppMsg::TextDelta(
        "| A | B |\n|---|---|\n| 1 | 2 |\n".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());

    assert!(stdout.contains("A"), "Table header A missing: {}", stdout);
    assert!(stdout.contains("B"), "Table header B missing: {}", stdout);
    assert!(stdout.contains("1"), "Table cell 1 missing: {}", stdout);
    assert!(stdout.contains("2"), "Table cell 2 missing: {}", stdout);
}

#[test]
fn graduation_happens_only_on_stream_complete() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);
    assert_eq!(runtime.graduated_count(), 1, "User message should graduate");

    for _ in 0..10 {
        app.on_message(ChatAppMsg::TextDelta("chunk ".to_string()));
        let tree = view_with_default_ctx(&app);
        runtime.render(&tree);
    }

    assert_eq!(
        runtime.graduated_count(),
        1,
        "Streaming content should NOT graduate until complete"
    );

    app.on_message(ChatAppMsg::StreamComplete);
    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    assert_eq!(
        runtime.graduated_count(),
        2,
        "Assistant message should graduate after StreamComplete"
    );
}

#[test]
fn terminal_render_produces_stable_output() {
    let mut terminal = Terminal::new().expect("Failed to create terminal");
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));

    for _ in 0..5 {
        let tree = view_with_default_ctx(&app);
        terminal.render(&tree).expect("render failed");
    }

    for i in 0..10 {
        app.on_message(ChatAppMsg::TextDelta(format!("word{} ", i)));
        let tree = view_with_default_ctx(&app);
        terminal.render(&tree).expect("render failed");
    }

    app.on_message(ChatAppMsg::StreamComplete);

    for _ in 0..5 {
        let tree = view_with_default_ctx(&app);
        terminal.render(&tree).expect("render failed");
    }
}

#[test]
fn table_graduates_at_large_width_not_terminal_width() {
    let mut runtime = TestRuntime::new(40, 24);
    let mut app = InkChatApp::default();

    let wide_table = r#"| Column One | Column Two | Column Three |
|------------|------------|--------------|
| Data A     | Data B     | Data C       |"#;

    app.on_message(ChatAppMsg::UserMessage("Show table".to_string()));
    app.on_message(ChatAppMsg::TextDelta(wide_table.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());
    let lines: Vec<&str> = stdout.lines().collect();
    let header_line = lines.iter().find(|l| l.contains("Column One"));

    assert!(
        header_line.is_some(),
        "Should find header line in stdout: {}",
        stdout
    );

    let header = header_line.unwrap();
    assert!(
        header.contains("Column Two") && header.contains("Column Three"),
        "Completed table columns should be on same line (not wrapped to terminal width): {}",
        header
    );
}

#[test]
fn streaming_table_uses_terminal_width_for_viewport() {
    let mut runtime = TestRuntime::new(40, 24);
    let mut app = InkChatApp::default();

    let wide_table = r#"| Column One | Column Two | Column Three |
|------------|------------|--------------|
| Data A     | Data B     | Data C       |"#;

    app.on_message(ChatAppMsg::UserMessage("Show table".to_string()));
    app.on_message(ChatAppMsg::TextDelta(wide_table.to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let viewport = strip_ansi(runtime.viewport_content());
    assert!(
        viewport.contains("Column"),
        "Streaming table should be in viewport: {}",
        viewport
    );
}

#[test]
fn graduated_table_fits_terminal_width() {
    use crate::tui::oil::ansi::visible_width;

    let mut runtime = TestRuntime::new(60, 24);
    let mut app = InkChatApp::default();

    let table = r#"| Header A | Header B | Header C |
|----------|----------|----------|
| Cell 1   | Cell 2   | Cell 3   |"#;

    app.on_message(ChatAppMsg::UserMessage("Show table".to_string()));
    app.on_message(ChatAppMsg::TextDelta(table.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());

    for line in stdout.lines() {
        if line.contains('┌') || line.contains('│') || line.contains('└') {
            let width = visible_width(line);
            assert!(
                width <= 60,
                "Table line exceeds terminal width (60): {} chars\n{}",
                width,
                line
            );
        }
    }
}

#[test]
fn live_graduation_does_not_duplicate_content() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Send first paragraph with blank line (triggers graduation)
    app.on_message(ChatAppMsg::TextDelta("First paragraph.\n\n".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Send second paragraph with blank line (triggers second graduation)
    app.on_message(ChatAppMsg::TextDelta("Second paragraph.\n\n".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    // Send third paragraph (still in progress)
    app.on_message(ChatAppMsg::TextDelta(
        "Third paragraph in progress".to_string(),
    ));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());

    // Each paragraph should appear exactly once in stdout
    let first_count = stdout.matches("First paragraph").count();
    let second_count = stdout.matches("Second paragraph").count();

    assert_eq!(
        first_count, 1,
        "First paragraph appears {} times in stdout (should be 1):\n{}",
        first_count, stdout
    );
    assert_eq!(
        second_count, 1,
        "Second paragraph appears {} times in stdout (should be 1):\n{}",
        second_count, stdout
    );

    // Third paragraph should be in viewport, not stdout (still in progress)
    assert!(
        !stdout.contains("Third paragraph"),
        "In-progress content should not be in stdout yet:\n{}",
        stdout
    );

    let viewport = strip_ansi(runtime.viewport_content());
    assert!(
        viewport.contains("Third paragraph"),
        "In-progress content should be in viewport:\n{}",
        viewport
    );
}

#[test]
fn graduated_table_has_bottom_border() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    let table = r#"| Header |
|--------|
| Cell   |"#;

    app.on_message(ChatAppMsg::UserMessage("Show table".to_string()));
    app.on_message(ChatAppMsg::TextDelta(table.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());

    assert!(
        stdout.contains('┌'),
        "Table missing top-left corner in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains('┐'),
        "Table missing top-right corner in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains('└'),
        "Table missing bottom-left corner in stdout:\n{}",
        stdout
    );
    assert!(
        stdout.contains('┘'),
        "Table missing bottom-right corner in stdout:\n{}",
        stdout
    );
}

#[test]
fn live_graduated_table_preserves_all_lines() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Show table".to_string()));

    let table_content = "| Header |\n|--------|\n| Cell   |\n\n";
    app.on_message(ChatAppMsg::TextDelta(table_content.to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());

    let top_left_count = stdout.matches('┌').count();
    let bottom_left_count = stdout.matches('└').count();

    assert!(
        top_left_count >= 1,
        "Table missing top-left corner (┌) in stdout:\n{}",
        stdout
    );
    assert!(
        bottom_left_count >= 1,
        "Table missing bottom-left corner (└) in stdout - last line may be eaten:\n{}",
        stdout
    );
    assert_eq!(
        top_left_count, bottom_left_count,
        "Mismatched table corners - {} top-left vs {} bottom-left:\n{}",
        top_left_count, bottom_left_count, stdout
    );
}

#[test]
fn graduated_user_prompt_has_bottom_border() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Test message".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());

    let top_border_count = stdout.matches('▄').count();
    let bottom_border_count = stdout.matches('▀').count();

    assert!(
        top_border_count >= 1,
        "User prompt missing top border (▄) in stdout:\n{}",
        stdout
    );
    assert!(
        bottom_border_count >= 1,
        "User prompt missing bottom border (▀) in stdout:\n{}",
        stdout
    );
}

#[test]
fn graduated_user_prompt_bottom_border_not_eaten_by_viewport() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Test message".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());
    let viewport = strip_ansi(runtime.viewport_content());

    let stdout_lines: Vec<&str> = stdout.lines().collect();
    let viewport_lines: Vec<&str> = viewport.lines().collect();

    let last_stdout_line = stdout_lines.last().unwrap_or(&"");
    let first_viewport_line = viewport_lines.first().unwrap_or(&"");

    assert!(
        last_stdout_line.contains('▀') || last_stdout_line.is_empty(),
        "Last stdout line should be bottom border (▀) or empty, not: {:?}\nFull stdout:\n{}",
        last_stdout_line,
        stdout
    );

    assert!(
        !first_viewport_line.contains('▀'),
        "Viewport should not start with bottom border (it should be in stdout):\n{}",
        viewport
    );
}

#[test]
fn streaming_only_first_block_gets_bullet() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("First paragraph.\n\n".to_string()));
    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("Second paragraph.\n\n".to_string()));
    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("Third paragraph.".to_string()));
    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let stdout = strip_ansi(runtime.stdout_content());
    let bullet_count = stdout.matches('●').count();

    assert_eq!(
        bullet_count, 1,
        "Only one bullet should appear (for first block), found {}: {}",
        bullet_count, stdout
    );
}

#[test]
fn stream_cancel_graduates_existing_content() {
    let mut runtime = TestRuntime::new(80, 24);
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    app.on_message(ChatAppMsg::TextDelta("First part of answer. ".to_string()));
    app.on_message(ChatAppMsg::TextDelta("More content here.".to_string()));

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let pre_cancel_viewport = strip_ansi(runtime.viewport_content());
    assert!(
        pre_cancel_viewport.contains("First part"),
        "Content should be visible before cancel: {}",
        pre_cancel_viewport
    );

    app.on_message(ChatAppMsg::StreamCancelled);

    let tree = view_with_default_ctx(&app);
    runtime.render(&tree);

    let post_cancel = strip_ansi(runtime.stdout_content());
    assert!(
        post_cancel.contains("First part"),
        "Cancelled content should be graduated to stdout: {}",
        post_cancel
    );
}
