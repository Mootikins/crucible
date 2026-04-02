//! Graduation invariant tests.
//!
//! These tests verify the critical invariants of the graduation system:
//! - No spinners in scrollback (the original bug that motivated the container model)
//! - Thinking blocks are collapsed after graduation
//! - All content is preserved through graduation
//! - Turn indicator (spinner) only appears in viewport chrome, never in scrollback

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use super::vt100_runtime::Vt100TestRuntime;

// ─── No spinners in scrollback ─────────────────────────────────────────────

#[test]
fn no_spinners_in_scrollback() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Test".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ThinkingDelta("reasoning about the problem".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Answer".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    vt.assert_no_spinners_in_scrollback();
}

#[test]
fn no_spinners_in_scrollback_after_tool_use() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Use a tool".into()));
    vt.render_frame(&mut app);

    // Tool call with pending state (renders spinner in viewport)
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "echo hello"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    vt.render_frame(&mut app);

    // Complete the tool and stream
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::TextDelta("Done.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    vt.assert_no_spinners_in_scrollback();
}

#[test]
fn no_spinners_after_multi_tool_graduation() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Multi-tool".into()));
    vt.render_frame(&mut app);

    // Multiple tools with renders between
    for i in 0..3 {
        let id = format!("c{}", i);
        app.on_message(ChatAppMsg::ToolCall {
            name: "read_file".into(),
            args: format!(r#"{{"path": "file{}.rs"}}"#, i),
            call_id: Some(id.clone()),
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        vt.render_frame(&mut app); // spinner visible during pending

        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "read_file".into(),
            call_id: Some(id),
        });
        vt.render_frame(&mut app);
    }

    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    vt.assert_no_spinners_in_scrollback();
}

// ─── Graduated thinking is collapsed ───────────────────────────────────────

#[test]
fn graduated_thinking_is_collapsed() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Think deeply".into()));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "This is a long chain of reasoning that should be collapsed after graduation".into(),
    ));
    app.on_message(ChatAppMsg::TextDelta("Final answer.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let scrollback = vt.scrollback_contents();
    let full = vt.full_history();
    let stripped_full = crucible_oil::ansi::strip_ansi(&full);

    // Graduated thinking should show collapsed form ("Thought" + word count)
    // not the full thinking content
    assert!(
        stripped_full.contains("Thought"),
        "Graduated thinking should show 'Thought' label.\nFull:\n{}",
        stripped_full
    );
    assert!(
        stripped_full.contains("words)"),
        "Graduated thinking should show word count.\nFull:\n{}",
        stripped_full
    );

    // The full raw thinking text should NOT appear in scrollback
    let stripped_scrollback = crucible_oil::ansi::strip_ansi(&scrollback);
    assert!(
        !stripped_scrollback.contains("long chain of reasoning"),
        "Full thinking text should not be in scrollback (should be collapsed).\nScrollback:\n{}",
        stripped_scrollback
    );
}

// ─── Graduation preserves content ──────────────────────────────────────────

#[test]
fn graduation_preserves_content() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Explain Rust".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Rust is a systems programming language.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);

    assert!(
        stripped.contains("Explain Rust"),
        "User message should survive graduation.\nFull:\n{}",
        stripped
    );
    assert!(
        stripped.contains("systems programming language"),
        "Assistant text should survive graduation.\nFull:\n{}",
        stripped
    );
}

#[test]
fn graduation_preserves_tool_results() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Check files".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "Cargo.toml"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".into(),
        delta: "[package]\nname = \"test\"".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);

    assert!(
        stripped.contains("Cargo.toml"),
        "Tool args should be in graduated content.\nFull:\n{}",
        stripped
    );
}

// ─── Turn indicator not in scrollback ──────────────────────────────────────

#[test]
fn turn_indicator_not_in_scrollback() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Send a message and let it stream (turn indicator should be active)
    app.on_message(ChatAppMsg::UserMessage("Test question".into()));
    app.on_message(ChatAppMsg::TextDelta("Streaming response".into()));
    vt.render_frame(&mut app); // renders with active turn indicator

    // Complete the stream
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Start a new turn to push previous content to scrollback
    app.on_message(ChatAppMsg::UserMessage("Follow up".into()));
    app.on_message(ChatAppMsg::TextDelta("Second response".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Check scrollback has no spinner characters
    vt.assert_no_spinners_in_scrollback();
}

// ─── Edge cases ────────────────────────────────────────────────────────────

#[test]
fn empty_stream_complete_does_not_crash() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // StreamComplete without any prior content
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Should not panic, screen should be usable
    let screen = vt.screen_contents();
    assert!(!screen.is_empty(), "Screen should not be empty after render");
}

#[test]
fn cancelled_stream_graduates_cleanly() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Start something".into()));
    app.on_message(ChatAppMsg::TextDelta("Partial respon".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::StreamCancelled);
    vt.render_frame(&mut app);

    vt.assert_no_spinners_in_scrollback();

    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);
    assert!(
        stripped.contains("Partial respon"),
        "Partial content should survive cancellation.\nFull:\n{}",
        stripped
    );
}
