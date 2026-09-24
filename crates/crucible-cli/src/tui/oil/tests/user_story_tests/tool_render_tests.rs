//! US-306: The tool card and the permission modal draw the render that the
//! daemon sent. The TUI does not rebuild the call from its arguments.

use serde_json::json;

use super::support::StoryRuntime;
use super::vocab::{relay_session_event, send_user_message};

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
