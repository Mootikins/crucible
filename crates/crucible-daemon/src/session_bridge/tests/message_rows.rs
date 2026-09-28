//! `message_rows` maps a session's transcript to the rows a Lua caller sees.
//! The function is pure, so these tests need no daemon.

use super::super::message_rows;
use crucible_core::protocol::SessionEventMessage;
use crucible_core::transcript::{Transcript, TranscriptFold};
use serde_json::json;

fn transcript(events: &[(&str, serde_json::Value)]) -> Transcript {
    let events: Vec<SessionEventMessage> = events
        .iter()
        .map(|(name, data)| SessionEventMessage::new("s", *name, data.clone()))
        .collect();
    TranscriptFold::of_events(&events)
}

fn tool_turn() -> Transcript {
    transcript(&[
        (
            "user_message",
            json!({"message_id": "t1", "content": "run it"}),
        ),
        (
            "tool_call",
            json!({"call_id": "c1", "tool": "bash", "args": {"command": "cargo test"}}),
        ),
        (
            "tool_result",
            json!({"call_id": "c1", "tool": "bash", "result": {"result": "ok"}}),
        ),
    ])
}

#[test]
fn tool_events_are_dropped_by_default() {
    let rows = message_rows(&tool_turn(), None, false);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["role"], "user");
}

#[test]
fn tool_events_become_rows_when_asked_for() {
    let rows = message_rows(&tool_turn(), None, true);
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
    let rows = message_rows(&tool_turn(), Some("user"), true);
    assert_eq!(rows.len(), 1);
}

/// mlua turns a JSON `null` into a truthy `null` userdata. A Lua caller who
/// writes `if row.error then` must see no `error` key on a good result, and
/// no `timestamp` key on an item without a time.
#[test]
fn a_tool_result_row_carries_error_only_when_the_tool_failed() {
    let transcript = transcript(&[
        ("user_message", json!({"message_id": "t1", "content": "go"})),
        (
            "tool_call",
            json!({"call_id": "c1", "tool": "bash", "args": {}}),
        ),
        (
            "tool_result",
            json!({"call_id": "c1", "tool": "bash", "result": {"error": "exit status 1"}}),
        ),
        (
            "tool_call",
            json!({"call_id": "c2", "tool": "bash", "args": {}}),
        ),
        (
            "tool_result",
            json!({"call_id": "c2", "tool": "bash", "result": {"result": "ok"}}),
        ),
    ]);
    let rows = message_rows(&transcript, None, true);
    let results: Vec<_> = rows.iter().filter(|r| r["role"] == "tool_result").collect();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["error"], "exit status 1");
    assert!(
        results[1].get("error").is_none(),
        "a good result must not carry an error key: {}",
        results[1]
    );
    assert!(rows.iter().all(|r| r.get("timestamp").is_none()));
}

/// A plugin turn and plugin context are `system` rows that name the plugin.
#[test]
fn a_plugin_turn_is_a_system_row_with_its_plugin() {
    let transcript = transcript(&[
        (
            "user_message",
            json!({"message_id": "t1", "content": "keep going", "origin": {"kind": "plugin", "name": "goal"}}),
        ),
        (
            "context_injected",
            json!({"role": "user", "content": "a note", "kind": "plugin", "source": "goal"}),
        ),
    ]);
    let rows = message_rows(&transcript, Some("system"), false);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.iter().all(|r| r["plugin"] == "goal"), "{rows:?}");
    assert!(message_rows(&transcript, Some("user"), false).is_empty());
}
