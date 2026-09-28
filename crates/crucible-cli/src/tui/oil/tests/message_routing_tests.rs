//! Message routing invariant tests.
//!
//! Verifies that every ChatAppMsg variant is routed to the correct handler
//! and produces the expected state change. Catches category mismatches
//! where a message is categorized as one type but handled in another.

use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::tests::helpers::{EventFeed, SendMsgs, ToolCallEvent};

// ─── Error routing ─────────────────────────────────────────────────────────

#[test]
fn error_message_creates_notification() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::Error("something broke".into()));

    assert!(
        app.has_notifications(),
        "Error message should create a notification"
    );
}

#[test]
fn error_during_streaming_creates_notification() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    app.send_msgs(feed.text("partial response"));
    app.on_message(ChatAppMsg::Error("LLM connection lost".into()));

    assert!(
        app.has_notifications(),
        "Stream error should create notification even during active streaming"
    );
}

// ─── Context usage routing ─────────────────────────────────────────────────

#[test]
fn context_usage_updates_state() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ContextUsage {
        used: 5000,
        total: 128000,
    });

    let (used, total) = app.context_usage();
    assert_eq!(used, 5000);
    assert_eq!(total, 128000);
}

// ─── Model flow routing ────────────────────────────────────────────────────

#[test]
fn models_loaded_updates_state() {
    let mut app = OilChatApp::default();
    let models = vec!["ollama/llama3".into(), "openai/gpt-4".into()];
    app.on_message(ChatAppMsg::ModelsLoaded(models));

    assert_eq!(app.available_models().len(), 2);
}

#[test]
fn models_fetch_failed_updates_state() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ModelsFetchFailed("timeout".into()));

    assert!(
        matches!(
            app.model_list_state(),
            crate::tui::oil::chat_app::model_state::ModelListState::Failed
        ),
        "ModelsFetchFailed should set state to Failed"
    );
}

// ─── Status routing ────────────────────────────────────────────────────────

#[test]
fn status_message_updates_status() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::Status("Thinking...".into()));

    assert_eq!(app.status_text(), "Thinking...");
}

// ─── Mode change routing ───────────────────────────────────────────────────

#[test]
fn mode_changed_updates_mode() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ModeChanged("plan".into()));

    assert_eq!(app.mode(), "plan");
}

// ─── Stream lifecycle routing ──────────────────────────────────────────────

#[test]
fn text_delta_starts_streaming() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    assert!(!app.is_streaming());

    app.send_msgs(feed.text("hello"));
    assert!(app.is_streaming());
}

#[test]
fn stream_complete_ends_streaming() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    app.send_msgs(feed.text("hello"));
    assert!(app.is_streaming());

    app.send_msgs(feed.complete());
    assert!(!app.is_streaming());
}

#[test]
fn stream_cancelled_ends_streaming() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    app.send_msgs(feed.text("partial"));
    assert!(app.is_streaming());

    app.on_message(ChatAppMsg::StreamCancelled);
    assert!(!app.is_streaming());
}

// ─── Delegation routing ────────────────────────────────────────────────────

#[test]
fn subagent_spawned_creates_container() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    app.send_msgs(feed.msgs("delegation_spawned", serde_json::json!({ "delegation_id": "agent-1", "prompt": "analyze code", "target_agent": null })));

    assert_eq!(app.container_list.len(), 1);
}

#[test]
fn subagent_completed_marks_container_complete() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    app.send_msgs(feed.msgs("delegation_spawned", serde_json::json!({ "delegation_id": "agent-1", "prompt": "analyze code", "target_agent": null })));
    app.send_msgs(feed.msgs(
        "delegation_completed",
        serde_json::json!({ "delegation_id": "agent-1", "result_summary": "done" }),
    ));

    let node = &app.container_list.nodes()[0];
    assert!(
        matches!(node, crate::tui::oil::containers::ChatNode::SubagentTask { agent } if agent.is_terminal()),
        "Subagent task should be complete"
    );
}

// ─── Tool routing ──────────────────────────────────────────────────────────

#[test]
fn tool_call_creates_tool_group() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "read_file",
        call_id: "c1",
        args: r#"{"path": "main.rs"}"#,
        render: Some("main.rs".into()),
        ..Default::default()
    }));

    assert_eq!(app.container_list.len(), 1);
}

#[test]
fn tool_call_update_replaces_empty_diffs_with_late_content() {
    use crucible_core::types::acp::FileDiff;

    // Simulates the ACP late-diff flow (Claude Code): the daemon
    // first emits a `tool_call` with empty diffs, then a follow-up
    // `tool_call_update` carries the diff content.
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    app.send_msgs(feed.tool(ToolCallEvent {
        tool: "edit_file",
        call_id: "late-1",
        args: r#"{"path": "src/late.rs"}"#,
        render: Some("src/late.rs".into()),
        ..Default::default()
    }));

    let diffs = vec![FileDiff::from_contents(
        "src/late.rs",
        Some("fn old() {}\n".to_string()),
        "fn new() {}\n",
    )];
    app.send_msgs(feed.msgs(
        "tool_call_update",
        serde_json::json!({
            "call_id": "late-1",
            "display": { "kind": "edit", "tool": "edit_file", "diffs": diffs },
        }),
    ));

    let nodes = app.container_list.nodes();
    if let crate::tui::oil::containers::ChatNode::ToolGroup { tools } = &nodes[0] {
        assert_eq!(
            tools[0].diffs, diffs,
            "a late tool_call_update must populate diffs on the matching tool"
        );
    } else {
        panic!("expected ToolGroup node");
    }
}

#[test]
fn tool_call_update_for_unknown_call_id_is_a_noop() {
    use crucible_core::types::acp::FileDiff;

    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    let diffs = vec![FileDiff::from_contents(
        "src/orphan.rs",
        None,
        "fn anything() {}\n",
    )];
    // No prior ToolCall — should silently skip without panicking.
    app.send_msgs(feed.msgs(
        "tool_call_update",
        serde_json::json!({
            "call_id": "ghost",
            "display": { "kind": "edit", "tool": "edit_file", "diffs": diffs },
        }),
    ));
    assert_eq!(
        app.container_list.len(),
        0,
        "orphan diff update must not insert a node"
    );
}

#[test]
fn tool_result_error_sets_error_on_tool() {
    let mut app = OilChatApp::default();
    let mut feed = EventFeed::default();
    app.send_msgs(feed.tool_call("bash", "c1", "{}"));
    app.send_msgs(feed.tool_error("bash", "c1", "command not found"));

    let nodes = app.container_list.nodes();
    if let crate::tui::oil::containers::ChatNode::ToolGroup { tools } = &nodes[0] {
        assert!(tools[0].error.is_some());
    } else {
        panic!("expected ToolGroup node");
    }
}

// ─── Interaction routing ───────────────────────────────────────────────────

#[test]
fn open_interaction_opens_modal() {
    let mut app = OilChatApp::default();
    use crucible_core::interaction::{InteractionRequest, PermRequest};

    let request = InteractionRequest::Permission(PermRequest::bash(["ls", "-la"]));

    app.on_message(ChatAppMsg::OpenInteraction {
        request_id: "req-1".into(),
        request,
    });

    assert!(
        app.has_interaction_modal(),
        "OpenInteraction should open the interaction modal"
    );
}

// ─── Category exhaustiveness ───────────────────────────────────────────────

/// Verify that every message variant that reaches on_message produces
/// a meaningful state change (not silently dropped to trace stub).
///
/// This test exists because category mismatches (e.g., Error categorized
/// as Ui but handled in Stream) cause silent drops.
#[test]
fn no_message_silently_dropped() {
    type TestCase<'a> = (&'a str, ChatAppMsg, Box<dyn Fn(&OilChatApp) -> bool>);
    let test_cases: Vec<TestCase<'_>> = vec![
        (
            "Error",
            ChatAppMsg::Error("test error".into()),
            Box::new(|app| app.has_notifications()),
        ),
        (
            "Status",
            ChatAppMsg::Status("test status".into()),
            Box::new(|app| app.status_text() == "test status"),
        ),
        (
            "ModeChanged",
            ChatAppMsg::ModeChanged("plan".into()),
            Box::new(|app| app.mode() == "plan"),
        ),
        (
            "ContextUsage",
            ChatAppMsg::ContextUsage {
                used: 100,
                total: 1000,
            },
            Box::new(|app| {
                let (u, t) = app.context_usage();
                u == 100 && t == 1000
            }),
        ),
        (
            "ModelsLoaded",
            ChatAppMsg::ModelsLoaded(vec!["m1".into()]),
            Box::new(|app| app.available_models().len() == 1),
        ),
        (
            "Transcript",
            EventFeed::default().text("hello").remove(0),
            Box::new(|app| app.is_streaming()),
        ),
    ];

    for (name, msg, check) in test_cases {
        let mut app = OilChatApp::default();
        app.on_message(msg);
        assert!(
            check(&app),
            "{} message was silently dropped — no state change detected",
            name
        );
    }
}

/// End to end through the TUI: a mode the TUI has never heard of arrives from
/// the daemon and the statusline renders it.
///
/// Before modes became Lua-declared this could not work at any layer —
/// `ChatMode::parse` mapped every unknown id to `Normal`, so the badge read
/// NORMAL while the daemon ran review.
#[test]
fn a_lua_declared_mode_reaches_the_statusline() {
    use crate::tui::oil::tests::helpers::vt_render;

    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ModesLoaded(
        crate::tui::oil::chat_app::state::mode_descriptors(&["ask", "review"]),
    ));
    app.on_message(ChatAppMsg::ModeSynced("review".into()));

    let frame = vt_render(&mut app);
    assert!(
        frame.contains("REVIEW"),
        "the statusline must render the mode the session is actually in; got:\n{frame}"
    );
}

/// A mode whose note writes become proposals says so in the badge, so a user
/// knows that the notes on disk do not change. The note follows the mode: a
/// mode change to an applying mode removes it.
#[test]
fn a_proposing_mode_says_so_in_the_statusline() {
    use crate::tui::oil::tests::helpers::vt_render;

    let mut modes = crate::tui::oil::chat_app::state::mode_descriptors(&["ask", "review"]);
    modes[1].writes = crucible_core::types::WriteMode::Propose;
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ModesLoaded(modes));

    app.on_message(ChatAppMsg::ModeSynced("review".into()));
    let frame = vt_render(&mut app);
    assert!(
        frame.contains("REVIEW · PROPOSES"),
        "a proposing mode must say so; got:\n{frame}"
    );

    app.on_message(ChatAppMsg::ModeSynced("ask".into()));
    let frame = vt_render(&mut app);
    assert!(
        frame.contains("ASK") && !frame.contains("PROPOSES"),
        "an applying mode has no note; got:\n{frame}"
    );
}

/// A mode change made by another client reaches this one. The daemon emits
/// `mode_changed`; the TUI had no arm for it, so the badge kept showing
/// whatever this client last set itself.
#[test]
fn a_mode_change_from_another_client_updates_the_mode() {
    use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

    let msgs = session_event_to_chat_msgs("mode_changed", &serde_json::json!({ "mode": "review" }));
    assert!(
        !msgs.is_empty(),
        "the daemon's mode_changed event must translate to a TUI message"
    );
    assert!(
        !msgs.iter().any(|m| matches!(m, ChatAppMsg::ModeChanged(_))),
        "an inbound event must not produce the outbound command — that RPCs \
         the daemon, which re-emits the event, which never terminates"
    );

    let mut app = OilChatApp::default();
    for msg in msgs {
        app.on_message(msg);
    }
    assert_eq!(app.mode(), "review");
}

// ─── Provider list routing ─────────────────────────────────────────────────

fn provider_info(name: &str) -> crucible_core::types::ProviderInfo {
    crucible_core::types::ProviderInfo {
        name: name.to_string(),
        provider_type: "ollama".to_string(),
        available: true,
        default_model: None,
        models: vec![],
        endpoint: None,
        reason: None,
        is_local: true,
    }
}

/// A zero-provider session used to no-op: the user saw a normal prompt, typed,
/// and got a raw transport error mid-conversation. The empty list must warn
/// with remedies instead of staying silent.
#[test]
fn an_empty_provider_list_surfaces_a_warning_with_remedies() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ProvidersListed(vec![]));

    assert!(
        app.has_notifications(),
        "an empty provider list must produce a visible warning, not silence"
    );
}

#[test]
fn a_populated_provider_list_sets_the_provider_without_warning() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ProvidersListed(vec![provider_info(
        "Ollama (Local)",
    )]));

    assert!(
        !app.has_notifications(),
        "a healthy provider list must not raise a warning"
    );
}
