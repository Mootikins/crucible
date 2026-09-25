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
fn translate_tool_call_with_malformed_diffs_yields_empty_diffs() {
    use serde_json::json;
    // Wire-protocol drift safety: if the daemon sends a `diffs` field that
    // isn't a Vec<FileDiff>, the translator must log a warning and emit
    // an empty Vec rather than panic or drop the entire ToolCall message.
    let data = json!({
        "call_id": "tc-1",
        "tool": "edit_file",
        "args": {},
        "display": {"kind": "file_edit", "tool": "edit_file", "diffs": "this is not a list"},
    });
    let msgs = session_event_to_chat_msgs("tool_call", &data);
    match msgs.as_slice() {
        [ChatAppMsg::ToolCall { diffs, .. }] => assert!(diffs.is_empty()),
        other => panic!("expected single ToolCall, got {other:?}"),
    }
}

#[test]
fn translate_tool_call_with_well_formed_diffs_passes_through() {
    use serde_json::json;
    let data = json!({
        "call_id": "tc-1",
        "tool": "edit_file",
        "args": {},
        "display": {"kind": "file_edit", "tool": "edit_file", "diffs": [{
            "path": "/tmp/foo.rs",
            "old_content": "old",
            "new_content": "new"
        }]},
    });
    let msgs = session_event_to_chat_msgs("tool_call", &data);
    match msgs.as_slice() {
        [ChatAppMsg::ToolCall { diffs, .. }] => {
            assert_eq!(diffs.len(), 1);
            assert_eq!(diffs[0].path, "/tmp/foo.rs");
        }
        other => panic!("expected single ToolCall, got {other:?}"),
    }
}

#[test]
fn translate_unknown_event_returns_empty() {
    use serde_json::json;
    let msgs = session_event_to_chat_msgs("never_heard_of_it", &json!({}));
    assert!(msgs.is_empty());
}

#[test]
fn translate_tool_call_propagates_diffs_into_chat_msg() {
    use crucible_core::types::acp::FileDiff;
    use serde_json::json;

    // Build a payload as the daemon emits via tool_call_with_metadata: the
    // diffs ride in the canonical call.
    let diffs_in = vec![FileDiff::from_contents(
        "src/foo.rs",
        Some("fn old() {}\n".to_string()),
        "fn new() {}\n",
    )];
    let data = json!({
        "call_id": "call-1",
        "tool": "edit",
        "args": { "path": "src/foo.rs" },
        "display": { "kind": "file_edit", "tool": "edit", "diffs": diffs_in },
    });

    let msgs = session_event_to_chat_msgs("tool_call", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::ToolCall { diffs, .. } => {
            assert_eq!(diffs, &diffs_in, "diffs must propagate end-to-end");
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

/// The card shows the canonical tool name of the call, not a second name
/// that the event carries beside it.
#[test]
fn translate_tool_call_names_the_card_by_the_canonical_tool() {
    use serde_json::json;
    let data = json!({
        "call_id": "call-1",
        "tool": "Edit src/foo.rs",
        "args": { "file_path": "src/foo.rs" },
        "display": { "kind": "file_edit", "tool": "Edit", "paths": ["src/foo.rs"] },
    });
    match session_event_to_chat_msgs("tool_call", &data).as_slice() {
        [ChatAppMsg::ToolCall { name, .. }] => assert_eq!(name, "Edit"),
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn translate_tool_call_without_diffs_yields_empty_vec() {
    use serde_json::json;
    let data = json!({
        "call_id": "call-1",
        "tool": "read_file",
        "args": { "path": "/tmp/x" },
    });
    let msgs = session_event_to_chat_msgs("tool_call", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::ToolCall { diffs, .. } => {
            assert!(
                diffs.is_empty(),
                "missing diffs key must yield empty Vec, got {diffs:?}"
            );
        }
        other => panic!("expected ToolCall, got {other:?}"),
    }
}

#[test]
fn translate_tool_call_update_emits_chat_msg_with_args_and_diffs() {
    use crucible_core::types::acp::FileDiff;
    use serde_json::json;

    // Late path: ACP agents like Claude Code first send an empty tool_call,
    // then attach the arguments and the diff via a follow-up
    // tool_call_update. The daemon sends the new canonical call in a
    // `tool_call_update` event; the TUI must produce a
    // `ChatAppMsg::ToolCallUpdate` so the existing card takes both.
    let diffs_in = vec![FileDiff::from_contents(
        "src/late.rs",
        Some("fn old() {}\n".to_string()),
        "fn new() {}\n",
    )];
    let data = json!({
        "call_id": "tc-late-1",
        "args": {"file_path": "src/late.rs"},
        "display": {
            "kind": "file_edit", "tool": "Edit", "diffs": diffs_in,
            "render": { "line": "src/late.rs" },
        },
    });

    let msgs = session_event_to_chat_msgs("tool_call_update", &data);
    assert_eq!(msgs.len(), 1);
    match &msgs[0] {
        ChatAppMsg::ToolCallUpdate {
            call_id,
            args,
            diffs,
            render,
            ..
        } => {
            assert_eq!(call_id, "tc-late-1");
            assert_eq!(
                render.as_ref(),
                Some(&"src/late.rs".into()),
                "the new render"
            );
            assert_eq!(args.as_deref(), Some(r#"{"file_path":"src/late.rs"}"#));
            assert_eq!(
                diffs.as_ref(),
                Some(&diffs_in),
                "diffs must propagate end-to-end"
            );
        }
        other => panic!("expected ToolCallUpdate, got {other:?}"),
    }
}

/// The render of a finished call rides the result and replaces the render
/// of the card, before the result completes the card.
#[test]
fn translate_tool_result_carries_the_render_of_the_result() {
    let data = serde_json::json!({
        "call_id": "c1", "tool": "read_file",
        "result": { "result": "a\nb", "render": { "line": "a.rs", "summary": "2 lines" } },
    });
    let msgs = session_event_to_chat_msgs("tool_result", &data);
    let render = crucible_core::types::ToolRender {
        summary: Some("2 lines".into()),
        .."a.rs".into()
    };
    assert!(
        matches!(&msgs[..], [
            ChatAppMsg::ToolCallUpdate { call_id, render: Some(r), args: None, diffs: None, .. },
            ChatAppMsg::ToolResultDelta { .. },
            ChatAppMsg::ToolResultComplete { .. },
        ] if call_id == "c1" && *r == render),
        "{msgs:?}"
    );
}

/// A structured result is not a string. The card shows its JSON, not nothing.
#[test]
fn translate_tool_result_shows_a_structured_result() {
    let data = serde_json::json!({
        "call_id": "c1", "tool": "Bash",
        "result": { "result": { "exit_code": 0 } },
    });
    let msgs = session_event_to_chat_msgs("tool_result", &data);
    assert!(
        matches!(&msgs[..], [ChatAppMsg::ToolResultDelta { delta, .. }, ChatAppMsg::ToolResultComplete { .. }]
            if delta == r#"{"exit_code":0}"#),
        "{msgs:?}"
    );
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
fn translate_tool_call_update_with_nothing_in_it_drops_msg() {
    use serde_json::json;
    // No args and no canonical call → no need to disturb the TUI scrollback.
    for args in [json!({}), json!(null)] {
        let data = json!({ "call_id": "tc-noop", "args": args });
        let msgs = session_event_to_chat_msgs("tool_call_update", &data);
        assert!(
            msgs.is_empty(),
            "an empty update should not emit a ChatAppMsg, got {msgs:?}"
        );
    }
}

/// A malformed `diffs` in the canonical call does not drop the update: the
/// call loads with no diff (`lenient_diffs`).
#[test]
fn translate_tool_call_update_with_malformed_diffs_keeps_the_update() {
    use serde_json::json;
    let data = json!({
        "call_id": "tc-bad",
        "display": { "kind": "file_edit", "tool": "Edit", "diffs": "not a list" },
    });
    match session_event_to_chat_msgs("tool_call_update", &data).as_slice() {
        [ChatAppMsg::ToolCallUpdate { diffs, .. }] => {
            assert_eq!(diffs.as_deref(), Some(&[][..]));
        }
        other => panic!("expected ToolCallUpdate, got {other:?}"),
    }
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

/// The auto-approval marker rides on `tool_call` rather than arriving as a
/// follow-up event. The gate decides before this event is emitted, so a
/// separate event would only make the badge appear a beat after the row.
#[test]
fn tool_call_carries_the_auto_approval_reason() {
    let data = serde_json::json!({
        "call_id": "c1",
        "tool": "bash",
        "args": {"command": "ls"},
        "auto_approved": "auto mode",
    });
    let msgs = session_event_to_chat_msgs("tool_call", &data);
    match msgs.as_slice() {
        [ChatAppMsg::ToolCall { auto_approved, .. }] => {
            assert_eq!(auto_approved.as_deref(), Some("auto mode"));
        }
        other => panic!("expected single ToolCall, got {other:?}"),
    }
}

#[test]
fn tool_call_without_auto_approval_carries_none() {
    let data = serde_json::json!({
        "call_id": "c1",
        "tool": "bash",
        "args": {"command": "ls"},
    });
    let msgs = session_event_to_chat_msgs("tool_call", &data);
    match msgs.as_slice() {
        [ChatAppMsg::ToolCall { auto_approved, .. }] => assert_eq!(*auto_approved, None),
        other => panic!("expected single ToolCall, got {other:?}"),
    }
}

/// A transcript from before `tool_call_update` holds
/// `tool_call_args_update` lines with args only. The card still gets its
/// args, and keeps its diffs.
#[test]
fn an_old_args_update_line_still_fills_the_card() {
    use serde_json::json;
    let data = json!({
        "call_id": "tc-late-args",
        "args": {"path": "Concepts/Target.md"},
    });

    let msgs = session_event_to_chat_msgs("tool_call_args_update", &data);
    assert_eq!(msgs.len(), 1, "got {msgs:?}");
    match &msgs[0] {
        ChatAppMsg::ToolCallUpdate {
            call_id,
            args,
            diffs,
            ..
        } => {
            assert_eq!(call_id, "tc-late-args");
            assert_eq!(args.as_deref(), Some(r#"{"path":"Concepts/Target.md"}"#));
            assert_eq!(diffs, &None, "an old line says nothing about diffs");
        }
        other => panic!("expected ToolCallUpdate, got {other:?}"),
    }
}

/// A reply the provider cut off draws a note in the transcript.
///
/// The daemon names the reason; the TUI is where a user meets it. Tested from
/// the WIRE payload, because a front end fed unfamiliar data is where this
/// breaks while the producing side's tests all still pass.
#[test]
fn a_truncated_reply_draws_a_note_after_the_bubble() {
    use serde_json::json;
    let data = json!({
        "message_id": "msg-1",
        "full_response": "Half an ans",
        "stop_reason": "max_tokens",
    });

    let msgs = session_event_to_chat_msgs("message_complete", &data);

    // The note comes AFTER the completion: the completion seals the assistant
    // bubble only while that bubble is the last node.
    let complete = msgs
        .iter()
        .position(|m| matches!(m, ChatAppMsg::StreamComplete))
        .expect("the stream still completes");
    let notice = msgs
        .iter()
        .position(|m| matches!(m, ChatAppMsg::SystemNotice(_)))
        .expect("a truncated reply is announced");
    assert!(notice > complete, "got {msgs:?}");
    match &msgs[notice] {
        ChatAppMsg::SystemNotice(text) => assert!(text.contains("output limit"), "{text}"),
        other => panic!("expected SystemNotice, got {other:?}"),
    }
}

/// A reply that finished, or one from a daemon too old to name a reason,
/// mints no notice.
#[test]
fn a_finished_reply_mints_no_notice() {
    use serde_json::json;
    for data in [
        json!({"message_id": "m", "full_response": "done", "stop_reason": "end_turn"}),
        json!({"message_id": "m", "full_response": "done"}),
    ] {
        let msgs = session_event_to_chat_msgs("message_complete", &data);
        assert!(
            !msgs
                .iter()
                .any(|m| matches!(m, ChatAppMsg::SystemNotice(_))),
            "got {msgs:?} for {data}"
        );
    }
}
