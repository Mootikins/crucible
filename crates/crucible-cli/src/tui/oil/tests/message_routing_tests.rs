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

    assert_eq!(
        app.mode(),
        crate::tui::oil::chat_app::state::ChatMode::Plan
    );
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
    });

    assert_eq!(app.container_list.len(), 1);
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
    let test_cases: Vec<(&str, ChatAppMsg, Box<dyn Fn(&OilChatApp) -> bool>)> = vec![
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
            Box::new(|app| {
                app.mode() == crate::tui::oil::chat_app::state::ChatMode::Plan
            }),
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
