//! US-401: Permission modal full flow.
//!
//! Security invariants (Esc-always-denies, y-always-allows) are property-
//! tested in `tests/permission_invariant_tests.rs`. This story covers the
//! end-to-end flow the doc flags as a gap: modal render → approve path
//! (tool result renders) and deny path (error result, turn continues),
//! plus queued-permission ordering.

use crossterm::event::KeyCode;

use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::ChatAppMsg;
use crucible_core::interaction::{InteractionRequest, InteractionResponse, PermRequest};

use super::support::StoryRuntime;

fn bash_perm(cmd: &[&str]) -> InteractionRequest {
    InteractionRequest::Permission(PermRequest::bash(cmd.iter().copied()))
}

/// The allow/deny decision carried by a CloseInteraction action, if any.
fn decision(action: &Action<ChatAppMsg>) -> Option<bool> {
    match action {
        Action::Send(ChatAppMsg::CloseInteraction {
            response: InteractionResponse::Permission(p),
            ..
        }) => Some(p.allowed),
        _ => None,
    }
}

#[test]
fn permission_modal_opens_and_shows_command() {
    let mut story = StoryRuntime::new(80, 24);
    story
        .app()
        .open_interaction("req-1".into(), bash_perm(&["ls", "-la"]));

    assert!(story.app().has_interaction_modal(), "modal should open");
    let screen = story.screen();
    assert!(
        screen.contains("ls"),
        "modal should display the requested command:\n{screen}"
    );
}

#[test]
fn approve_emits_allow_and_closes_modal() {
    let mut story = StoryRuntime::new(80, 24);
    story
        .app()
        .open_interaction("req-1".into(), bash_perm(&["ls"]));

    let action = story.key(KeyCode::Char('y'));
    assert_eq!(decision(&action), Some(true), "`y` must allow");
    assert!(
        !story.app().has_interaction_modal(),
        "modal should close after a decision"
    );
}

#[test]
fn approve_lets_tool_result_render() {
    let mut story = StoryRuntime::new(80, 24);
    story
        .app()
        .open_interaction("req-1".into(), bash_perm(&["ls", "-la"]));
    let action = story.key(KeyCode::Char('y'));
    assert_eq!(decision(&action), Some(true));

    // Daemon runs the approved tool and streams the result back.
    story.pump_fixture("permission_flow.jsonl");
    let screen = story.screen();
    assert!(
        screen.contains("Cargo.toml"),
        "the approved tool's result should render into the transcript:\n{screen}"
    );
}

#[test]
fn deny_emits_deny_and_turn_continues_with_error() {
    let mut story = StoryRuntime::new(80, 24);

    // A tool call is announced, then permission is requested for it.
    story.send(ChatAppMsg::ToolCall {
        name: "bash".into(),
        args: r#"{"command":"rm -rf /"}"#.into(),
        call_id: Some("c1".into()),
        description: None,
        source: Some("Core".into()),
        lua_primary_arg: None,
        diffs: Vec::new(),
    });
    story
        .app()
        .open_interaction("req-1".into(), bash_perm(&["rm", "-rf", "/"]));

    let action = story.key(KeyCode::Char('n'));
    assert_eq!(decision(&action), Some(false), "`n` must deny");

    // The daemon reports the tool as errored and the turn continues.
    story.send(ChatAppMsg::ToolResultError {
        name: "bash".into(),
        error: "Permission denied by user".into(),
        call_id: Some("c1".into()),
    });
    story.send(ChatAppMsg::StreamComplete);

    let screen = story.screen();
    assert!(
        screen.contains("Permission denied"),
        "a denied tool should surface an error result:\n{screen}"
    );
    assert!(
        !story.app().has_interaction_modal(),
        "the modal should not linger after denial"
    );
}

#[test]
fn queued_permissions_open_in_arrival_order() {
    let mut story = StoryRuntime::new(80, 24);
    story
        .app()
        .open_interaction("req-1".into(), bash_perm(&["ls"]));
    // Second request arrives while the first modal is open → queued.
    story
        .app()
        .open_interaction("req-2".into(), bash_perm(&["cat", "secret.txt"]));

    assert!(
        story.screen().contains("ls"),
        "the first request should be shown first"
    );

    // Approving the first auto-opens the queued second.
    let action = story.key(KeyCode::Char('y'));
    assert_eq!(decision(&action), Some(true));

    assert!(
        story.app().has_interaction_modal(),
        "queued request should open"
    );
    let screen = story.screen();
    assert!(
        screen.contains("secret.txt"),
        "the queued request should surface after the first resolves:\n{screen}"
    );
}
