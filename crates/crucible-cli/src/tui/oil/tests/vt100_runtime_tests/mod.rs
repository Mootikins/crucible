//! Screen-level tests driven through [`Vt100TestRuntime`], split from
//! `vt100_runtime.rs`, which keeps the harness itself.

mod spacing;
mod spinner_leak;

use super::vt100_runtime::Vt100TestRuntime;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::tests::helpers::{EventFeed, SendMsgs};

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

use super::helpers::assert_no_triple_blanks;

fn think(app: &mut OilChatApp, feed: &mut EventFeed, content: &str) {
    app.send_msgs(feed.thinking(content));
}

fn tool(app: &mut OilChatApp, feed: &mut EventFeed, name: &str, call_id: &str) {
    app.send_msgs(feed.tool_call(name, call_id, &format!(r#"{{"path": "{call_id}.rs"}}"#)));
    app.send_msgs(feed.tool_result(name, call_id, ""));
}

// ─── Bug 1: Spacing between graduated content and viewport ────────
//
// The user sees two blank lines between the graduated user message
// and the first thought/tool in the viewport. The root cause is
// the unconditional text(" ") at chat_app/mod.rs:176 combining
// with Terminal::apply()'s \r\n separator.
