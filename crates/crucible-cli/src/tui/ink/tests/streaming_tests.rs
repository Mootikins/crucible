use crate::tui::ink::ansi::strip_ansi;
use crate::tui::ink::app::App;
use crate::tui::ink::chat_app::{ChatAppMsg, InkChatApp};
use crate::tui::ink::render::render_to_string;
use crate::tui::ink::terminal::Terminal;
use crate::tui::ink::*;

#[test]
fn streaming_content_grows_incrementally() {
    let mut app = InkChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));

    let chunks = ["The ", "answer ", "is ", "42."];
    for chunk in chunks {
        app.on_message(ChatAppMsg::TextDelta(chunk.to_string()));

        let tree = app.view();
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

    let tree = app.view();
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

        let tree = app.view();
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

    let tree = app.view();
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

    let tree = app.view();
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

        let tree = app.view();
        runtime.render(&tree);

        let viewport = strip_ansi(runtime.viewport_content());
        let line_count = viewport.lines().count();

        assert!(
            line_count < 20,
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

    let tree = app.view();
    runtime.render(&tree);

    let initial_graduated = runtime.graduated_count();
    assert_eq!(initial_graduated, 1, "User message should graduate");

    for i in 0..50 {
        app.on_message(ChatAppMsg::TextDelta(format!("w{} ", i)));

        let tree = app.view();
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

    let tree = app.view();
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

        let tree = app.view();
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

    let tree = app.view();
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

    let tree = app.view();
    runtime.render(&tree);
    assert_eq!(runtime.graduated_count(), 1, "User message should graduate");

    for _ in 0..10 {
        app.on_message(ChatAppMsg::TextDelta("chunk ".to_string()));
        let tree = app.view();
        runtime.render(&tree);
    }

    assert_eq!(
        runtime.graduated_count(),
        1,
        "Streaming content should NOT graduate until complete"
    );

    app.on_message(ChatAppMsg::StreamComplete);
    let tree = app.view();
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
        let tree = app.view();
        terminal.render(&tree).expect("render failed");
    }

    for i in 0..10 {
        app.on_message(ChatAppMsg::TextDelta(format!("word{} ", i)));
        let tree = app.view();
        terminal.render(&tree).expect("render failed");
    }

    app.on_message(ChatAppMsg::StreamComplete);

    for _ in 0..5 {
        let tree = app.view();
        terminal.render(&tree).expect("render failed");
    }
}
