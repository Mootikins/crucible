//! Contract tests for the Lua event table a handler receives.
//!
//! These pin the exact key names. Drift here used to be free — the shape fell
//! out of `#[derive(Serialize)]` on `SessionEvent`, so `docs/Help/Extending/
//! Event Hooks.md` and every shipped plugin disagreed with the dispatcher and
//! nothing failed. If you change the shape, these tests are the thing that
//! makes you update the docs too.

use crate::handlers::conversion::{session_event_to_flat_json, session_event_to_lua};
use crucible_core::events::SessionEvent;
use mlua::{Lua, LuaSerdeExt, Table};

fn pre_tool_call_event() -> SessionEvent {
    SessionEvent::Custom {
        name: "pre_tool_call".to_string(),
        payload: serde_json::json!({
            "tool": "bash",
            "args": { "command": "ls -la" },
        }),
    }
}

#[test]
fn custom_event_exposes_payload_keys_at_top_level() {
    let lua = Lua::new();
    let table = session_event_to_lua(&lua, &pre_tool_call_event()).unwrap();

    // The shape plugins and docs actually use.
    assert_eq!(table.get::<String>("tool").unwrap(), "bash");
    let args: Table = table.get("args").unwrap();
    assert_eq!(args.get::<String>("command").unwrap(), "ls -la");
}

#[test]
fn custom_event_type_is_the_event_name_not_the_rust_variant() {
    let lua = Lua::new();
    let table = session_event_to_lua(&lua, &pre_tool_call_event()).unwrap();

    // Previously "Custom" — the Rust enum variant leaking through serde.
    // A handler asking what happened wants "pre_tool_call".
    assert_eq!(table.get::<String>("type").unwrap(), "pre_tool_call");
}

#[test]
fn custom_event_drops_the_payload_envelope() {
    let lua = Lua::new();
    let table = session_event_to_lua(&lua, &pre_tool_call_event()).unwrap();

    assert!(
        table.get::<mlua::Value>("payload").unwrap().is_nil(),
        "`payload` was an artifact of SessionEvent::Custom's serde shape, not a \
         designed part of the handler API"
    );
    assert!(
        table.get::<mlua::Value>("name").unwrap().is_nil(),
        "`name` meant the event type in code and the tool name in the docs — \
         that ambiguity is why it's gone"
    );
}

#[test]
fn envelope_keys_win_over_colliding_payload_keys() {
    let lua = Lua::new();
    let event = SessionEvent::Custom {
        name: "weird_event".to_string(),
        payload: serde_json::json!({ "type": "impostor", "tool": "bash" }),
    };
    let table = session_event_to_lua(&lua, &event).unwrap();

    // `event.type` has to stay trustworthy even if a payload carries the key.
    assert_eq!(table.get::<String>("type").unwrap(), "weird_event");
    assert_eq!(table.get::<String>("tool").unwrap(), "bash");
}

#[test]
fn non_custom_events_keep_their_flattened_fields() {
    let lua = Lua::new();
    let event = SessionEvent::ToolCalled {
        name: "read".to_string(),
        args: serde_json::json!({ "path": "a.md" }),
        description: None,
        source: None,
    };
    let table = session_event_to_lua(&lua, &event).unwrap();

    assert_eq!(table.get::<String>("type").unwrap(), "ToolCalled");
    assert_eq!(table.get::<String>("name").unwrap(), "read");
}

fn sample_events() -> Vec<SessionEvent> {
    vec![
        pre_tool_call_event(),
        SessionEvent::ToolCalled {
            name: "read".to_string(),
            args: serde_json::json!({ "path": "a.md" }),
            description: None,
            source: None,
        },
        SessionEvent::MessageReceived {
            content: "hello".to_string(),
            participant_id: "user".to_string(),
        },
        SessionEvent::AgentThinking {
            thought: "hmm".to_string(),
        },
    ]
}

/// Both live handler paths — script handlers (`crucible.on` via
/// `session_event_to_lua`) and file handlers (`core_handler` via
/// `session_event_to_flat_json`) — must present ONE contract. They used to
/// disagree for every non-Custom variant: `"ToolCalled"` on one path,
/// `"tool_called"` and no `event_type`/`summary` on the other.
#[test]
fn script_and_file_handler_paths_agree_for_every_variant() {
    let lua = Lua::new();
    for event in sample_events() {
        let flat = session_event_to_flat_json(&event);
        let table = session_event_to_lua(&lua, &event).unwrap();
        let table_json: serde_json::Value = lua.from_value(mlua::Value::Table(table)).unwrap();
        assert_eq!(
            table_json,
            flat,
            "the two conversion paths diverged for {}",
            event.type_name()
        );
    }
}
