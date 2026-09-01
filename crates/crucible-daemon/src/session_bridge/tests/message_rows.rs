//! `message_rows` maps a session's event log to the rows a Lua caller sees.
//! The function is pure, so these tests need no daemon.

use super::super::message_rows;
use crate::observe::LogEvent;
use chrono::Utc;

fn events() -> Vec<LogEvent> {
    vec![
        LogEvent::User {
            ts: Utc::now(),
            content: "run it".into(),
        },
        LogEvent::ToolCall {
            ts: Utc::now(),
            id: "c1".into(),
            name: "bash".into(),
            args: serde_json::json!({ "command": "cargo test" }),
        },
        LogEvent::ToolResult {
            ts: Utc::now(),
            id: "c1".into(),
            result: "ok".into(),
            truncated: false,
            full_size: None,
            error: None,
        },
    ]
}

#[test]
fn tool_events_are_dropped_by_default() {
    let rows = message_rows(&events(), None, false);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["role"], "user");
}

#[test]
fn tool_events_become_rows_when_asked_for() {
    let rows = message_rows(&events(), None, true);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1]["role"], "tool_call");
    assert_eq!(rows[1]["name"], "bash");
    assert_eq!(rows[1]["args"]["command"], "cargo test");
    assert_eq!(rows[2]["role"], "tool_result");
    assert_eq!(rows[2]["id"], "c1");
    assert_eq!(rows[2]["content"], "ok");
}

#[test]
fn a_role_filter_still_excludes_tool_rows() {
    let rows = message_rows(&events(), Some("user"), true);
    assert_eq!(rows.len(), 1);
}
