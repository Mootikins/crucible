//! US-401: Permission modal full flow.
//!
//! Security invariants (Esc-always-denies, y-always-allows) are property-
//! tested in `tests/permission_invariant_tests.rs`. This story covers the
//! end-to-end flow the doc flags as a gap: modal render → approve path
//! (tool result renders) and deny path (error result, turn continues),
//! plus queued-permission ordering.
//!
//! Written against the intent-vocabulary layer (`vocab.rs`) and the
//! `expect_frame` eventual-state helper, as an exemplar for new stories.

use crate::tui::oil::chat_app::ChatAppMsg;

use super::support::StoryRuntime;
use super::vocab::{approve_permission, deny_permission, open_permission};

#[test]
fn permission_modal_opens_and_shows_command() {
    let mut story = StoryRuntime::new(80, 24);
    let _ = open_permission(&mut story, "req-1", &["ls", "-la"]);

    assert!(story.app().has_interaction_modal(), "modal should open");
    // Eventual-state: the requested command becomes visible in the modal.
    story.expect_frame(|f| f.contains("ls"), 8);
}

#[test]
fn approve_emits_allow_and_closes_modal() {
    let mut story = StoryRuntime::new(80, 24);
    let _ = open_permission(&mut story, "req-1", &["ls"]);

    assert_eq!(approve_permission(&mut story), Some(true), "`y` must allow");
    assert!(
        !story.app().has_interaction_modal(),
        "modal should close after a decision"
    );
}

#[test]
fn approve_lets_tool_result_render() {
    let mut story = StoryRuntime::new(80, 24);
    let _ = open_permission(&mut story, "req-1", &["ls", "-la"]);
    assert_eq!(approve_permission(&mut story), Some(true));

    // Daemon runs the approved tool and streams the result back.
    story.pump_fixture("permission_flow.jsonl");
    story.expect_frame(|f| f.contains("Cargo.toml"), 8);
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
    let _ = open_permission(&mut story, "req-1", &["rm", "-rf", "/"]);

    assert_eq!(deny_permission(&mut story), Some(false), "`n` must deny");

    // The daemon reports the tool as errored and the turn continues.
    story.send(ChatAppMsg::ToolResultError {
        name: "bash".into(),
        error: "Permission denied by user".into(),
        call_id: Some("c1".into()),
    });
    story.send(ChatAppMsg::StreamComplete);

    story.expect_frame(|f| f.contains("Permission denied"), 8);
    assert!(
        !story.app().has_interaction_modal(),
        "the modal should not linger after denial"
    );
}

#[test]
fn queued_permissions_open_in_arrival_order() {
    let mut story = StoryRuntime::new(80, 24);
    let _ = open_permission(&mut story, "req-1", &["ls"]);
    // Second request arrives while the first modal is open → queued.
    let _ = open_permission(&mut story, "req-2", &["cat", "secret.txt"]);

    story.expect_frame(|f| f.contains("ls"), 8);

    // Approving the first auto-opens the queued second.
    assert_eq!(approve_permission(&mut story), Some(true));

    assert!(
        story.app().has_interaction_modal(),
        "queued request should open"
    );
    story.expect_frame(|f| f.contains("secret.txt"), 8);
}
