//! Intent-level vocabulary over [`StoryRuntime`].
//!
//! Story tests should read as user intent ("send a message", "approve the
//! permission"), not as key codes and enum construction. These thin helpers
//! are the shared page-object layer: the same verbs a mock-tier story and the
//! cross-surface hero legs both speak, so a rename of the underlying key or
//! message shape is a one-line change here instead of across every spec.
//!
//! Signatures are deliberately small and stable — the hero-flow branch calls
//! into these, so treat them as an API.

use crossterm::event::KeyCode;

use crucible_core::interaction::{InteractionRequest, InteractionResponse, PermRequest};

use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::ChatAppMsg;

use super::support::StoryRuntime;

/// Type `text` and press Enter, as a user submitting a chat message. Returns
/// the resulting [`Action`] (normally `Action::Send(ChatAppMsg::UserMessage)`,
/// or `QueueMessage` if a turn is already streaming).
pub(crate) fn send_user_message(story: &mut StoryRuntime, text: &str) -> Action<ChatAppMsg> {
    story.text(text);
    story.enter()
}

/// Simulate the daemon streaming an assistant reply back to this console:
/// one text delta followed by stream completion. This is the mock-tier stand-in
/// for a real turn (the live tier drives a real model through the daemon).
pub(crate) fn stream_assistant_reply(story: &mut StoryRuntime, text: &str) {
    story.send(ChatAppMsg::TextDelta(text.to_string()));
    story.send(ChatAppMsg::StreamComplete);
}

/// Assert the assistant's reply containing `needle` becomes visible, settling
/// spinner/animation frames first. Panics (via `expect_frame`) with the last
/// frame if it never appears within `max_ticks`.
pub(crate) fn expect_assistant_contains(
    story: &mut StoryRuntime,
    needle: &str,
    max_ticks: usize,
) -> String {
    let needle = needle.to_string();
    story.expect_frame(move |frame| frame.contains(&needle), max_ticks)
}

/// Open a bash permission request modal (as the daemon would when a tool needs
/// approval). `argv` is the command the agent wants to run.
pub(crate) fn open_permission(
    story: &mut StoryRuntime,
    request_id: &str,
    argv: &[&str],
) -> Action<ChatAppMsg> {
    let request = InteractionRequest::Permission(PermRequest::bash(argv.iter().copied()));
    story
        .app()
        .open_interaction(request_id.to_string(), request)
}

/// Press `y` to approve the open permission modal; returns the allow/deny
/// decision the modal emitted (`Some(true)` when approved).
pub(crate) fn approve_permission(story: &mut StoryRuntime) -> Option<bool> {
    permission_decision(&story.key(KeyCode::Char('y')))
}

/// Press `n` to deny the open permission modal; returns the decision
/// (`Some(false)` when denied).
pub(crate) fn deny_permission(story: &mut StoryRuntime) -> Option<bool> {
    permission_decision(&story.key(KeyCode::Char('n')))
}

/// Run a shell command through the `!`-prefix path, opening the shell modal.
/// Returns the submit action. The modal spawns a real child; callers that need
/// its output pump the modal to completion (see `shell_tests`).
pub(crate) fn run_shell(story: &mut StoryRuntime, command: &str) -> Action<ChatAppMsg> {
    send_user_message(story, &format!("!{command}"))
}

/// Rehydrate this console from a recorded session by replaying its events —
/// the mock-tier analog of attaching a fresh TUI/web console to an existing
/// daemon session and having the viewport fill in from history.
pub(crate) fn hydrate_from_recording(story: &mut StoryRuntime, fixture: &str) {
    story.pump_fixture(fixture);
}

/// Extract the allow/deny decision carried by a `CloseInteraction` action.
fn permission_decision(action: &Action<ChatAppMsg>) -> Option<bool> {
    match action {
        Action::Send(ChatAppMsg::CloseInteraction {
            response: InteractionResponse::Permission(p),
            ..
        }) => Some(p.allowed),
        _ => None,
    }
}
