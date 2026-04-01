use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};

use super::helpers::vt_render;
use super::vt100_runtime::Vt100TestRuntime;

#[test]
fn streaming_content_grows_incrementally() {
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));

    let chunks = ["The ", "answer ", "is ", "42."];
    for chunk in chunks {
        app.on_message(ChatAppMsg::TextDelta(chunk.to_string()));

        let stripped = vt_render(&mut app);

        assert!(
            stripped.contains(chunk.trim()),
            "Should contain chunk '{}', got: {}",
            chunk,
            stripped
        );
    }

    app.on_message(ChatAppMsg::StreamComplete);

    let stripped = vt_render(&mut app);

    assert!(
        stripped.contains("The answer is 42."),
        "Final render should contain full message: {}",
        stripped
    );
}

#[test]
fn multiline_streaming_renders_correctly() {
    let mut vt = Vt100TestRuntime::new(80, 60);
    let mut app = OilChatApp::default();

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

        vt.render_frame(&mut app);

        let screen = strip_ansi(&vt.screen_contents());
        let line_count = screen.lines().count();

        assert!(
            line_count < 70,
            "Screen should not explode in size, got {} lines",
            line_count
        );
    }
}

#[test]
fn graduated_table_fits_terminal_width() {
    use crate::tui::oil::ansi::visible_width;

    let mut vt = Vt100TestRuntime::new(60, 60);
    let mut app = OilChatApp::default();

    let table = r#"| Header A | Header B | Header C |
|----------|----------|----------|
| Cell 1   | Cell 2   | Cell 3   |"#;

    app.on_message(ChatAppMsg::UserMessage("Show table".to_string()));
    app.on_message(ChatAppMsg::TextDelta(table.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.full_history());

    for line in screen.lines() {
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
fn code_block_with_blank_line_not_split_into_separate_blocks() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Show code".to_string()));

    app.on_message(ChatAppMsg::TextDelta("```bash\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("echo hello\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("echo world\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("```".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let stripped = vt_render(&mut app);

    let backtick_count = stripped.matches("```").count();
    assert!(
        backtick_count <= 2,
        "Code block with blank line should have at most 2 fence markers (open+close), got {}.\nOutput:\n{}",
        backtick_count,
        stripped
    );

    assert!(
        stripped.contains("echo hello"),
        "Should contain first line of code block.\nOutput:\n{}",
        stripped
    );
    assert!(
        stripped.contains("echo world"),
        "Should contain second line of code block.\nOutput:\n{}",
        stripped
    );
}

#[test]
fn streaming_code_block_fence_not_tripled() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Show code".to_string()));

    app.on_message(ChatAppMsg::TextDelta(
        "Here's the code:\n\n```bash\ngit clone repo\n```\n\nDone.".to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    let stripped = vt_render(&mut app);

    let backtick_count = stripped.matches("```").count();
    assert_eq!(
        backtick_count, 2,
        "Should have exactly 2 fence markers, got {}.\nOutput:\n{}",
        backtick_count, stripped
    );
}

#[test]
fn streaming_incremental_code_block_no_duplicate_fences() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Show code".to_string()));

    app.on_message(ChatAppMsg::TextDelta("Here's the output:\n\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("```\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("total 100\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("drwxr-xr-x file1\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("```\n\n".to_string()));
    app.on_message(ChatAppMsg::TextDelta("That's all.".to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let stripped = vt_render(&mut app);

    let backtick_count = stripped.matches("```").count();
    assert_eq!(
        backtick_count, 2,
        "Should have exactly 2 fence markers, got {}.\nOutput:\n{}",
        backtick_count, stripped
    );
}

#[test]
fn content_not_duplicated_during_streaming() {
    let mut vt = Vt100TestRuntime::new(80, 60);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("First paragraph.\n\n".to_string()));

    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Second paragraph.\n\n".to_string()));

    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta(
        "Third paragraph in progress".to_string(),
    ));

    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.full_history());

    let first_count = screen.matches("First paragraph").count();
    let second_count = screen.matches("Second paragraph").count();
    let third_count = screen.matches("Third paragraph").count();

    assert_eq!(
        first_count, 1,
        "First paragraph appears {} times (should be 1):\n{}",
        first_count, screen
    );
    assert_eq!(
        second_count, 1,
        "Second paragraph appears {} times (should be 1):\n{}",
        second_count, screen
    );
    assert_eq!(
        third_count, 1,
        "Third paragraph appears {} times (should be 1):\n{}",
        third_count, screen
    );
}

#[test]
fn streaming_only_first_block_gets_bullet() {
    let mut vt = Vt100TestRuntime::new(80, 60);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("First paragraph.\n\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Second paragraph.\n\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Third paragraph.".to_string()));
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.full_history());
    let bullet_count = screen.matches('●').count();

    assert_eq!(
        bullet_count, 1,
        "Only one bullet should appear (for first block), found {}: {}",
        bullet_count, screen
    );
}

#[test]
fn stream_cancel_preserves_existing_content() {
    let mut vt = Vt100TestRuntime::new(80, 60);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("Question".to_string()));

    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("First part of answer. ".to_string()));
    app.on_message(ChatAppMsg::TextDelta("More content here.".to_string()));

    vt.render_frame(&mut app);

    let pre_cancel_screen = strip_ansi(&vt.screen_contents());
    assert!(
        pre_cancel_screen.contains("First part"),
        "Content should be visible before cancel: {}",
        pre_cancel_screen
    );

    app.on_message(ChatAppMsg::StreamCancelled);

    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.full_history());
    assert!(
        screen.contains("First part"),
        "Cancelled content should be visible in output: {}",
        screen
    );
}

#[test]
fn overflow_graduation_does_not_duplicate_content() {
    let mut vt = Vt100TestRuntime::new(80, 60);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("run ls".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: r#"{"command":"ls -la"}"#.to_string(),
        call_id: None,
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "total 100\n".to_string(),
        call_id: None,
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".to_string(),
        call_id: None,
    });
    vt.render_frame(&mut app);

    let mut long_response = String::new();
    for i in 1..=25 {
        long_response.push_str(&format!("Line {} of the response\n", i));
        app.on_message(ChatAppMsg::TextDelta(format!(
            "Line {} of the response\n",
            i
        )));
        vt.render_frame(&mut app);
    }

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.full_history());

    for i in 1..=25 {
        let marker = format!("Line {} of the response", i);
        let count = screen.matches(&marker).count();
        assert!(
            count <= 1,
            "Line {} appears {} times in output (should be 0 or 1):\n{}",
            i,
            count,
            screen
        );
    }

    let bullet_count = screen.matches('●').count();
    assert!(
        bullet_count <= 2,
        "Too many bullets in output: {} (expected at most 2 - one for user, one for assistant):\n{}",
        bullet_count, screen
    );
}

#[test]
fn incremental_text_after_tool_no_duplication() {
    let mut vt = Vt100TestRuntime::new(80, 60);
    let mut app = OilChatApp::default();

    app.on_message(ChatAppMsg::UserMessage("test".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".to_string(),
        args: "{}".to_string(),
        call_id: None,
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "output line 1\n".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".to_string(),
        delta: "output line 2\n".to_string(),
        call_id: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".to_string(),
        call_id: None,
    });
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("```\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("total 100\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("drwxr-xr-x file1\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("```\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("```\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("total 100\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("drwxr-xr-x file1\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("drwxr-xr-x file2\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("```\n".to_string()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let screen = strip_ansi(&vt.full_history());

    let total_count = screen.matches("total 100").count();
    assert!(
        total_count <= 4,
        "'total 100' appears {} times (expected at most 4 - two code blocks):\n{}",
        total_count,
        screen
    );
}
