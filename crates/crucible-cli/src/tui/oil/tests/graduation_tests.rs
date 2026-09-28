//! Graduation invariant tests.
//!
//! These tests verify the critical invariants of the graduation system:
//! - No spinners in scrollback (the original bug that motivated the container model)
//! - Thinking blocks are collapsed after graduation
//! - All content is preserved through graduation
//! - Turn indicator (spinner) only appears in viewport chrome, never in scrollback

use super::vt100_runtime::Vt100TestRuntime;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::tests::helpers::{EventFeed, SendMsgs, ToolCallEvent};

// ─── No spinners in scrollback ─────────────────────────────────────────────

#[test]
fn no_spinners_in_scrollback() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("Test"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.thinking("reasoning about the problem"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.text("Answer"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    vt.assert_no_spinners_in_scrollback();
}

#[test]
fn no_spinners_in_scrollback_after_tool_use() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("Use a tool"));
    vt.render_frame(&mut app);

    // Tool call with pending state (renders spinner in viewport)
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "bash",
        call_id: "c1",
        args: r#"{"command": "echo hello"}"#,
        render: Some("echo hello".into()),
        ..Default::default()
    }));
    vt.render_frame(&mut app);

    // Complete the tool and stream
    app.send_msgs(feed.tool_result("bash", "c1", ""));
    app.send_msgs(feed.text("Done."));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    vt.assert_no_spinners_in_scrollback();
}

#[test]
fn no_spinners_after_multi_tool_graduation() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("Multi-tool"));
    vt.render_frame(&mut app);

    // Multiple tools with renders between
    for i in 0..3 {
        let id = format!("c{}", i);
        app.send_msgs(feed.tool_call("read_file", &id, &format!(r#"{{"path": "file{}.rs"}}"#, i)));
        vt.render_frame(&mut app); // spinner visible during pending

        app.send_msgs(feed.tool_result("read_file", &id, ""));
        vt.render_frame(&mut app);
    }

    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    vt.assert_no_spinners_in_scrollback();
}

// ─── Graduated thinking is collapsed ───────────────────────────────────────

#[test]
fn graduated_thinking_is_collapsed() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("Think deeply"));
    app.send_msgs(
        feed.thinking(
            "This is a long chain of reasoning that should be collapsed after graduation",
        ),
    );
    app.send_msgs(feed.text("Final answer."));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let scrollback = vt.scrollback_contents();
    let full = vt.full_history();
    let stripped_full = crucible_oil::ansi::strip_ansi(&full);

    // Graduated thinking should show collapsed form ("Thought" + token
    // estimate), not the full thinking content
    assert!(
        stripped_full.contains("Thought"),
        "Graduated thinking should show 'Thought' label.\nFull:\n{}",
        stripped_full
    );
    assert!(
        stripped_full.contains("tokens)"),
        "Graduated thinking should show token estimate.\nFull:\n{}",
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
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("Explain Rust"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.text("Rust is a systems programming language."));
    app.send_msgs(feed.complete());
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
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("Check files"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "read_file",
        call_id: "c1",
        args: r#"{"path": "Cargo.toml"}"#,
        render: Some("Cargo.toml".into()),
        ..Default::default()
    }));
    app.send_msgs(feed.tool_result("read_file", "c1", "[package]\nname = \"test\""));
    app.send_msgs(feed.complete());
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
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Send a message and let it stream (turn indicator should be active)
    app.send_msgs(feed.user("Test question"));
    app.send_msgs(feed.text("Streaming response"));
    vt.render_frame(&mut app); // renders with active turn indicator

    // Complete the stream
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    // Start a new turn to push previous content to scrollback
    app.send_msgs(feed.user("Follow up"));
    app.send_msgs(feed.text("Second response"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    // Check scrollback has no spinner characters
    vt.assert_no_spinners_in_scrollback();
}

// ─── Edge cases ────────────────────────────────────────────────────────────

#[test]
fn empty_stream_complete_does_not_crash() {
    let mut app = OilChatApp::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // StreamComplete without any prior content
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Should not panic, screen should be usable
    let screen = vt.screen_contents();
    assert!(
        !screen.is_empty(),
        "Screen should not be empty after render"
    );
}

#[test]
fn cancelled_stream_graduates_cleanly() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    app.send_msgs(feed.user("Start something"));
    app.send_msgs(feed.text("Partial respon"));
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
