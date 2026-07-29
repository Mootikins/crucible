//! Container rendering tests.
//!
//! Two test populations live here:
//! - **State-assertion tests** verify purely structural invariants
//!   (`container_kind`, `container_list.len()`) — these were converted from
//!   full-screen VT snapshots as a DECLARED coverage-type change. They no
//!   longer catch visual-layout regressions (box rendering, statusline
//!   layout, indicator placement); visual coverage for those flows survives
//!   in `fixture_replay_tests.rs` (real recordings + styled ANSI snapshots).
//! - **Snapshot tests** verify visual rendering that state introspection
//!   cannot capture: spinner indicators (pending tools) and statusline
//!   context-usage formatting.

use super::helpers::vt_render;
use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::containers::ChatNode;

// ─── Structural state-assertion tests (DECLARED coverage-type change) ──────
//
// Each of these was previously a full-screen VT snapshot that captured the
// rendered user-message box, assistant text formatting, and statusline.
// The snapshot regressed on any visual change; the state assertion only
// regresses when the container model itself changes. The visual classes
// now outside coverage are noted per test.

#[test]
fn user_message_creates_user_container() {
    // Formerly `snapshot_user_message`.
    // Visual class now outside coverage: user-message box (▄▄/▀▀ bars, `>` prefix).
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage(
        "What is the meaning of life?".into(),
    ));

    let nodes = app.container_list.nodes();
    assert_eq!(nodes.len(), 1, "UserMessage should create one container");
    match &nodes[0] {
        ChatNode::UserMessage { text } => assert_eq!(text, "What is the meaning of life?"),
        other => panic!("expected UserMessage, got {other:?}"),
    }
}

#[test]
fn assistant_text_creates_response_container() {
    // Formerly `snapshot_assistant_text`.
    // Visual class now outside coverage: assistant markdown bullet (`● ` prefix).
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Hello".into()));
    app.on_message(ChatAppMsg::TextDelta("The answer is 42.".into()));
    app.on_message(ChatAppMsg::StreamComplete);

    let nodes = app.container_list.nodes();
    assert_eq!(nodes.len(), 2, "user + assistant should be 2 containers");
    assert!(
        matches!(nodes[0], ChatNode::UserMessage { .. }),
        "first container should be UserMessage"
    );
    match &nodes[1] {
        ChatNode::AssistantResponse { text, complete, .. } => {
            assert_eq!(text, "The answer is 42.");
            assert!(*complete, "StreamComplete should mark AssistantResponse complete");
        }
        other => panic!("expected AssistantResponse, got {other:?}"),
    }
}

#[test]
fn tool_complete_creates_tool_group() {
    // Formerly `snapshot_tool_complete`.
    // Visual class now outside coverage: completed-tool checkmark (`✓ ToolName arg`).
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Read a file".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "src/main.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);

    let nodes = app.container_list.nodes();
    assert_eq!(nodes.len(), 2, "user + tool group should be 2 containers");
    assert!(
        matches!(nodes[0], ChatNode::UserMessage { .. }),
        "first container should be UserMessage"
    );
    match &nodes[1] {
        ChatNode::ToolGroup { tools } => {
            assert_eq!(tools.len(), 1, "ToolGroup should hold one tool");
            assert!(tools[0].complete, "tool should be marked complete");
            assert_eq!(tools[0].name.as_ref(), "read_file");
        }
        other => panic!("expected ToolGroup, got {other:?}"),
    }
}

#[test]
fn multi_turn_creates_containers_in_order() {
    // Formerly `snapshot_multi_turn`.
    // Visual class now outside coverage: thinking-block collapse formatting,
    // continuation-text indentation, tool checkmark row.
    let mut app = OilChatApp::init();

    // Turn 1: user → thinking → text → tool → continuation text
    app.on_message(ChatAppMsg::UserMessage("Analyze this code".into()));
    app.on_message(ChatAppMsg::ThinkingDelta("Reviewing the structure".into()));
    app.on_message(ChatAppMsg::TextDelta("I see a few issues.".into()));

    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "lib.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });

    app.on_message(ChatAppMsg::TextDelta(
        "After reading the file, here are my findings.".into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    let nodes = app.container_list.nodes();
    // user → AssistantResponse(thinking+text, marked complete by ToolCall) →
    // ToolGroup → AssistantResponse(continuation text)
    assert_eq!(
        nodes.len(),
        4,
        "multi-turn should produce 4 containers (user, AR1, ToolGroup, AR2)"
    );
    assert!(matches!(nodes[0], ChatNode::UserMessage { .. }));
    match &nodes[1] {
        ChatNode::AssistantResponse { thinking, text, complete, .. } => {
            assert!(!thinking.is_empty(), "AR1 should hold the streamed thinking");
            assert!(!text.is_empty(), "AR1 should hold the streamed text");
            assert!(*complete, "AR1 should be marked complete before the tool");
        }
        other => panic!("expected first AssistantResponse, got {other:?}"),
    }
    assert!(
        matches!(nodes[2], ChatNode::ToolGroup { .. }),
        "third container should be ToolGroup"
    );
    match &nodes[3] {
        ChatNode::AssistantResponse { text, complete, .. } => {
            assert!(!text.is_empty(), "continuation AR should hold text");
            assert!(*complete, "StreamComplete should mark continuation AR");
        }
        other => panic!("expected continuation AssistantResponse, got {other:?}"),
    }
}

#[test]
fn user_assistant_exchange_creates_two_containers() {
    // Formerly `snapshot_user_and_assistant_exchange`.
    // Visual class now outside coverage: user-message box, assistant bullet prefix.
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::UserMessage("What is Rust?".into()));
    app.on_message(ChatAppMsg::TextDelta(
        "Rust is a systems programming language focused on safety and performance.".into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    let nodes = app.container_list.nodes();
    assert_eq!(nodes.len(), 2);
    assert!(matches!(nodes[0], ChatNode::UserMessage { .. }));
    match &nodes[1] {
        ChatNode::AssistantResponse { text, complete, .. } => {
            assert!(
                text.contains("systems programming language"),
                "assistant text should be preserved"
            );
            assert!(*complete);
        }
        other => panic!("expected AssistantResponse, got {other:?}"),
    }
}

// ─── Snapshot tests (visual rendering — MUST STAY snapshot) ────────────────
//
// These tests assert on visual rendering that state introspection cannot
// capture: spinners (visual indicators) and statusline token/percent
// formatting (escape-sequence layout).

#[test]
fn snapshot_tool_pending() {
    // Visual indicator: pending-tool spinner frame (`◐` braille).
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Run a command".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "ls"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    // Tool is still pending (no ToolResultComplete)

    let output = vt_render(&mut app);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_context_indicator_after_usage() {
    // Visual rendering: statusline token-count format ("Nk tok").
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::UserMessage("Hi".into()));
    app.on_message(ChatAppMsg::TextDelta("Hello!".into()));
    app.on_message(ChatAppMsg::ContextUsage {
        used: 2555,
        total: 0,
    });
    app.on_message(ChatAppMsg::StreamComplete);

    let output = vt_render(&mut app);
    // Statusline should show token count (no total → "Nk tok" format)
    assert!(
        output.contains("2k tok"),
        "Statusline should show token usage after ContextUsage message: {output:?}"
    );
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_context_indicator_with_percentage() {
    // Visual rendering: statusline context-percentage format ("N% ctx").
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::UserMessage("Hi".into()));
    app.on_message(ChatAppMsg::TextDelta("Hello!".into()));
    app.on_message(ChatAppMsg::ContextUsage {
        used: 4096,
        total: 131072,
    });
    app.on_message(ChatAppMsg::StreamComplete);

    let output = vt_render(&mut app);
    // Statusline should show percentage (has total → "N% ctx" format)
    assert!(
        output.contains("3% ctx"),
        "Statusline should show context percentage when total is known: {output:?}"
    );
    insta::assert_snapshot!(output);
}

// ─── Visual-content assertion tests (no snapshot, content presence checks) ─

#[test]
fn show_diffs_off_omits_diff_body() {
    use crucible_core::types::acp::FileDiff;

    let mut app = OilChatApp::init();
    app.set_show_diffs(false);
    app.on_message(ChatAppMsg::UserMessage("edit a file".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "edit_file".into(),
        args: r#"{"path": "src/lib.rs"}"#.into(),
        call_id: Some("e1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: vec![FileDiff::from_contents(
            "src/lib.rs",
            Some("fn old() {}\n".to_string()),
            "fn new() {}\n",
        )],
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "edit_file".into(),
        call_id: Some("e1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);

    let output = vt_render(&mut app);
    // With show_diffs off, the rendered diff body must not appear.
    // (The tool call header — file path, action — may still render.)
    assert!(
        !output.contains("fn old()") && !output.contains("fn new()"),
        "show_diffs=false should suppress diff body lines, got: {output:?}"
    );
}

#[test]
fn show_diffs_on_includes_diff_body() {
    use crucible_core::types::acp::FileDiff;

    let mut app = OilChatApp::init();
    app.set_show_diffs(true);
    app.on_message(ChatAppMsg::UserMessage("edit a file".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "edit_file".into(),
        args: r#"{"path": "src/lib.rs"}"#.into(),
        call_id: Some("e1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
        diffs: vec![FileDiff::from_contents(
            "src/lib.rs",
            Some("fn old() {}\n".to_string()),
            "fn new() {}\n",
        )],
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "edit_file".into(),
        call_id: Some("e1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);

    let output = vt_render(&mut app);
    assert!(
        output.contains("fn old()") || output.contains("fn new()"),
        "show_diffs=true must render diff body content: {output:?}"
    );
}
