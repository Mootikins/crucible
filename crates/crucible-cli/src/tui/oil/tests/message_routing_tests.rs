//! Message routing invariant tests.
//!
//! Verifies that every ChatAppMsg variant is routed to the correct handler
//! and produces the expected state change. Catches category mismatches
//! where a message is categorized as one type but handled in another.

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};

// ─── Error routing ─────────────────────────────────────────────────────────

#[test]
fn error_message_creates_notification() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::Error("something broke".into()));

    assert!(
        app.has_notifications(),
        "Error message should create a notification"
    );
}

#[test]
fn error_during_streaming_creates_notification() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::TextDelta("partial response".into()));
    app.on_message(ChatAppMsg::Error("LLM connection lost".into()));

    assert!(
        app.has_notifications(),
        "Stream error should create notification even during active streaming"
    );
}

// ─── Context usage routing ─────────────────────────────────────────────────

#[test]
fn context_usage_updates_state() {
    let mut app = OilChatApp::init();
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
    let mut app = OilChatApp::init();
    let models = vec!["ollama/llama3".into(), "openai/gpt-4".into()];
    app.on_message(ChatAppMsg::ModelsLoaded(models));

    assert_eq!(app.available_models().len(), 2);
}

#[test]
fn models_fetch_failed_updates_state() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ModelsFetchFailed("timeout".into()));

    assert!(
        matches!(
            app.model_list_state(),
            crate::tui::oil::chat_app::model_state::ModelListState::Failed(_)
        ),
        "ModelsFetchFailed should set state to Failed"
    );
}

// ─── Status routing ────────────────────────────────────────────────────────

#[test]
fn status_message_updates_status() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::Status("Thinking...".into()));

    assert_eq!(app.status_text(), "Thinking...");
}

// ─── Mode change routing ───────────────────────────────────────────────────

#[test]
fn mode_changed_updates_mode() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ModeChanged("plan".into()));

    assert_eq!(app.mode(), "plan");
}

// ─── Stream lifecycle routing ──────────────────────────────────────────────

#[test]
fn text_delta_starts_streaming() {
    let mut app = OilChatApp::init();
    assert!(!app.is_streaming());

    app.on_message(ChatAppMsg::TextDelta("hello".into()));
    assert!(app.is_streaming());
}

#[test]
fn stream_complete_ends_streaming() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::TextDelta("hello".into()));
    assert!(app.is_streaming());

    app.on_message(ChatAppMsg::StreamComplete);
    assert!(!app.is_streaming());
}

#[test]
fn stream_cancelled_ends_streaming() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::TextDelta("partial".into()));
    assert!(app.is_streaming());

    app.on_message(ChatAppMsg::StreamCancelled);
    assert!(!app.is_streaming());
}

// ─── Delegation routing ────────────────────────────────────────────────────

#[test]
fn subagent_spawned_creates_container() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "agent-1".into(),
        prompt: "analyze code".into(),
    });

    assert_eq!(app.container_list.len(), 1);
}

#[test]
fn subagent_completed_marks_container_complete() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::SubagentSpawned {
        id: "agent-1".into(),
        prompt: "analyze code".into(),
    });
    app.on_message(ChatAppMsg::SubagentCompleted {
        id: "agent-1".into(),
        summary: "done".into(),
    });

    let node = &app.container_list.nodes()[0];
    assert!(
        matches!(node, crate::tui::oil::containers::ChatNode::SubagentTask { agent } if agent.is_terminal()),
        "Subagent task should be complete"
    );
}

// ─── Tool routing ──────────────────────────────────────────────────────────

#[test]
fn tool_call_creates_tool_group() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "main.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
    });

    assert_eq!(app.container_list.len(), 1);
}

#[test]
fn tool_call_diff_update_replaces_empty_diffs_with_late_content() {
    use crucible_core::types::acp::FileDiff;

    // Simulates the ACP late-diff flow (Claude Code): the daemon
    // first emits a ToolCall with empty diffs, then a follow-up
    // ToolCallDiffUpdate carries the diff content.
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ToolCall {
        name: "edit_file".into(),
        args: r#"{"path": "src/late.rs"}"#.into(),
        call_id: Some("late-1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
    });

    let diffs = vec![FileDiff::from_contents(
        "src/late.rs",
        Some("fn old() {}\n".to_string()),
        "fn new() {}\n",
    )];
    app.on_message(ChatAppMsg::ToolCallDiffUpdate {
        call_id: "late-1".into(),
        diffs: diffs.clone(),
    });

    let nodes = app.container_list.nodes();
    if let crate::tui::oil::containers::ChatNode::ToolGroup { tools } = &nodes[0] {
        assert_eq!(
            tools[0].diffs, diffs,
            "late ToolCallDiffUpdate must populate diffs on the matching tool"
        );
    } else {
        panic!("expected ToolGroup node");
    }
}

#[test]
fn tool_call_diff_update_for_unknown_call_id_is_a_noop() {
    use crucible_core::types::acp::FileDiff;

    let mut app = OilChatApp::init();
    let diffs = vec![FileDiff::from_contents(
        "src/orphan.rs",
        None,
        "fn anything() {}\n",
    )];
    // No prior ToolCall — should silently skip without panicking.
    app.on_message(ChatAppMsg::ToolCallDiffUpdate {
        call_id: "ghost".into(),
        diffs,
    });
    assert_eq!(
        app.container_list.len(),
        0,
        "orphan diff update must not insert a node"
    );
}

#[test]
fn tool_result_error_sets_error_on_tool() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: "{}".into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
    });
    app.on_message(ChatAppMsg::ToolResultError {
        name: "bash".into(),
        error: "command not found".into(),
        call_id: Some("c1".into()),
    });

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
    let mut app = OilChatApp::init();
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
            "TextDelta",
            ChatAppMsg::TextDelta("hello".into()),
            Box::new(|app| app.is_streaming()),
        ),
    ];

    for (name, msg, check) in test_cases {
        let mut app = OilChatApp::init();
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

    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ModesLoaded(vec![
        "normal".to_string(),
        "review".to_string(),
    ]));
    app.on_message(ChatAppMsg::ModeSynced("review".into()));

    let frame = vt_render(&mut app);
    assert!(
        frame.contains("REVIEW"),
        "the statusline must render the mode the session is actually in; got:\n{frame}"
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

    let mut app = OilChatApp::init();
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
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ProvidersListed(vec![]));

    assert!(
        app.has_notifications(),
        "an empty provider list must produce a visible warning, not silence"
    );
}

#[test]
fn a_populated_provider_list_sets_the_provider_without_warning() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::ProvidersListed(vec![provider_info(
        "Ollama (Local)",
    )]));

    assert!(
        !app.has_notifications(),
        "a healthy provider list must not raise a warning"
    );
}
