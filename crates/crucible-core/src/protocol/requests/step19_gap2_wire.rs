//! Wire-compatibility proof for step 19 gap 2.
//!
//! Each `wire` call below names the JSON the *pre-change* code built with
//! `json!` for one RPC method (transcribed from the `crucible-daemon` source
//! before this change gave the method a named reply type — see the
//! Simplification Plan, step 19). It checks two things at once:
//!
//! 1. The new type, filled with the same values, serializes to that exact
//!    JSON — so the wire did not move.
//! 2. That JSON still deserializes into the new type — so a client library
//!    or a stored fixture built against the old shape still reads.
//!
//! A change here on purpose (the wire moving) means editing the literal by
//! hand in the same commit as the code change, and saying why.

use super::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

fn wire<T: Serialize + DeserializeOwned>(value: T, expected: Value) {
    let actual = serde_json::to_value(&value).expect("the reply serializes");
    assert_eq!(
        actual, expected,
        "the reply JSON moved from its pre-change shape"
    );
    let read: T = serde_json::from_value(expected).expect("the pre-change JSON still reads");
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        actual,
        "the pre-change JSON does not round-trip"
    );
}

#[test]
fn session_pause_and_resume() {
    wire(
        SessionTransitionReply {
            session_id: "chat-1".to_string(),
            previous_state: "Active".to_string(),
            state: "paused".to_string(),
        },
        json!({"session_id": "chat-1", "previous_state": "Active", "state": "paused"}),
    );
    wire(
        SessionTransitionReply {
            session_id: "chat-1".to_string(),
            previous_state: "Paused".to_string(),
            state: "active".to_string(),
        },
        json!({"session_id": "chat-1", "previous_state": "Paused", "state": "active"}),
    );
}

#[test]
fn session_history_and_resume_from_storage() {
    wire(
        SessionHistoryReply {
            session_id: crate::session::SessionId::parse("chat-1").unwrap(),
            session_type: "chat".to_string(),
            state: "Active".to_string(),
            kilns: vec![crate::config::KilnName::parse("docs").unwrap()],
            history: vec![json!({"event": "turn_started"})],
            total_events: 1,
            transcript: crate::transcript::Transcript::default(),
        },
        json!({
            "session_id": "chat-1",
            "type": "chat",
            "state": "Active",
            "kilns": ["docs"],
            "history": [{"event": "turn_started"}],
            "total_events": 1,
            "transcript": {"as_of_seq": 0, "items": []},
        }),
    );
}

#[test]
fn session_end() {
    wire(
        SessionEndReply {
            session_id: crate::session::SessionId::parse("chat-1").unwrap(),
            state: "ended".to_string(),
            kilns: vec![crate::config::KilnName::parse("docs").unwrap()],
        },
        json!({"session_id": "chat-1", "state": "ended", "kilns": ["docs"]}),
    );
}

#[test]
fn session_delete() {
    wire(
        SessionDeleteReply {
            session_id: crate::session::SessionId::parse("chat-1").unwrap(),
            deleted: true,
        },
        json!({"session_id": "chat-1", "deleted": true}),
    );
}

#[test]
fn session_archive_and_unarchive() {
    wire(
        SessionArchiveReply {
            session_id: crate::session::SessionId::parse("chat-1").unwrap(),
            archived: true,
        },
        json!({"session_id": "chat-1", "archived": true}),
    );
}

#[test]
fn session_replay() {
    wire(
        SessionReplayStartedReply {
            session_id: "replay-abc".to_string(),
            status: "replaying".to_string(),
            speed: 1.0,
        },
        json!({"session_id": "replay-abc", "status": "replaying", "speed": 1.0}),
    );
}

#[test]
fn session_compact() {
    wire(
        SessionCompactReply {
            session_id: crate::session::SessionId::parse("chat-1").unwrap(),
            state: "Active".to_string(),
            compaction_requested: true,
        },
        json!({"session_id": "chat-1", "state": "Active", "compaction_requested": true}),
    );
}

#[test]
fn session_subscribe_and_unsubscribe() {
    wire(
        SessionSubscribeReply {
            subscribed: vec!["chat-1".to_string()],
            client_id: "ClientId(1)".to_string(),
        },
        json!({"subscribed": ["chat-1"], "client_id": "ClientId(1)"}),
    );
    wire(
        SessionUnsubscribeReply {
            unsubscribed: vec!["chat-1".to_string()],
            client_id: "ClientId(1)".to_string(),
        },
        json!({"unsubscribed": ["chat-1"], "client_id": "ClientId(1)"}),
    );
}

#[test]
fn session_set_title_and_generate_title() {
    wire(
        SessionTitleReply {
            session_id: "chat-1".to_string(),
            title: "Fix the parser".to_string(),
        },
        json!({"session_id": "chat-1", "title": "Fix the parser"}),
    );
}

#[test]
fn session_configure_agent() {
    wire(
        SessionConfigureAgentReply {
            session_id: "chat-1".to_string(),
            configured: true,
        },
        json!({"session_id": "chat-1", "configured": true}),
    );
}

#[test]
fn session_inject_context() {
    wire(
        SessionInjectContextReply {
            status: "ok".to_string(),
        },
        json!({"status": "ok"}),
    );
}

#[test]
fn session_clear() {
    wire(
        SessionClearReply {
            session_id: "chat-1".to_string(),
        },
        json!({"session_id": "chat-1"}),
    );
}

#[test]
fn session_connect_disconnect_and_set_workspace_scope() {
    wire(
        SessionScopeReply {
            session_id: crate::session::SessionId::parse("chat-1").unwrap(),
            kilns: vec![crate::config::KilnName::parse("docs").unwrap()],
            workspace: Some("/work/space".to_string()),
        },
        json!({"session_id": "chat-1", "kilns": ["docs"], "workspace": "/work/space"}),
    );
}

#[test]
fn session_list_models() {
    wire(
        SessionListModelsReply {
            session_id: "chat-1".to_string(),
            models: vec!["ollama/llama3.2".to_string()],
        },
        json!({"session_id": "chat-1", "models": ["ollama/llama3.2"]}),
    );
}

#[test]
fn session_commands() {
    wire(
        SessionCommandsReply {
            session_id: "chat-1".to_string(),
            commands: Vec::new(),
        },
        json!({"session_id": "chat-1", "commands": []}),
    );
}

#[test]
fn session_list_agent_options() {
    wire(
        SessionListAgentOptionsReply {
            session_id: "chat-1".to_string(),
            options: Vec::new(),
        },
        json!({"session_id": "chat-1", "options": []}),
    );
}

#[test]
fn session_knob_set() {
    wire(
        SessionKnobSetReply {
            session_id: "chat-1".to_string(),
            knob: "model".to_string(),
            set: true,
        },
        json!({"session_id": "chat-1", "knob": "model", "set": true}),
    );
}

#[test]
fn session_add_list_and_dismiss_notification() {
    wire(
        SessionAddNotificationReply {
            session_id: "chat-1".to_string(),
            success: true,
        },
        json!({"session_id": "chat-1", "success": true}),
    );
    wire(
        SessionListNotificationsReply {
            session_id: "chat-1".to_string(),
            notifications: Vec::new(),
        },
        json!({"session_id": "chat-1", "notifications": []}),
    );
    wire(
        SessionDismissNotificationReply {
            session_id: "chat-1".to_string(),
            notification_id: "n-1".to_string(),
            success: true,
        },
        json!({"session_id": "chat-1", "notification_id": "n-1", "success": true}),
    );
}

#[test]
fn session_pending_interactions_and_respond() {
    wire(
        SessionPendingInteractionsReply {
            pending: vec![PendingInteraction {
                session_id: "chat-1".to_string(),
                request_id: "req-1".to_string(),
                request: crate::interaction::InteractionRequest::Ask(
                    crate::interaction::AskRequest {
                        question: "Which?".to_string(),
                        choices: None,
                        allow_other: false,
                        multi_select: false,
                    },
                ),
            }],
        },
        json!({"pending": [{
            "session_id": "chat-1",
            "request_id": "req-1",
            "request": {
                "kind": "ask",
                "question": "Which?",
                "allow_other": false,
                "multi_select": false,
            },
        }]}),
    );
    wire(
        SessionInteractionRespondReply {
            session_id: "chat-1".to_string(),
            request_id: "req-1".to_string(),
        },
        json!({"session_id": "chat-1", "request_id": "req-1"}),
    );
}

#[test]
fn session_plugin_approval() {
    wire(
        PluginApprovalReply {
            plugin: "oci".to_string(),
            approval: "ask".to_string(),
        },
        json!({"plugin": "oci", "approval": "ask"}),
    );
    wire(
        SessionListPluginApprovalsReply {
            approvals: std::collections::BTreeMap::from([(
                "oci".to_string(),
                crate::session::PluginApproval::Ask,
            )]),
        },
        json!({"approvals": {"oci": "ask"}}),
    );
}

#[test]
fn session_test_interaction() {
    wire(
        SessionTestInteractionReply {
            session_id: "chat-1".to_string(),
            request_id: "test-abc".to_string(),
            interaction_type: "ask".to_string(),
        },
        json!({"session_id": "chat-1", "request_id": "test-abc", "type": "ask"}),
    );
}

#[test]
fn session_fork() {
    wire(
        SessionForkReply {
            id: crate::session::SessionId::parse("chat-2").unwrap(),
            parent_id: "chat-1".to_string(),
            messages_copied: 3,
        },
        json!({"id": "chat-2", "parent_id": "chat-1", "messages_copied": 3}),
    );
}

#[test]
fn session_cache_stats() {
    wire(
        SessionCacheStatsReply {
            session_id: "chat-1".to_string(),
            hits: 1,
            misses: 2,
            read_tokens: 100,
            creation_tokens: 200,
            prompt_tokens: 300,
            completion_tokens: 400,
            hit_rate: Some(0.5),
        },
        json!({
            "session_id": "chat-1",
            "hits": 1,
            "misses": 2,
            "read_tokens": 100,
            "creation_tokens": 200,
            "prompt_tokens": 300,
            "completion_tokens": 400,
            "hit_rate": 0.5,
        }),
    );
}

#[test]
fn session_undo_can_undo_and_undo_depth() {
    wire(
        SessionUndoReply {
            session_id: "chat-1".to_string(),
            undone: vec![crate::types::UndoSummary {
                messages_removed: 2,
            }],
        },
        json!({"session_id": "chat-1", "undone": [{"messages_removed": 2}]}),
    );
    wire(
        SessionCanUndoReply {
            session_id: "chat-1".to_string(),
            can_undo: true,
        },
        json!({"session_id": "chat-1", "can_undo": true}),
    );
    wire(
        SessionUndoDepthReply {
            session_id: "chat-1".to_string(),
            undo_depth: 2,
        },
        json!({"session_id": "chat-1", "undo_depth": 2}),
    );
}

#[test]
fn session_status() {
    wire(
        SessionStatusReply { status: Vec::new() },
        json!({"status": []}),
    );
}

#[test]
fn session_cleanup() {
    wire(
        SessionCleanupReply {
            deleted: vec![crate::session::SessionId::parse("chat-1").unwrap()],
            total: 1,
            dry_run: false,
            scope: "docs".to_string(),
        },
        json!({"deleted": ["chat-1"], "total": 1, "dry_run": false, "scope": "docs"}),
    );
}

#[test]
fn session_render_markdown_and_export_to_file() {
    wire(
        SessionRenderMarkdownResponse {
            markdown: "# Hi".to_string(),
        },
        json!({"markdown": "# Hi"}),
    );
    wire(
        SessionExportToFileResponse {
            status: "ok".to_string(),
            output_path: "/tmp/session.md".to_string(),
        },
        json!({"status": "ok", "output_path": "/tmp/session.md"}),
    );
}

#[test]
fn lua_register_commands() {
    wire(
        LuaRegisterCommandsReply { registered: 2 },
        json!({"registered": 2}),
    );
}

#[test]
fn config_set_and_save() {
    wire(
        ConfigSetReply {
            ok: true,
            rejected: vec!["kilns".to_string()],
        },
        json!({"ok": true, "rejected": ["kilns"]}),
    );
    wire(
        ConfigSaveReply {
            ok: true,
            refused: Vec::new(),
            rejected: Vec::new(),
        },
        json!({"ok": true, "refused": [], "rejected": []}),
    );
}

#[test]
fn ui_set_theme() {
    wire(
        UiSetThemeReply {
            theme: "dark".to_string(),
        },
        json!({"theme": "dark"}),
    );
}

#[test]
fn workflow_start_approve_gate_status_and_cancel() {
    wire(
        WorkflowRunReply {
            session_id: "chat-1".to_string(),
            status: crate::workflow::WorkflowStatus::Completed,
        },
        json!({"session_id": "chat-1", "status": {"kind": "completed"}}),
    );
    wire(
        WorkflowStatusReply {
            status: crate::workflow::WorkflowStatus::Running,
            completed_slots: 1,
            total_slots: 3,
            scope: std::collections::HashMap::from([("out".to_string(), json!("value"))]),
        },
        json!({
            "status": {"kind": "running"},
            "completed_slots": 1,
            "total_slots": 3,
            "scope": {"out": "value"},
        }),
    );
    wire(
        WorkflowCancelReply {
            session_id: "chat-1".to_string(),
            status: "cancelled".to_string(),
        },
        json!({"session_id": "chat-1", "status": "cancelled"}),
    );
    wire(
        WorkflowCancelReply {
            session_id: "chat-1".to_string(),
            status: "not_found".to_string(),
        },
        json!({"session_id": "chat-1", "status": "not_found"}),
    );
}
