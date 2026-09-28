use super::super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ─── Setup-event translation (Task 1.3) ─────────────────────────────

#[test]
fn translate_session_initialized_produces_payload_msg() {
    use serde_json::json;
    let data = json!({
        "model": "glm-5",
        "mode": "plan",
        "agent_name": "claude",
        "kilns": ["notes"],
        "workspace_path": "/w",
    });
    let msgs = session_event_to_chat_msgs("session_initialized", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::SessionInitialized(p) => {
            assert_eq!(p.model, "glm-5");
            assert_eq!(p.mode, "plan");
            assert_eq!(p.agent_name.as_deref(), Some("claude"));
        }
        other => panic!("expected SessionInitialized, got {other:?}"),
    }
}

#[test]
fn translate_providers_listed_carries_providers() {
    use serde_json::json;
    let data = json!({
        "providers": [{
            "name": "OpenAI", "provider_type": "openai", "available": true,
            "default_model": null, "models": [], "endpoint": null,
            "reason": null, "is_local": false,
        }],
    });
    let msgs = session_event_to_chat_msgs("providers_listed", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::ProvidersListed(providers) => {
            assert_eq!(providers.len(), 1);
            assert_eq!(providers[0].name, "OpenAI");
        }
        other => panic!("expected ProvidersListed, got {other:?}"),
    }
}

#[test]
fn translate_context_limit_resolved_parses_source() {
    use crucible_core::protocol::session_events::ContextLimitSource;
    use serde_json::json;
    let data = json!({ "limit": 128_000, "source": "provider_api" });
    let msgs = session_event_to_chat_msgs("context_limit_resolved", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::ContextLimitResolved { limit, source } => {
            assert_eq!(*limit, 128_000);
            assert_eq!(*source, ContextLimitSource::ProviderApi);
        }
        other => panic!("expected ContextLimitResolved, got {other:?}"),
    }
}

#[test]
fn translate_workspace_indexed_carries_files() {
    use serde_json::json;
    let data = json!({ "files": ["src/lib.rs", "README.md"] });
    let msgs = session_event_to_chat_msgs("workspace_indexed", &data);
    match msgs.as_slice() {
        [ChatAppMsg::WorkspaceIndexed(files)] => assert_eq!(
            files,
            &vec!["src/lib.rs".to_string(), "README.md".to_string()]
        ),
        other => panic!("expected WorkspaceIndexed, got {other:?}"),
    }
}

#[test]
fn translate_kiln_notes_indexed_carries_notes() {
    use serde_json::json;
    let data = json!({ "notes": ["note:Daily.md"] });
    let msgs = session_event_to_chat_msgs("kiln_notes_indexed", &data);
    match msgs.as_slice() {
        [ChatAppMsg::KilnNotesIndexed(notes)] => {
            assert_eq!(notes, &vec!["note:Daily.md".to_string()])
        }
        other => panic!("expected KilnNotesIndexed, got {other:?}"),
    }
}

#[test]
fn translate_plugins_discovered_carries_entries() {
    use serde_json::json;
    let data = json!({
        "plugins": [
            { "name": "kiln-expert", "version": "0.1.0", "state": "loaded", "error": null }
        ]
    });
    let msgs = session_event_to_chat_msgs("plugins_discovered", &data);
    match msgs.as_slice() {
        [ChatAppMsg::PluginsDiscovered(entries)] => {
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].name, "kiln-expert");
            assert_eq!(entries[0].state, "loaded");
        }
        other => panic!("expected PluginsDiscovered, got {other:?}"),
    }
}

#[test]
fn translate_mcp_servers_ready_maps_to_display_and_collapses_tools() {
    use serde_json::json;
    let data = json!({
        "servers": [
            {
                "name": "context7",
                "prefix": "c7_",
                "tools": ["query-docs", "resolve-library-id"],
                "connected": true,
            }
        ]
    });
    let msgs = session_event_to_chat_msgs("mcp_servers_ready", &data);
    match msgs.as_slice() {
        [ChatAppMsg::McpServersReady(servers)] => {
            assert_eq!(servers.len(), 1);
            assert_eq!(servers[0].name, "context7");
            // trailing `_` stripped to match legacy McpServerDisplay shape
            assert_eq!(servers[0].prefix, "c7");
            assert_eq!(servers[0].tool_count, 2);
            assert!(servers[0].connected);
        }
        other => panic!("expected McpServersReady, got {other:?}"),
    }
}

#[test]
fn translate_bad_payload_shape_returns_empty() {
    use serde_json::json;
    // Missing required fields — the type-strict deserializer fails and the
    // translator returns an empty vec rather than panicking.
    let msgs = session_event_to_chat_msgs("context_limit_resolved", &json!({}));
    assert!(msgs.is_empty());
}

#[test]
fn translate_unknown_event_returns_empty() {
    use serde_json::json;
    let msgs = session_event_to_chat_msgs("never_heard_of_it", &json!({}));
    assert!(msgs.is_empty());
}

/// A daemon notification goes to the notification area with its kind.
#[test]
fn translate_a_daemon_notification_keeps_its_kind() {
    let notification = crucible_core::types::Notification::warning("kiln docs failed");
    let data =
        serde_json::json!({ "notification_id": notification.id, "notification": notification });
    let msgs = session_event_to_chat_msgs("notification_added", &data);
    assert!(
        matches!(&msgs[..], [ChatAppMsg::Notification(n)] if *n == notification),
        "{msgs:?}"
    );
}

/// A notification that another client dismissed leaves this one too.
#[test]
fn translate_a_dismissed_notification() {
    let data = serde_json::json!({ "notification_id": "notif-1" });
    let msgs = session_event_to_chat_msgs("notification_dismissed", &data);
    assert!(
        matches!(&msgs[..], [ChatAppMsg::DismissNotification(id)] if id == "notif-1"),
        "{msgs:?}"
    );
}

#[test]
fn translate_context_limit_resolved_updates_atomic_through_stream() {
    use serde_json::json;
    let limit = Arc::new(AtomicUsize::new(0));
    let mut stream = SessionEventStream::new().with_context_limit(limit.clone());
    let msgs = stream.translate(
        "context_limit_resolved",
        &json!({ "limit": 4096, "source": "config" }),
    );
    assert_eq!(msgs.len(), 1);
    assert_eq!(limit.load(Ordering::Relaxed), 4096);
}

// ─── Transcript items, as the TUI draws them ────────────────────────
//
// The daemon folds the events (`crucible_core::transcript`) and the TUI
// draws the items. These tests feed wire events through the fold and the
// runner, as a live session does, and read what the app drew.

use crate::tui::oil::containers::ChatNode;
use crate::tui::oil::tests::helpers::EventFeed;
use crate::tui::oil::viewport_cache::CachedToolCall;

/// The app after `events`, on the live path.
fn drawn(events: &[(&str, serde_json::Value)]) -> OilChatApp {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    for (event, data) in events {
        for msg in feed.msgs(event, data.clone()) {
            app.on_message(msg);
        }
    }
    app
}

/// The first tool card that `events` draw.
fn card(events: &[(&str, serde_json::Value)]) -> CachedToolCall {
    drawn(events)
        .container_list()
        .nodes()
        .iter()
        .find_map(|node| match node {
            ChatNode::ToolGroup { tools } => tools.first().cloned(),
            _ => None,
        })
        .expect("a tool card is drawn")
}

fn edit_call(diffs: serde_json::Value) -> (&'static str, serde_json::Value) {
    (
        "tool_call",
        serde_json::json!({
            "call_id": "tc-1", "tool": "edit_file", "args": { "path": "src/foo.rs" },
            "display": { "kind": "file_edit", "tool": "edit_file", "diffs": diffs },
        }),
    )
}

/// A diff list of the wrong shape loads as no diffs; the card stays.
#[test]
fn a_malformed_diff_list_draws_a_card_without_diffs() {
    assert!(card(&[edit_call(serde_json::json!("not a list"))])
        .diffs
        .is_empty());
}

#[test]
fn the_diffs_of_a_call_reach_its_card() {
    let tool = card(&[edit_call(serde_json::json!([
        { "path": "src/foo.rs", "old_content": "old", "new_content": "new" }
    ]))]);
    assert_eq!(tool.diffs.len(), 1);
    assert_eq!(tool.diffs[0].path, "src/foo.rs");
}

/// The card shows the canonical tool name of the call, not a second name
/// that the event carries beside it.
#[test]
fn a_card_takes_the_canonical_tool_name() {
    let tool = card(&[(
        "tool_call",
        serde_json::json!({
            "call_id": "call-1", "tool": "Edit src/foo.rs", "args": {},
            "display": { "kind": "file_edit", "tool": "Edit", "paths": ["src/foo.rs"] },
        }),
    )]);
    assert_eq!(tool.name.as_ref(), "Edit");
}

/// ACP agents first announce an empty call, then send its arguments, diff
/// and render in a `tool_call_update`. The card takes all three.
#[test]
fn a_late_update_fills_the_args_diffs_and_render_of_the_card() {
    let tool = card(&[
        (
            "tool_call",
            serde_json::json!({ "call_id": "tc-late", "tool": "Edit", "args": {} }),
        ),
        (
            "tool_call_update",
            serde_json::json!({
                "call_id": "tc-late", "args": { "file_path": "src/late.rs" },
                "display": {
                    "kind": "file_edit", "tool": "Edit",
                    "diffs": [{ "path": "src/late.rs", "old_content": "a", "new_content": "b" }],
                    "render": { "line": "src/late.rs" },
                },
            }),
        ),
    ]);
    assert_eq!(tool.args.as_ref(), r#"{"file_path":"src/late.rs"}"#);
    assert_eq!(tool.diffs.len(), 1);
    assert_eq!(tool.render.as_deref(), Some(&"src/late.rs".into()));
}

/// An update with nothing in it keeps the arguments of the card.
#[test]
fn an_empty_update_keeps_the_args_of_the_card() {
    for args in [serde_json::json!({}), serde_json::json!(null)] {
        let tool = card(&[
            (
                "tool_call",
                serde_json::json!({ "call_id": "c", "tool": "bash", "args": { "command": "ls" } }),
            ),
            (
                "tool_call_update",
                serde_json::json!({ "call_id": "c", "args": args }),
            ),
        ]);
        assert_eq!(tool.args.as_ref(), r#"{"command":"ls"}"#);
    }
}

/// A transcript from before `tool_call_update` holds
/// `tool_call_args_update` lines with args only. The card still gets its
/// args, and keeps its diffs.
#[test]
fn an_old_args_update_line_still_fills_the_card() {
    let tool = card(&[
        edit_call(serde_json::json!([
            { "path": "src/foo.rs", "old_content": "old", "new_content": "new" }
        ])),
        (
            "tool_call_args_update",
            serde_json::json!({ "call_id": "tc-1", "args": { "path": "Concepts/Target.md" } }),
        ),
    ]);
    assert_eq!(tool.args.as_ref(), r#"{"path":"Concepts/Target.md"}"#);
    assert_eq!(tool.diffs.len(), 1, "an old line says nothing about diffs");
}

/// The render of a finished call replaces the render of the card, and a
/// structured result shows as its JSON.
#[test]
fn a_result_gives_the_card_its_render_and_its_output() {
    let tool = card(&[
        (
            "tool_call",
            serde_json::json!({
                "call_id": "c1", "tool": "read_file", "args": {},
                "display": { "kind": "file_read", "tool": "read_file", "render": { "line": "a.rs" } },
            }),
        ),
        (
            "tool_result",
            serde_json::json!({
                "call_id": "c1", "tool": "read_file",
                "result": { "result": { "exit_code": 0 }, "render": { "line": "a.rs", "summary": "2 lines" } },
            }),
        ),
    ]);
    assert!(tool.complete);
    assert_eq!(
        tool.render.as_ref().and_then(|r| r.summary.as_deref()),
        Some("2 lines")
    );
    assert_eq!(tool.result(), r#"{"exit_code":0}"#);
}

/// The auto-approval marker rides on `tool_call`, so the badge draws with
/// the row.
#[test]
fn a_card_carries_its_auto_approval_reason_or_none() {
    let approved = card(&[(
        "tool_call",
        serde_json::json!({ "call_id": "c1", "tool": "bash", "args": {}, "auto_approved": "auto mode" }),
    )]);
    assert_eq!(approved.auto_approved.as_deref(), Some("auto mode"));
    let asked = card(&[(
        "tool_call",
        serde_json::json!({ "call_id": "c1", "tool": "bash", "args": {} }),
    )]);
    assert_eq!(asked.auto_approved, None);
}

/// A reply that the provider cut off draws a note after its bubble; a reply
/// that finished, or one from a daemon too old to name a reason, draws none.
#[test]
fn a_truncated_reply_draws_a_note_after_the_bubble() {
    let reply = |data: serde_json::Value| {
        let app = drawn(&[("message_complete", data)]);
        app.container_list()
            .nodes()
            .iter()
            .map(|node| match node {
                ChatNode::AssistantResponse { text, .. } => format!("reply:{text}"),
                ChatNode::SystemMessage { text } => format!("note:{text}"),
                _ => "other".to_string(),
            })
            .collect::<Vec<_>>()
    };
    let cut =
        reply(serde_json::json!({ "full_response": "Half an ans", "stop_reason": "max_tokens" }));
    assert_eq!(cut.len(), 2, "{cut:?}");
    assert_eq!(cut[0], "reply:Half an ans");
    assert!(
        cut[1].starts_with("note:") && cut[1].contains("output limit"),
        "{cut:?}"
    );

    for data in [
        serde_json::json!({ "full_response": "done", "stop_reason": "end_turn" }),
        serde_json::json!({ "full_response": "done" }),
    ] {
        assert_eq!(reply(data), ["reply:done"]);
    }
}

/// The text of each node that `events` draw.
fn drawn_texts(events: &[(&str, serde_json::Value)]) -> Vec<String> {
    drawn(events)
        .container_list()
        .nodes()
        .iter()
        .filter_map(|node| match node {
            ChatNode::UserMessage { text } => Some(format!("user:{text}")),
            ChatNode::SystemMessage { text } => Some(format!("system:{text}")),
            _ => None,
        })
        .collect()
}

#[test]
fn a_plugin_clear_names_the_plugin_in_the_transcript() {
    assert_eq!(
        drawn_texts(&[("context_cleared", serde_json::json!({ "plugin": "alpha" }))]),
        ["system:── ↻ alpha cleared the context ──"]
    );
}

/// A relayed message is a person's words, so it stays a user message, and
/// it names the plugin that relayed it.
#[test]
fn a_relayed_message_is_a_user_message_that_names_its_relay() {
    assert_eq!(
        drawn_texts(&[(
            "user_message",
            serde_json::json!({
                "message_id": "m1", "content": "hi", "origin": { "kind": "relay", "name": "discord" }
            }),
        )]),
        ["user:via discord\nhi"]
    );
}

#[test]
fn a_plugin_turn_is_a_labelled_system_row_in_the_tui() {
    assert_eq!(
        drawn_texts(&[(
            "user_message",
            serde_json::json!({
                "message_id": "m2", "content": "continue with the detailed plan",
                "origin": { "kind": "plugin", "name": "alpha" }
            }),
        )]),
        ["system:↻ alpha\ncontinue with the detailed plan"]
    );
}
