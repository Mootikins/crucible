//! Spacing acceptance tests — exercises the LIVE rendering path.
//!
//! These tests use `chat_runner::render_frame()` which calls `drain_graduated()`
//! before rendering, matching the real TUI flow. This is critical because
//! snapshot tests use a fresh FramePlanner per call and never graduate content,
//! so they only test viewport spacing (Taffy gap), not stdout spacing.
//!
//! The spacing rule is simple: one blank line between all non-tool-group
//! containers. Consecutive tool groups are tight (no blank line).

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::chat_runner::render_frame;
use crate::tui::oil::focus::FocusContext;
use crucible_oil::TestRuntime;

/// Simulate the live rendering loop: drain_graduated + render viewport.
///
/// This is the ONLY correct way to test what the user sees. It mirrors
/// `chat_runner::render_frame()` exactly.
fn live_render(app: &mut OilChatApp, runtime: &mut TestRuntime) {
    let focus = FocusContext::new();
    render_frame(app, runtime, &focus);
}

/// Get the full screen content (stdout + viewport), ANSI-stripped.
fn screen(runtime: &TestRuntime) -> String {
    let stdout = runtime.stdout_content();
    let viewport = runtime.viewport_content();
    strip_ansi(&format!("{}{}", stdout, viewport))
}

/// Assert no double blank lines appear anywhere in the output.
fn assert_no_double_blanks(output: &str, context: &str) {
    let lines: Vec<&str> = output.lines().collect();
    for (i, window) in lines.windows(3).enumerate() {
        let all_blank = window.iter().all(|l| l.trim().is_empty());
        assert!(
            !all_blank,
            "{}: found triple blank at lines {}-{}: {:?}",
            context,
            i,
            i + 2,
            window
        );
    }
}

/// Count blank lines between two content patterns in the output.
fn blank_lines_between(output: &str, before: &str, after: &str) -> Option<usize> {
    let lines: Vec<&str> = output.lines().collect();

    // Find the LAST line matching `before`
    let before_end = lines.iter().rposition(|l| l.contains(before))?;
    // Find the FIRST line matching `after` that comes after `before_end`
    let after_start = lines[before_end + 1..]
        .iter()
        .position(|l| l.contains(after))
        .map(|p| p + before_end + 1)?;

    let blanks = lines[before_end + 1..after_start]
        .iter()
        .filter(|l| l.trim().is_empty())
        .count();
    Some(blanks)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// User message → assistant response: exactly one blank line between them
/// after graduation across multiple frames.
#[test]
fn user_then_assistant_has_one_blank_line() {
    let mut app = OilChatApp::init();
    let mut runtime = TestRuntime::new(80, 24);

    // Frame 1: user message
    app.on_message(ChatAppMsg::UserMessage("What is 2+2?".into()));
    live_render(&mut app, &mut runtime);

    // Frame 2: assistant streams
    app.on_message(ChatAppMsg::TextDelta("The answer is 4.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime);

    let output = screen(&runtime);

    let blanks = blank_lines_between(&output, "What is 2+2?", "The answer is 4.");
    assert_eq!(
        blanks,
        Some(1),
        "Expected exactly 1 blank line between user prompt and assistant response.\nFull output:\n{}",
        output
    );
    assert_no_double_blanks(&output, "user_then_assistant");
}

/// System message (Precognition) between user and assistant: blank lines on both sides.
#[test]
fn user_system_assistant_spacing() {
    let mut app = OilChatApp::init();
    let mut runtime = TestRuntime::new(80, 24);

    // Frame 1: user message arrives and graduates
    app.on_message(ChatAppMsg::UserMessage("Hello".into()));
    live_render(&mut app, &mut runtime);

    // Frame 2: system message (Precognition) arrives and graduates
    app.on_message(ChatAppMsg::PrecognitionResult {
        notes_count: 3,
        notes: vec![],
    });
    live_render(&mut app, &mut runtime);

    // Frame 3: assistant streams
    app.on_message(ChatAppMsg::TextDelta("Response text".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime);

    let output = screen(&runtime);

    // User → System: 1 blank line
    let user_to_system = blank_lines_between(&output, "Hello", "Found 3 relevant notes");
    assert_eq!(
        user_to_system,
        Some(1),
        "Expected 1 blank line between user and system message.\nFull output:\n{}",
        output
    );

    // System → Assistant: 1 blank line
    let system_to_assistant =
        blank_lines_between(&output, "Found 3 relevant notes", "Response text");
    assert_eq!(
        system_to_assistant,
        Some(1),
        "Expected 1 blank line between system message and assistant response.\nFull output:\n{}",
        output
    );

    assert_no_double_blanks(&output, "user_system_assistant");
}

/// Consecutive tool groups: tight (no blank line between them).
#[test]
fn consecutive_tools_are_tight() {
    let mut app = OilChatApp::init();
    let mut runtime = TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Do stuff".into()));

    // Two tool calls in sequence
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "a.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "b.rs"}"#.into(),
        call_id: Some("c2".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c2".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);

    // Render enough frames for graduation
    for _ in 0..3 {
        live_render(&mut app, &mut runtime);
    }

    let output = screen(&runtime);

    // Both tool calls should appear with no blank line between them
    let blanks = blank_lines_between(&output, "a.rs", "b.rs");
    assert_eq!(
        blanks,
        Some(0),
        "Expected 0 blank lines between consecutive tool calls.\nFull output:\n{}",
        output
    );
}

/// Tool group → assistant text: one blank line.
#[test]
fn tool_then_assistant_has_one_blank_line() {
    let mut app = OilChatApp::init();
    let mut runtime = TestRuntime::new(80, 24);

    app.on_message(ChatAppMsg::UserMessage("Do stuff".into()));

    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"cmd": "ls"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });
    live_render(&mut app, &mut runtime);

    // Assistant responds after tool
    app.on_message(ChatAppMsg::TextDelta("Here are the files.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime);

    let output = screen(&runtime);

    let blanks = blank_lines_between(&output, "Bash", "Here are the files");
    assert_eq!(
        blanks,
        Some(1),
        "Expected 1 blank line between tool group and assistant text.\nFull output:\n{}",
        output
    );
}

/// Multi-turn: spacing consistent across graduation boundaries.
/// This is the cross-frame spacing bug — user graduates in frame N,
/// system message in frame N+1, assistant in frame N+2.
#[test]
fn multi_frame_graduation_spacing_consistent() {
    let mut app = OilChatApp::init();
    let mut runtime = TestRuntime::new(80, 24);

    // Turn 1: user → assistant (each graduates on different frames)
    app.on_message(ChatAppMsg::UserMessage("First question".into()));
    live_render(&mut app, &mut runtime); // user graduates

    app.on_message(ChatAppMsg::TextDelta("First answer".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime); // assistant graduates

    // Turn 2: user → assistant
    app.on_message(ChatAppMsg::UserMessage("Second question".into()));
    live_render(&mut app, &mut runtime); // user graduates

    app.on_message(ChatAppMsg::TextDelta("Second answer".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime); // assistant graduates

    let output = screen(&runtime);

    // Every non-tool transition should have exactly 1 blank line
    let q1_to_a1 = blank_lines_between(&output, "First question", "First answer");
    let a1_to_q2 = blank_lines_between(&output, "First answer", "Second question");
    let q2_to_a2 = blank_lines_between(&output, "Second question", "Second answer");

    assert_eq!(
        q1_to_a1,
        Some(1),
        "Q1→A1 spacing\nFull output:\n{}",
        output
    );
    assert_eq!(
        a1_to_q2,
        Some(1),
        "A1→Q2 spacing\nFull output:\n{}",
        output
    );
    assert_eq!(
        q2_to_a2,
        Some(1),
        "Q2→A2 spacing\nFull output:\n{}",
        output
    );
    assert_no_double_blanks(&output, "multi_frame_graduation");
}
