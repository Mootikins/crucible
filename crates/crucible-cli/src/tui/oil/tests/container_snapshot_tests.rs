//! Snapshot tests for each container type rendered through the real terminal path.
//!
//! These tests verify that container rendering produces correct visual output
//! using `insta::assert_snapshot!` for regression protection.

use crate::tui::oil::app::App;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use super::helpers::vt_render;

// ─── Individual container types ────────────────────────────────────────────

#[test]
fn snapshot_user_message() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("What is the meaning of life?".into()));

    let output = vt_render(&mut app);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_assistant_text() {
    let mut app = OilChatApp::init();
    // User message first (triggers turn_active)
    app.on_message(ChatAppMsg::UserMessage("Hello".into()));
    app.on_message(ChatAppMsg::TextDelta("The answer is 42.".into()));
    app.on_message(ChatAppMsg::StreamComplete);

    let output = vt_render(&mut app);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_thinking_collapsed() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Think about this".into()));
    app.on_message(ChatAppMsg::ThinkingDelta(
        "Let me reason through this problem step by step to find the answer".into(),
    ));
    app.on_message(ChatAppMsg::TextDelta("Here is my conclusion.".into()));
    app.on_message(ChatAppMsg::StreamComplete);

    // After StreamComplete, thinking should graduate collapsed.
    // Render through vt100 which triggers graduation.
    let mut vt = super::vt100_runtime::Vt100TestRuntime::new(80, 24);
    vt.render_frame(&mut app);

    // The full history (scrollback + screen) should show collapsed thinking
    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);
    insta::assert_snapshot!(stripped);
}

#[test]
fn snapshot_tool_complete() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Read a file".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "src/main.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });
    app.on_message(ChatAppMsg::StreamComplete);

    let output = vt_render(&mut app);
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_tool_pending() {
    let mut app = OilChatApp::init();
    app.on_message(ChatAppMsg::UserMessage("Run a command".into()));
    app.on_message(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command": "ls"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    // Tool is still pending (no ToolResultComplete)

    let output = vt_render(&mut app);
    // Pending tool should have a spinner in the viewport (that's OK)
    insta::assert_snapshot!(output);
}

#[test]
fn snapshot_multi_turn() {
    let mut app = OilChatApp::init();

    // Turn 1: user → thinking → text → tool → continuation text
    app.on_message(ChatAppMsg::UserMessage("Analyze this code".into()));
    app.on_message(ChatAppMsg::ThinkingDelta("Reviewing the structure".into()));
    app.on_message(ChatAppMsg::TextDelta("I see a few issues.".into()));

    // Tool call
    app.on_message(ChatAppMsg::ToolCall {
        name: "read_file".into(),
        args: r#"{"path": "lib.rs"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: None,
        lua_primary_arg: None,
    });
    app.on_message(ChatAppMsg::ToolResultComplete {
        name: "read_file".into(),
        call_id: Some("c1".into()),
    });

    // Continuation text after tool
    app.on_message(ChatAppMsg::TextDelta("After reading the file, here are my findings.".into()));
    app.on_message(ChatAppMsg::StreamComplete);

    // Render through vt100 to exercise graduation
    let mut vt = super::vt100_runtime::Vt100TestRuntime::new(80, 30);
    vt.render_frame(&mut app);

    let full = vt.full_history();
    let stripped = crucible_oil::ansi::strip_ansi(&full);
    insta::assert_snapshot!(stripped);
}

#[test]
fn snapshot_user_and_assistant_exchange() {
    let mut app = OilChatApp::init();

    app.on_message(ChatAppMsg::UserMessage("What is Rust?".into()));
    app.on_message(ChatAppMsg::TextDelta(
        "Rust is a systems programming language focused on safety and performance."
            .into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    let output = vt_render(&mut app);
    insta::assert_snapshot!(output);
}
