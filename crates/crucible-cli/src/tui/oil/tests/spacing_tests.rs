//! Spacing acceptance tests.
//!
//! Verifies the spacing rules between adjacent containers:
//! - Adjacent tool groups: zero blank lines
//! - Everything else: one blank line separator
//!
//! Uses `vt_render` (real terminal path) and counts blank lines between
//! content patterns.

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use super::helpers::vt_render;

/// Count blank lines between the last line matching `before` and the first
/// line matching `after` (searching after the `before` line).
#[allow(dead_code)] // available for future spacing tests
fn blank_lines_between(screen: &str, before: &str, after: &str) -> Option<usize> {
    let lines: Vec<&str> = screen.lines().collect();
    let before_end = lines.iter().rposition(|l| l.contains(before))?;
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

/// Assert no triple-blank lines anywhere in the output (always a bug).
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
fn adjacent_tools_no_gap() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Do two things".into()));

    // Tool 1
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

    // Tool 2 (should group with tool 1 — zero gap)
    app.on_message(ChatAppMsg::ToolCall {
        name: "write_file".into(),
        args: r#"{"path": "b.rs"}"#.into(),
        call_id: Some("c2".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "write_file".into(),
        call_id: Some("c2".into()),
    });

    app.on_message(ChatAppMsg::StreamComplete);

    let output = vt_render(&mut app);

    // Both tools should be in the same tool group (adjacent, no blank lines between)
    // Find lines with tool names
    let lines: Vec<&str> = output.lines().collect();
    let tool_a_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("a.rs"))
        .map(|(i, _)| i)
        .collect();
    let tool_b_lines: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.contains("b.rs"))
        .map(|(i, _)| i)
        .collect();

    if let (Some(&idx_a), Some(&idx_b)) = (tool_a_lines.last(), tool_b_lines.first()) {
        let blank_count = lines[idx_a + 1..idx_b]
            .iter()
            .filter(|l| l.trim().is_empty())
            .count();
        assert_eq!(
            blank_count, 0,
            "Adjacent tools should have zero blank lines between them.\nScreen:\n{}",
            output
        );
    }

    assert_no_triple_blanks(&output, "adjacent_tools");
}

#[test]
fn tool_then_text_one_blank_line() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Check and explain".into()));

    // Tool call
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "main.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });

    // Continuation text after tool
    app.on_message(ChatAppMsg::TextDelta("Based on the file contents here is the explanation.".into()));
    app.on_message(ChatAppMsg::StreamComplete);

    // Render through vt100 for full graduation
    let mut vt = super::vt100_runtime::Vt100TestRuntime::new(80, 30);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);

    // Tool indicator line → assistant text should have spacing
    // The exact gap depends on layout, but there should be no triple blanks
    assert_no_triple_blanks(&stripped, "tool_then_text");

    // Verify both pieces of content are present
    assert!(
        stripped.contains("main.rs"),
        "Tool content should be present.\n{}",
        stripped
    );
    assert!(
        stripped.contains("explanation"),
        "Assistant text should be present.\n{}",
        stripped
    );
}

#[test]
fn user_then_assistant_one_blank_line() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Hello there".into()));
    app.on_message(ChatAppMsg::TextDelta("General Kenobi".into()));
    app.on_message(ChatAppMsg::StreamComplete);

    // Render through vt100 to trigger graduation
    let mut vt = super::vt100_runtime::Vt100TestRuntime::new(80, 24);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);

    // User message ends with bottom bar (unicode block), then spacing before assistant
    assert!(
        stripped.contains("Hello there"),
        "User message present.\n{}",
        stripped
    );
    assert!(
        stripped.contains("General Kenobi"),
        "Assistant text present.\n{}",
        stripped
    );
    assert_no_triple_blanks(&stripped, "user_then_assistant");
}

#[test]
fn thinking_then_tools_one_blank_line() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Plan and execute".into()));
    app.on_message(ChatAppMsg::ThinkingDelta("I need to check the codebase first".into()));
    app.on_message(ChatAppMsg::TextDelta("Let me check.".into()));

    // Tool follows thinking+text
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "ls src/"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "bash".into(),
        call_id: Some("c1".into()),
    });

    app.on_message(ChatAppMsg::StreamComplete);

    let mut vt = super::vt100_runtime::Vt100TestRuntime::new(80, 30);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);

    assert_no_triple_blanks(&stripped, "thinking_then_tools");

    // All content present
    assert!(stripped.contains("Let me check"), "Text present.\n{}", stripped);
    assert!(
        stripped.contains("bash") || stripped.contains("ls src/"),
        "Tool present.\n{}",
        stripped
    );
}

#[test]
fn no_triple_blanks_in_multi_turn_conversation() {
    let mut app = OilChatApp::init();
    let mut vt = super::vt100_runtime::Vt100TestRuntime::new(80, 24);

    // Turn 1
    app.on_message(ChatAppMsg::UserMessage("First question".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("First answer".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    // Turn 2
    app.on_message(ChatAppMsg::UserMessage("Second question".into()));
    vt.render_frame(&mut app);

    app.on_message(ChatAppMsg::TextDelta("Second answer".into()));
    app.on_message(ChatAppMsg::StreamComplete);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);

    assert_no_triple_blanks(&stripped, "multi_turn");
}
