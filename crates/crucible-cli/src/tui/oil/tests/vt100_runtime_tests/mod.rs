//! Screen-level tests driven through [`Vt100TestRuntime`], split from
//! `vt100_runtime.rs` (harness) to stay under the module size ceilings.

mod spacing;
mod spinner_leak;

use super::vt100_runtime::Vt100TestRuntime;
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};

/// Count blank lines between two content patterns in screen text.
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

/// Assert no triple-blank lines (always a bug).
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

fn think(app: &mut OilChatApp, content: &str) {
    app.on_message(ChatAppMsg::ThinkingDelta(content.into()));
}

fn tool(app: &mut OilChatApp, name: &str, call_id: &str) {
    app.on_message(ChatAppMsg::ToolCall {
        name: name.into(),
        args: format!(r#"{{"path": "{call_id}.rs"}}"#),
        call_id: Some(call_id.into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: name.into(),
        call_id: Some(call_id.into()),
    });
}

// ─── Bug 1: Spacing between graduated content and viewport ────────
//
// The user sees two blank lines between the graduated user message
// and the first thought/tool in the viewport. The root cause is
// the unconditional text(" ") at chat_app/mod.rs:176 combining
// with Terminal::apply()'s \r\n separator.
