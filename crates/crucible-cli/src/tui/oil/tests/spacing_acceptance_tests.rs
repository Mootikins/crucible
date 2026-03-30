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

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Send a thinking delta (creates/appends to AssistantResponse with thinking).
fn think(app: &mut OilChatApp, content: &str) {
    app.on_message(ChatAppMsg::ThinkingDelta(content.into()));
}

/// Send a tool call and complete it immediately.
fn tool(app: &mut OilChatApp, name: &str, call_id: &str) {
    app.on_message(ChatAppMsg::ToolCall {
        name: name.into(),
        args: format!(r#"{{"path": "{call_id}.rs"}}"#),
        call_id: Some(call_id.into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: name.into(),
        call_id: Some(call_id.into()),
    });
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

    assert_eq!(q1_to_a1, Some(1), "Q1→A1 spacing\nFull output:\n{}", output);
    assert_eq!(a1_to_q2, Some(1), "A1→Q2 spacing\nFull output:\n{}", output);
    assert_eq!(q2_to_a2, Some(1), "Q2→A2 spacing\nFull output:\n{}", output);
    assert_no_double_blanks(&output, "multi_frame_graduation");
}

/// Reproduce exact screenshot scenario: thinking → tools → thinking → tools → thinking+text.
/// Each thinking block is an AssistantResponse; tools interrupt it, creating a new
/// AssistantResponse for the next thinking block. Spacing rule: 1 blank line between
/// every non-ToolGroup transition.
#[test]
fn thinking_tools_thinking_tools_text_spacing() {
    let mut app = OilChatApp::init();
    let mut runtime = TestRuntime::new(120, 40);

    // User message
    app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
    live_render(&mut app, &mut runtime);

    // Thought #1
    think(
        &mut app,
        "I'll explore the repository to give you a comprehensive overview.",
    );
    live_render(&mut app, &mut runtime);

    // Tool batch #1 (5 tools)
    tool(&mut app, "get_kiln_info", "t1");
    tool(&mut app, "read_file", "t2");
    tool(&mut app, "glob", "t3");
    tool(&mut app, "read_note", "t4");
    tool(&mut app, "read_note", "t5");
    live_render(&mut app, &mut runtime);

    // Thought #2 (short)
    think(
        &mut app,
        "Let me check more details about the crate structure.",
    );
    live_render(&mut app, &mut runtime);

    // Tool batch #2 (2 tools)
    tool(&mut app, "glob", "t6");
    tool(&mut app, "read_note", "t7");
    live_render(&mut app, &mut runtime);

    // Thought #3 + assistant text
    think(&mut app, "Now I have enough context to give a full answer.");
    app.on_message(ChatAppMsg::TextDelta(
        "Crucible is a knowledge-grounded agent runtime.".into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime);

    let output = screen(&runtime);

    // Symptom 1: Thought#1 → tools should have 1 blank line
    let thought1_to_tools = blank_lines_between(&output, "comprehensive overview", "Get Kiln Info");
    assert_eq!(
        thought1_to_tools,
        Some(1),
        "Symptom 1: No blank line after Thought #1 (before tools).\nFull output:\n{}",
        output
    );

    // Symptom 2: tools → Thought#2 should have exactly 1 blank line (not 2)
    let tools_to_thought2 = blank_lines_between(&output, "t5.rs", "crate structure");
    assert_eq!(
        tools_to_thought2,
        Some(1),
        "Symptom 2: Wrong number of blank lines between last tool and Thought #2.\nFull output:\n{}",
        output
    );

    // Symptom 3: Thought#2 → tools should have 1 blank line
    let thought2_to_tools = blank_lines_between(&output, "crate structure", "t6.rs");
    assert_eq!(
        thought2_to_tools,
        Some(1),
        "Symptom 3: No blank line after Thought #2 (before tools).\nFull output:\n{}",
        output
    );

    assert_no_double_blanks(&output, "thinking_tools_thinking");
}

/// Same scenario but with show_thinking=false (collapsed ◇ Thought summary).
/// This is the actual user-reported bug: spacing breaks in collapsed mode.
#[test]
fn thinking_tools_spacing_collapsed_mode() {
    let mut app = OilChatApp::init();
    app.set_show_thinking(false);
    let mut runtime = TestRuntime::new(120, 40);

    // User message
    app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
    live_render(&mut app, &mut runtime);

    // Thought #1
    think(
        &mut app,
        "I'll explore the repository to give you a comprehensive overview.",
    );
    live_render(&mut app, &mut runtime);

    // Tool batch #1
    tool(&mut app, "get_kiln_info", "t1");
    tool(&mut app, "read_file", "t2");
    tool(&mut app, "read_note", "t3");
    live_render(&mut app, &mut runtime);

    // Thought #2
    think(&mut app, "Let me check more details.");
    live_render(&mut app, &mut runtime);

    // Tool batch #2
    tool(&mut app, "glob", "t4");
    tool(&mut app, "read_note", "t5");
    live_render(&mut app, &mut runtime);

    // Thought #3 + text
    think(&mut app, "Now I have enough context.");
    app.on_message(ChatAppMsg::TextDelta(
        "Crucible is a knowledge-grounded agent runtime.".into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime);

    let output = screen(&runtime);

    // Symptom 1: Thought#1 → tools should have 1 blank line
    let thought1_to_tools = blank_lines_between(&output, "Thought (10", "Get Kiln Info");
    assert_eq!(
        thought1_to_tools,
        Some(1),
        "Symptom 1: No blank line after collapsed Thought #1.\nFull output:\n{}",
        output
    );

    // Symptom 2: tools → Thought#2 should have exactly 1 blank line
    let tools_to_thought2 = blank_lines_between(&output, "t3.rs", "Thought (5");
    assert_eq!(
        tools_to_thought2,
        Some(1),
        "Symptom 2: Wrong blank lines between tools and collapsed Thought #2.\nFull output:\n{}",
        output
    );

    // Symptom 3: Between t3.rs and t4.rs there should be:
    // 1 blank line, ◇ Thought, 1 blank line = 2 blank lines total
    let thought2_to_tools = blank_lines_between(&output, "t3.rs", "t4.rs");
    assert_eq!(
        thought2_to_tools,
        Some(2), // blank + Thought(5) + blank = 2 blank lines in between
        "Symptom 3: Wrong spacing around collapsed Thought #2.\nFull output:\n{}",
        output
    );

    assert_no_double_blanks(&output, "thinking_tools_collapsed");
}

/// Simulate realistic tick-per-event rendering like the real TUI.
/// The real TUI calls render_frame on every tick, not just at convenient points.
/// This tests that rapid-fire events with a render between each still space correctly.
#[test]
fn thinking_tools_collapsed_tick_per_event() {
    let mut app = OilChatApp::init();
    app.set_show_thinking(false);
    let mut runtime = TestRuntime::new(120, 40);

    // User message + render
    app.on_message(ChatAppMsg::UserMessage("tell me about this repo".into()));
    live_render(&mut app, &mut runtime);

    // Thought #1: 10 words
    think(
        &mut app,
        "I will explore the repository to give you a comprehensive overview.",
    );
    live_render(&mut app, &mut runtime);

    // Each tool arrives with a render between them
    tool(&mut app, "get_kiln_info", "t1");
    live_render(&mut app, &mut runtime);
    tool(&mut app, "read_file", "t2");
    live_render(&mut app, &mut runtime);
    tool(&mut app, "read_note", "t3");
    live_render(&mut app, &mut runtime);

    // Thought #2: 3 words
    think(&mut app, "Let me check.");
    live_render(&mut app, &mut runtime);

    // More tools with renders
    tool(&mut app, "glob", "t4");
    live_render(&mut app, &mut runtime);
    tool(&mut app, "read_note", "t5");
    live_render(&mut app, &mut runtime);

    // Final thought + text + complete: 5 words
    think(&mut app, "Now I can answer fully.");
    live_render(&mut app, &mut runtime);
    app.on_message(ChatAppMsg::TextDelta("Crucible is great.".into()));
    live_render(&mut app, &mut runtime);
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime);

    let output = screen(&runtime);

    // Verify spacing structure by counting blank lines between consecutive
    // content lines. Content lines are non-blank stripped lines.
    let lines: Vec<&str> = output.lines().collect();
    let content_lines: Vec<(usize, &str)> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| (i, *l))
        .collect();

    // Between each pair of content lines, count blank lines
    let mut spacing_issues = Vec::new();
    for window in content_lines.windows(2) {
        let (i1, l1) = window[0];
        let (i2, l2) = window[1];
        let blanks = i2 - i1 - 1;

        // Determine expected spacing:
        // - Within the user input bar (▄/▀ decorations): 0
        // - ToolGroup→ToolGroup: 0
        // - Everything else: 1
        let is_tool = |l: &str| l.contains("✓") || l.contains("✗") || l.contains("●");
        let is_decoration = |l: &str| {
            l.trim().chars().all(|c| c == '▄' || c == '▀' || c == ' ')
                || l.contains("NORMAL")
                || l.contains("ctx")
        };
        if is_decoration(l1) || is_decoration(l2) {
            continue; // Skip UI chrome
        }

        let expected = if is_tool(l1) && is_tool(l2) { 0 } else { 1 };

        if blanks != expected {
            spacing_issues.push(format!(
                "Lines {}-{}: expected {} blank(s), got {}.\n  L{}: {:?}\n  L{}: {:?}",
                i1,
                i2,
                expected,
                blanks,
                i1,
                l1.trim(),
                i2,
                l2.trim()
            ));
        }
    }

    assert!(
        spacing_issues.is_empty(),
        "Spacing issues found:\n{}\n\nFull output:\n{}",
        spacing_issues.join("\n"),
        output
    );
}

/// Simpler variant: single thinking → single tool group → assistant text.
/// Isolates the thinking-to-tool spacing without multi-batch complexity.
#[test]
fn thinking_then_tools_has_one_blank_line() {
    let mut app = OilChatApp::init();
    let mut runtime = TestRuntime::new(80, 30);

    app.on_message(ChatAppMsg::UserMessage("Hello".into()));
    live_render(&mut app, &mut runtime);

    // Thinking arrives
    think(&mut app, "Let me look into this.");
    live_render(&mut app, &mut runtime);

    // Tool call arrives (closes the thinking AssistantResponse)
    tool(&mut app, "read_file", "c1");
    live_render(&mut app, &mut runtime);

    // Complete
    app.on_message(ChatAppMsg::TextDelta("Here is what I found.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime);

    let output = screen(&runtime);

    // Thought → tool: 1 blank line
    let thought_to_tool = blank_lines_between(&output, "Thought", "Read File");
    assert_eq!(
        thought_to_tool,
        Some(1),
        "Expected 1 blank line between thinking and tool.\nFull output:\n{}",
        output
    );

    // Tool → text: 1 blank line
    let tool_to_text = blank_lines_between(&output, "Read File", "Here is what I found");
    assert_eq!(
        tool_to_text,
        Some(1),
        "Expected 1 blank line between tool and assistant text.\nFull output:\n{}",
        output
    );
}

/// Tools → thinking: the reverse direction. One blank line expected.
#[test]
fn tools_then_thinking_has_one_blank_line() {
    let mut app = OilChatApp::init();
    let mut runtime = TestRuntime::new(80, 30);

    app.on_message(ChatAppMsg::UserMessage("Check this".into()));

    // Tool first (before any thinking)
    tool(&mut app, "glob", "c1");
    live_render(&mut app, &mut runtime);

    // Thinking after tool
    think(&mut app, "Interesting results from the glob.");
    app.on_message(ChatAppMsg::TextDelta("Found the files.".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    live_render(&mut app, &mut runtime);

    let output = screen(&runtime);

    // Tool → thought: 1 blank line
    let tool_to_thought = blank_lines_between(&output, "Glob", "Thought");
    assert_eq!(
        tool_to_thought,
        Some(1),
        "Expected 1 blank line between tool and thinking.\nFull output:\n{}",
        output
    );
}
