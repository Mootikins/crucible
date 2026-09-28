//! Rendering regression tests for the component-based container system.
//!
//! Tests for visual artifacts, styling consistency, and animation issues
//! that are hard to catch with unit tests alone.

use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::tests::helpers::{EventFeed, SendMsgs, ToolCallEvent};
use crucible_oil::ansi::strip_ansi;

use super::vt100_runtime::Vt100TestRuntime;

// ─── Cancelled tool rendering ──────────────────────────────────────────────

#[test]
fn cancelled_stream_keeps_all_containers() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Start a turn with text + pending tool
    app.send_msgs(feed.text("Let me check..."));
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "read_file",
        call_id: "c1",
        args: r#"{"path": "test.rs"}"#,
        render: Some("test.rs".into()),
        ..Default::default()
    }));

    // Cancel mid-stream
    app.on_message(ChatAppMsg::StreamCancelled);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);

    // Both the text and tool should appear in scrollback
    assert!(
        stripped.contains("Let me check"),
        "Cancelled text should be in scrollback.\n{}",
        stripped
    );
    assert!(
        stripped.contains("read_file") || stripped.contains("test.rs"),
        "Cancelled tool should be in scrollback.\n{}",
        stripped
    );

    // The transcript keeps every node. A resize reprints from these nodes,
    // so a cancelled turn must stay in the list rather than be handed off.
    assert!(
        !app.container_list.is_empty(),
        "cancelled containers must stay in the transcript"
    );
}

#[test]
fn cancelled_during_thinking_keeps_the_node() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Start thinking, no text yet
    app.send_msgs(feed.thinking("analyzing the problem"));
    app.on_message(ChatAppMsg::StreamCancelled);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let _stripped = strip_ansi(&full);

    // Should not panic, and the cancelled turn stays in the transcript.
    assert!(!app.is_streaming());
    assert!(
        !app.container_list.is_empty(),
        "cancelled thinking must stay in the transcript"
    );
}

// ─── Thinking display consistency ──────────────────────────────────────────

#[test]
fn thinking_not_duplicated_between_chrome_and_content() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    // Only thinking, no text yet — chrome shows thinking indicator
    app.send_msgs(feed.thinking("deep analysis of the problem "));

    // Render viewport (not graduated yet)
    let output = super::helpers::vt_render(&mut app);

    // Count occurrences of "Thinking" — should appear at most once
    // (either in chrome turn indicator OR in container content, not both)
    let thinking_count = output.matches("Thinking").count();
    assert!(
        thinking_count <= 1,
        "Thinking should appear at most once, found {} times.\n{}",
        thinking_count,
        output
    );
}

#[test]
fn thinking_transitions_to_collapsed_on_text_start() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    app.send_msgs(feed.thinking("reasoning about the answer "));
    app.send_msgs(feed.text("Here is my answer."));

    let output = super::helpers::vt_render(&mut app);

    // After text starts, thinking should show as collapsed summary (Thought),
    // not the full "Thinking..." label
    assert!(
        output.contains("Thought") || output.contains("tokens)"),
        "After text starts, thinking should show collapsed summary.\n{}",
        output
    );
    assert!(
        output.contains("Here is my answer"),
        "Text content should be visible.\n{}",
        output
    );
}

// ─── User message styling ──────────────────────────────────────────────────

#[test]
fn user_message_has_consistent_width() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Hello world".into()));

    let mut vt = Vt100TestRuntime::new(60, 24);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);

    // User message should have top and bottom bars
    // (half-block characters: ▄ or ▀)
    let has_bars = stripped.contains('\u{2584}') || stripped.contains('\u{2580}');
    assert!(
        has_bars,
        "User message should have top/bottom edge bars.\n{}",
        stripped
    );
}

#[test]
fn user_message_wraps_long_text() {
    let mut app = OilChatApp::default();
    let long_text = "This is a very long message that should wrap across multiple lines when the terminal width is narrow enough to require wrapping behavior";
    app.on_message(ChatAppMsg::UserMessage(long_text.into()));

    let mut vt = Vt100TestRuntime::new(40, 24);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);

    // Text should be split across multiple lines
    let content_lines: Vec<&str> = stripped
        .lines()
        .filter(|l| l.contains("This") || l.contains("wrap") || l.contains("behavior"))
        .collect();
    assert!(
        content_lines.len() > 1,
        "Long user message should wrap at width=40.\n{}",
        stripped
    );
}

// ─── No triple blank lines invariant ───────────────────────────────────────

use super::helpers::assert_no_triple_blanks;

#[test]
fn no_triple_blanks_tool_heavy_conversation() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 30);

    // User asks, assistant uses multiple tools
    app.send_msgs(feed.user("Fix the bug"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.text("Let me investigate."));

    // Tool 1
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "read_file",
        call_id: "c1",
        args: r#"{"path": "src/main.rs"}"#,
        render: Some("src/main.rs".into()),
        ..Default::default()
    }));
    app.send_msgs(feed.tool_result("read_file", "c1", "fn main() {}\n"));

    // Tool 2
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "bash",
        call_id: "c2",
        args: r#"{"command": "cargo test"}"#,
        render: Some("cargo test".into()),
        ..Default::default()
    }));
    app.send_msgs(feed.tool_result("bash", "c2", "test result: ok"));

    // Continuation text
    app.send_msgs(feed.text("The tests pass now."));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);
    assert_no_triple_blanks(&stripped, "tool_heavy_conversation");
}

#[test]
fn no_triple_blanks_thinking_then_tools_then_text() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 30);

    app.send_msgs(feed.user("Plan this"));
    vt.render_frame(&mut app);

    // Thinking → text → tool → continuation
    app.send_msgs(feed.thinking("I should check the file first."));
    app.send_msgs(feed.text("Let me check."));

    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "read_file",
        call_id: "c1",
        args: r#"{"path": "config.toml"}"#,
        render: Some("config.toml".into()),
        ..Default::default()
    }));
    app.send_msgs(feed.tool_result("read_file", "c1", ""));

    app.send_msgs(feed.text("Based on the config, here is the plan."));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);
    assert_no_triple_blanks(&stripped, "thinking_tools_text");
}

// ─── Graduation boundary styling ───────────────────────────────────────────

#[test]
fn graduation_across_multiple_frames_consistent() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Frame 1: user message (graduates immediately)
    app.send_msgs(feed.user("Question 1"));
    vt.render_frame(&mut app);

    // Frame 2: assistant response (graduates on complete)
    app.send_msgs(feed.text("Answer 1"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    // Frame 3: second turn
    app.send_msgs(feed.user("Question 2"));
    vt.render_frame(&mut app);

    app.send_msgs(feed.text("Answer 2"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);

    // All four pieces of content should be in scrollback
    assert!(stripped.contains("Question 1"), "Q1 missing.\n{}", stripped);
    assert!(stripped.contains("Answer 1"), "A1 missing.\n{}", stripped);
    assert!(stripped.contains("Question 2"), "Q2 missing.\n{}", stripped);
    assert!(stripped.contains("Answer 2"), "A2 missing.\n{}", stripped);

    assert_no_triple_blanks(&stripped, "multi_frame_graduation");
}

// ─── Empty container edge cases ────────────────────────────────────────────

#[test]
fn empty_text_delta_does_not_create_visible_artifact() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();

    // Empty delta should not create visible content
    app.send_msgs(feed.text(""));
    app.send_msgs(feed.complete());

    let mut vt = Vt100TestRuntime::new(80, 24);
    vt.render_frame(&mut app);

    // The composer and status line always paint, so "no visible artifact"
    // means the transcript region above the composer stays blank.
    let stripped = strip_ansi(&vt.screen_contents());
    let transcript: String = stripped
        .lines()
        .take_while(|line| !line.contains('\u{2584}'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        transcript.trim().is_empty(),
        "an empty delta must not paint transcript content.\n{transcript}"
    );
}

#[test]
fn thinking_only_no_text_renders_cleanly() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Only thinking, then stream complete (no text delta)
    app.send_msgs(feed.thinking("I am thinking about this"));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);

    assert!(
        !app.container_list.is_empty(),
        "a thinking-only response must stay in the transcript"
    );
    // Should show collapsed thinking in scrollback
    assert!(
        stripped.contains("Thought") || stripped.contains("tokens)"),
        "Graduated thinking should show collapsed summary.\n{}",
        stripped
    );
}

// ─── Multiple thinking blocks ──────────────────────────────────────────────

#[test]
fn multiple_thinking_blocks_render_without_duplication() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 30);

    // First thinking → text → tool → second thinking → more text
    app.send_msgs(feed.thinking("first analysis"));
    app.send_msgs(feed.text("First part."));

    app.send_msgs(feed.tool_call("bash", "c1", "{}"));
    app.send_msgs(feed.tool_result("bash", "c1", ""));

    // Second thinking block after tool
    app.send_msgs(feed.thinking("second analysis"));
    app.send_msgs(feed.text("Second part."));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);

    // Both text parts should appear
    assert!(
        stripped.contains("First part"),
        "First text should be present.\n{}",
        stripped
    );
    assert!(
        stripped.contains("Second part"),
        "Second text should be present.\n{}",
        stripped
    );

    // Count "Thought" occurrences — should be at most 2 (one per thinking block)
    let thought_count = stripped.matches("Thought").count();
    assert!(
        thought_count <= 2,
        "Should have at most 2 'Thought' summaries, found {}.\n{}",
        thought_count,
        stripped
    );
}

// ─── Continuation margins ──────────────────────────────────────────────────

#[test]
fn continuation_after_tool_has_no_bullet() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Text → tool → continuation text
    app.send_msgs(feed.text("Let me check."));
    app.send_msgs(feed.tool_call("read_file", "c1", "{}"));
    app.send_msgs(feed.tool_result("read_file", "c1", ""));
    app.send_msgs(feed.text("Here is the answer."));
    app.send_msgs(feed.complete());
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);

    // Both text segments should be present
    assert!(
        stripped.contains("Let me check"),
        "Initial text should be present.\n{}",
        stripped
    );
    assert!(
        stripped.contains("Here is the answer"),
        "Continuation text should be present.\n{}",
        stripped
    );
}
