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
use crucible_core::types::acp::FileDiff;

use crate::tui::oil::app::Action;
use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::chat_runner::SessionEventStream;

use super::support::StoryRuntime;

/// Type `text` and press Enter, as a user submitting a chat message. Returns
/// the resulting [`Action`] (normally `Action::Send(ChatAppMsg::UserMessage)`;
/// while a turn is streaming the draft stays in the input and this returns
/// `Action::Continue`).
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

/// Simulate the daemon announcing that a tool is running. `source` is the wire
/// provenance string the daemon stamps on the event (`Acp:claude`, `Mcp:gmail`,
/// `Core`, …); `None` for a call with no attribution.
pub(crate) fn announce_tool_call(
    story: &mut StoryRuntime,
    name: &str,
    args: &str,
    source: Option<&str>,
) {
    story.send(ChatAppMsg::ToolCall {
        name: name.to_string(),
        args: args.to_string(),
        call_id: Some(format!("{name}-1")),
        description: None,
        source: source.map(str::to_string),
        lua_primary_arg: None,
        diffs: Vec::new(),
        auto_approved: None,
    });
}

/// Simulate a delegated agent attaching file diffs to a tool call it already
/// announced — ACP's `tool_call_update` carrying `ToolCallContent::Diff`, which
/// the daemon forwards as a `tool_call_diff_update` event keyed only on
/// `call_id`. Only ACP produces this; the internal agent synthesizes its diffs
/// up front and ships them on the `tool_call` itself.
pub(crate) fn attach_late_diff(
    story: &mut StoryRuntime,
    call_id: &str,
    path: &str,
    old_content: &str,
    new_content: &str,
) {
    story.send(ChatAppMsg::ToolCallDiffUpdate {
        call_id: call_id.to_string(),
        diffs: vec![FileDiff::from_contents(
            path.to_string(),
            Some(old_content.to_string()),
            new_content.to_string(),
        )],
    });
}

/// Simulate the daemon reporting that a tool finished, as the pair of messages
/// the `tool_result` event maps to (output, then completion). `call_id` must
/// match the one [`announce_tool_call`] used, or the update misses the card.
pub(crate) fn complete_tool_call(
    story: &mut StoryRuntime,
    name: &str,
    call_id: &str,
    output: &str,
) {
    story.send(ChatAppMsg::ToolResultDelta {
        name: name.to_string(),
        delta: output.to_string(),
        call_id: Some(call_id.to_string()),
    });
    story.send(ChatAppMsg::ToolResultComplete {
        name: name.to_string(),
        call_id: Some(call_id.to_string()),
    });
}

/// Feed this console a raw daemon session event, through the same
/// `session_event → ChatAppMsg` mapping the live RPC client uses. Use this
/// (rather than [`announce_tool_call`]) when the story is about the wire
/// payload surviving that mapping.
///
/// **Stateless.** This is the bare mapping function; it carries nothing across
/// events. Any story whose meaning depends on what came before — replay dedup,
/// `message_complete` suppression — must use [`relay_session_turn`] instead, or
/// it will assert against a stream no live console produces.
pub(crate) fn relay_session_event(story: &mut StoryRuntime, event: &str, data: serde_json::Value) {
    for msg in crate::tui::oil::chat_runner::session_event_to_chat_msgs(event, &data) {
        story.send(msg);
    }
}

/// Relay a whole turn of raw daemon session events through the *stateful*
/// [`SessionEventStream`] the live runner and session resume both use.
///
/// Prefer this over repeated [`relay_session_event`] calls whenever the story
/// depends on cross-event state — de-duplicating a provider's end-of-stream
/// replays, or suppressing `message_complete`'s full-response snapshot once
/// granular deltas have streamed.
///
/// **One call is one turn, and each call builds a fresh `SessionEventStream`.**
/// Production keeps *one* stream per session for its whole life and relies on
/// the `user_message` event to reset the per-turn state
/// (`chat_runner/stream.rs`). So consecutive calls here model two turns only
/// because construction happens to reset the same fields; a story that needs to
/// pin the `user_message` reset itself must drive one stream across both turns
/// directly (see `session_event_stream_tests`).
pub(crate) fn relay_session_turn(story: &mut StoryRuntime, events: &[(&str, serde_json::Value)]) {
    let mut stream = SessionEventStream::new();
    for (event, data) in events {
        for msg in stream.translate(event, data) {
            story.send(msg);
        }
    }
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

/// Open a *tool* permission request modal, built exactly the way the daemon
/// builds one: `PermRequest::tool(name, args)` carrying whatever
/// `synthesize_diffs` makes of that name and those args against `workspace`.
///
/// Both live construction sites are literally this shape — the ACP gate
/// (`agent_manager/messaging/permission.rs`, the `request_permission`
/// callback) and the internal tool loop (same file, the `interaction_requested`
/// emitter). They differ only in where `name` comes from: the internal one has
/// the real tool name, the ACP one has a coarse `ToolKind`-derived stand-in
/// (`acp_tool_name`). Calling the daemon's own synthesizer rather than handing
/// in diffs means a change to tool-name normalization shows up here instead of
/// being papered over by a hand-built `FileDiff`.
pub(crate) fn open_tool_permission(
    story: &mut StoryRuntime,
    request_id: &str,
    tool_name: &str,
    args: serde_json::Value,
) -> Action<ChatAppMsg> {
    let diffs = crucible_daemon::tools::diff_synth::synthesize_diffs(tool_name, &args);
    let request =
        InteractionRequest::Permission(PermRequest::tool(tool_name, args).with_diffs(diffs));
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
