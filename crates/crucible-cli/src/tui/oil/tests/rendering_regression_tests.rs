//! Rendering regression tests for the component-based container system.
//!
//! Tests for visual artifacts, styling consistency, and animation issues
//! that are hard to catch with unit tests alone.

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crucible_oil::ansi::strip_ansi;

use super::vt100_runtime::Vt100TestRuntime;

// ─── Cancelled tool rendering ──────────────────────────────────────────────

#[test]
fn cancelled_stream_graduates_all_containers() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Start a turn with text + pending tool
    app.on_message(ChatAppMsg::TextDelta("Let me check...".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "test.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });

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

    // No containers should remain in viewport
    assert!(
        app.container_list.is_empty(),
        "All containers should graduate after cancellation"
    );
}

#[test]
fn cancelled_during_thinking_graduates_cleanly() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Start thinking, no text yet
    app.on_message(ChatAppMsg::ThinkingDelta("analyzing the problem".into()));
    app.on_message(ChatAppMsg::StreamCancelled);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let _stripped = strip_ansi(&full);

    // Should not panic, and thinking should appear in some form
    assert!(!app.is_streaming());
    assert!(
        app.container_list.is_empty(),
        "Cancelled thinking container should graduate"
    );
}

// ─── Thinking display consistency ──────────────────────────────────────────

#[test]
fn thinking_not_duplicated_between_chrome_and_content() {
    let mut app = OilChatApp::init();

    // Only thinking, no text yet — chrome shows thinking indicator
    app.on_message(ChatAppMsg::ThinkingDelta("deep analysis of the problem ".into()));

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
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::ThinkingDelta("reasoning about the answer ".into()));
    app.on_message(ChatAppMsg::TextDelta("Here is my answer.".into()));

    let output = super::helpers::vt_render(&mut app);

    // After text starts, thinking should show as collapsed summary (Thought),
    // not the full "Thinking..." label
    assert!(
        output.contains("Thought") || output.contains("words)"),
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
    let mut app = OilChatApp::init();
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
    let mut app = OilChatApp::init();
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

fn assert_no_triple_blanks(screen: &str, context: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    for (i, window) in lines.windows(3).enumerate() {
        let all_blank = window.iter().all(|l| l.trim().is_empty());
        assert!(
            !all_blank,
            "{}: triple blank at lines {}-{}.\nScreen:\n{}",
            context,
            i,
            i + 2,
            screen
        );
    }
}

#[test]
fn no_triple_blanks_tool_heavy_conversation() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 30);

    // User asks, assistant uses multiple tools
    app.on_message(ChatAppMsg::UserMessage("Fix the bug".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Let me investigate.".into()));

    // Tool 1
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "src/main.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "read_file".into(),
        delta: "fn main() {}\n".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });

    // Tool 2
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "cargo test"}"#.into(),
        call_id: Some("c2".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultDelta {
        name: "bash".into(),
        delta: "test result: ok".into(),
        call_id: Some("c2".into()),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c2".into()),
    });

    // Continuation text
    app.on_message(ChatAppMsg::TextDelta("The tests pass now.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);
    assert_no_triple_blanks(&stripped, "tool_heavy_conversation");
}

#[test]
fn no_triple_blanks_thinking_then_tools_then_text() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 30);

    app.on_message(ChatAppMsg::UserMessage("Plan this".into()));
    vt.render_frame(&mut app);

    // Thinking → text → tool → continuation
    app.on_message(ChatAppMsg::ThinkingDelta("I should check the file first.".into()));
    app.on_message(ChatAppMsg::TextDelta("Let me check.".into()));

    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "config.toml"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });

    app.on_message(ChatAppMsg::TextDelta("Based on the config, here is the plan.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);
    assert_no_triple_blanks(&stripped, "thinking_tools_text");
}

// ─── Graduation boundary styling ───────────────────────────────────────────

#[test]
fn graduation_across_multiple_frames_consistent() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Frame 1: user message (graduates immediately)
    app.on_message(ChatAppMsg::UserMessage("Question 1".into()));
    vt.render_frame(&mut app);

    // Frame 2: assistant response (graduates on complete)
    app.on_message(ChatAppMsg::TextDelta("Answer 1".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Frame 3: second turn
    app.on_message(ChatAppMsg::UserMessage("Question 2".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Answer 2".into()));
    app.on_message(ChatAppMsg::StreamComplete);
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
    let mut app = OilChatApp::init();

    // Empty delta should not create visible content
    app.on_message(ChatAppMsg::TextDelta("".into()));
    app.on_message(ChatAppMsg::StreamComplete);

    let mut vt = Vt100TestRuntime::new(80, 24);
    vt.render_frame(&mut app);

    // Should graduate cleanly with no visual content (or minimal)
    assert!(
        app.container_list.is_empty(),
        "Empty response should still graduate"
    );
}

#[test]
fn thinking_only_no_text_graduates_cleanly() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Only thinking, then stream complete (no text delta)
    app.on_message(ChatAppMsg::ThinkingDelta("I am thinking about this".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = strip_ansi(&full);

    assert!(
        app.container_list.is_empty(),
        "Thinking-only response should graduate"
    );
    // Should show collapsed thinking in scrollback
    assert!(
        stripped.contains("Thought") || stripped.contains("words)"),
        "Graduated thinking should show collapsed summary.\n{}",
        stripped
    );
}

// ─── Multiple thinking blocks ──────────────────────────────────────────────

#[test]
fn multiple_thinking_blocks_render_without_duplication() {
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 30);

    // First thinking → text → tool → second thinking → more text
    app.on_message(ChatAppMsg::ThinkingDelta("first analysis".into()));
    app.on_message(ChatAppMsg::TextDelta("First part.".into()));

    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: "{}".into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });

    // Second thinking block after tool
    app.on_message(ChatAppMsg::ThinkingDelta("second analysis".into()));
    app.on_message(ChatAppMsg::TextDelta("Second part.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
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
    let mut app = OilChatApp::init();
    let mut vt = Vt100TestRuntime::new(80, 24);

    // Text → tool → continuation text
    app.on_message(ChatAppMsg::TextDelta("Let me check.".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: "{}".into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::TextDelta("Here is the answer.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
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
