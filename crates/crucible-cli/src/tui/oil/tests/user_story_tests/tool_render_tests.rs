//! US-306: The tool card and the permission modal draw the render that the
//! daemon sent. The TUI does not rebuild the call from its arguments.

use serde_json::json;

use super::support::StoryRuntime;
use super::vocab::{relay_session_event, send_user_message};
use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

/// A plugin kind: the card shows the canonical tool and the render line, and
/// not a value that the TUI took from the arguments.
#[test]
fn a_tool_card_shows_the_line_that_the_daemon_rendered() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "fix it");
    relay_session_event(
        &mut story,
        "tool_call",
        json!({
            "call_id": "c1",
            "tool": "spawn_agent",
            "args": { "prompt": "from the arguments" },
            "display": {
                "kind": "delegate", "tool": "Task",
                "render": { "line": "fix the parser bug" },
            },
        }),
    );

    let frame = story.fresh_screen();
    assert!(frame.contains("Task fix the parser bug"), "{frame}");
    assert!(!frame.contains("from the arguments"), "{frame}");
}

/// A render that a plugin gives reaches the card: its line, each field on a
/// row, and the summary of the result render. The TUI takes nothing from
/// the tool name.
#[test]
fn a_tool_card_draws_the_fields_and_the_result_summary() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "search it");
    relay_session_event(
        &mut story,
        "tool_call",
        json!({
            "call_id": "c1", "tool": "web_search", "args": { "query": "rust" },
            "display": {
                "kind": "search", "tool": "web_search", "query": "rust",
                "render": { "line": "rust", "fields": [{ "label": "provider", "value": "ddg" }] },
            },
        }),
    );
    relay_session_event(
        &mut story,
        "tool_result",
        json!({
            "call_id": "c1", "tool": "web_search",
            "result": {
                "result": "one\ntwo\nthree",
                "render": {
                    "line": "rust",
                    "fields": [{ "label": "provider", "value": "ddg" }],
                    "summary": "ddg · 3 results",
                },
            },
        }),
    );

    let frame = story.fresh_screen();
    assert!(frame.contains("rust → ddg · 3 results"), "{frame}");
    assert!(frame.contains("provider: ddg"), "{frame}");
    assert!(
        !frame.contains("three"),
        "the summary stands for the output: {frame}"
    );
}

/// A slow call goes to the background. Its finish row still shows the
/// summary of the result render.
#[test]
fn a_background_call_keeps_its_result_summary() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "search it");
    relay_session_event(
        &mut story,
        "tool_call",
        json!({
            "call_id": "c1", "tool": "web_search", "args": { "query": "rust" },
            "display": { "kind": "search", "tool": "web_search", "render": { "line": "rust" } },
        }),
    );
    story.advance(std::time::Duration::from_secs(1));
    assert!(
        story.app().split_slow_tools(),
        "the call goes to the background"
    );
    relay_session_event(
        &mut story,
        "tool_result",
        json!({
            "call_id": "c1", "tool": "web_search",
            "result": { "result": "one\ntwo", "render": { "line": "rust", "summary": "ddg · 2 results" } },
        }),
    );

    let frame = story.fresh_screen();
    assert!(frame.contains("ddg · 2 results"), "{frame}");
}

/// A later update of the call brings a new render, and the card takes its line.
#[test]
fn a_tool_call_update_replaces_the_line() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "edit it");
    relay_session_event(
        &mut story,
        "tool_call",
        json!({
            "call_id": "c1", "tool": "Edit", "args": {},
            "display": { "kind": "tool", "tool": "Edit", "render": { "line": "Edit" } },
        }),
    );
    relay_session_event(
        &mut story,
        "tool_call_update",
        json!({
            "call_id": "c1", "args": { "file_path": "src/a.rs" },
            "display": {
                "kind": "file_edit", "tool": "Edit", "paths": ["src/a.rs"],
                "render": { "line": "src/a.rs" },
            },
        }),
    );

    let frame = story.fresh_screen();
    assert!(frame.contains("Edit src/a.rs"), "{frame}");
}

/// The prompt shows everything that is known: the agent, the tool name on
/// the wire and the layer that asked.
#[test]
fn the_permission_modal_shows_the_agent_the_wire_name_and_the_layer() {
    let mut story = StoryRuntime::new(100, 30);
    send_user_message(&mut story, "edit it");
    let request = json!({
        "kind": "permission",
        "action": { "type": "tool", "name": "file_edit", "args": { "file_path": "src/a.rs" } },
        "call": {
            "kind": "file_edit", "tool": "file_edit", "paths": ["src/a.rs"],
            "agent": "claude",
            "raw": { "name": "Edit", "rawInput": { "file_path": "src/a.rs" } },
            "render": { "line": "src/a.rs" },
        },
        "layer": "ask mode",
    });
    let _ = story.app().open_interaction(
        "req-1".to_string(),
        serde_json::from_value(request).expect("the daemon's request decodes"),
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("agent claude · wire name Edit · asked by ask mode"),
        "{frame}"
    );
}

/// The prompt draws the render line and the render fields, not the
/// arguments of the call.
#[test]
fn the_permission_modal_draws_the_render_line_and_fields() {
    let mut story = StoryRuntime::new(100, 30);
    send_user_message(&mut story, "delegate it");
    let request = json!({
        "kind": "permission",
        "action": { "type": "tool", "name": "spawn", "args": { "prompt": "from the arguments" } },
        "call": {
            "kind": "delegate", "tool": "spawn",
            "render": { "line": "fix the parser", "fields": [{ "label": "agent", "value": "claude" }] },
        },
    });
    let _ = story.app().open_interaction(
        "req-1".to_string(),
        serde_json::from_value(request).expect("the daemon's request decodes"),
    );

    let frame = story.fresh_screen();
    assert!(frame.contains("spawn fix the parser"), "{frame}");
    assert!(frame.contains("agent: claude"), "{frame}");
    assert!(!frame.contains("from the arguments"), "{frame}");
}

/// A transcript from before the render still shows each card line and each
/// diff, and each failed turn shows its error one time. The events go
/// through the migration of the daemon history, as on a resume.
#[test]
fn an_old_transcript_shows_its_lines_and_diffs() {
    let path = crate::tui::oil::tests::helpers::fixture_path("old_wire_session.jsonl");
    let text = std::fs::read_to_string(path).expect("the fixture reads");
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect();
    let msgs: Vec<ChatAppMsg> = crucible_core::protocol::session_events::migrate_history(lines)
        .iter()
        .flat_map(|e| session_event_to_chat_msgs(e["event"].as_str().unwrap_or(""), &e["data"]))
        .collect();
    let errors: Vec<&ChatAppMsg> = msgs
        .iter()
        .filter(|m| matches!(m, ChatAppMsg::Error(_)))
        .collect();
    assert_eq!(
        errors.len(),
        2,
        "one error for each failed turn: {errors:?}"
    );

    let mut story = StoryRuntime::new(100, 60);
    for msg in msgs {
        story.send(msg);
    }
    let frame = story.fresh_screen();
    assert!(frame.contains("lib.rs (from Lua)"), "{frame}");
    assert!(frame.contains("src/main.rs"), "{frame}");
    for diff in ["+b", "+y"] {
        assert!(frame.contains(diff), "the diff {diff} shows: {frame}");
    }
}
