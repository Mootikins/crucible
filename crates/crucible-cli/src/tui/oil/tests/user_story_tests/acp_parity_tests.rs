//! US-307: delegated (ACP) turns must present like internal ones.
//!
//! A tool call executed inside an external agent's own loop still shows up
//! as a card in our transcript. Without a provenance badge the card is
//! indistinguishable from a tool Crucible ran itself, which hides the fact
//! that another process — under another permission gate — touched the
//! workspace. See `docs/Meta/Plans/2026-08-03-acp-presentation-parity.md`.
//!
//! These cover the *consumer* half of the badge contract. The producer half —
//! the daemon stamping `Acp:<agent>` onto the `tool_call` event — is pinned in
//! `crucible-daemon/src/agent_manager/tests/messaging.rs`, because nothing in
//! this crate can fail when that literal regresses.
//!
//! The two thinking tests at the end belong to **US-203** (thinking display),
//! not US-307: they are about which thoughts reach the screen, which has
//! nothing to do with provenance. They live here because a delegated agent is
//! the shape that exposes the behaviour — it runs its own tool loop, so it
//! alternates reasoning and narration inside one turn.

use serde_json::json;

use super::support::StoryRuntime;
use super::vocab::{
    announce_tool_call, relay_session_event, relay_session_turn, send_user_message,
};

const READ_ARGS: &str = r#"{"path":"README.md"}"#;

#[test]
fn acp_tool_call_renders_a_provenance_badge() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "read the readme");
    announce_tool_call(&mut story, "read_file", READ_ARGS, Some("Acp:claude"));

    let frame = story.fresh_screen();
    assert!(
        frame.contains("acp:claude"),
        "delegated tool card showed no provenance badge:\n{frame}"
    );
}

#[test]
fn acp_tool_call_badge_survives_the_daemon_event_mapping() {
    // The daemon's `tool_call` event carries the agent under `source`; the
    // RPC → `ChatAppMsg` mapping must keep it all the way to the badge.
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "read the readme");
    relay_session_event(
        &mut story,
        "tool_call",
        serde_json::json!({
            "tool": "read_file",
            "args": {"path": "README.md"},
            "call_id": "c1",
            "source": "Acp:claude",
        }),
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("acp:claude"),
        "the daemon's `source` field did not reach the rendered badge:\n{frame}"
    );
}

#[test]
fn a_pre_badge_recording_still_badges_the_delegated_card() {
    // Sessions recorded before the daemon named the agent carry a bare
    // lowercase `acp`. Replaying one must still say "another agent ran this",
    // even though it can no longer say which one.
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "read the readme");
    announce_tool_call(&mut story, "read_file", READ_ARGS, Some("acp"));

    let frame = story.fresh_screen();
    assert!(
        frame.contains("[acp]"),
        "a pre-badge ACP recording replayed with no provenance badge:\n{frame}"
    );
}

/// US-203: a delegated agent runs its own tool loop, so it narrates and reasons
/// in alternation across a single turn. Every one of those thoughts is content
/// the user asked to see (`show_thinking` defaults on), not a restatement.
#[test]
fn a_delegated_agent_second_thought_reaches_the_screen() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "why is the build slow?");
    relay_session_turn(
        &mut story,
        &[
            ("thinking", json!({"content": "profile the build first"})),
            ("text_delta", json!({"content": "Checking the build. "})),
            ("thinking", json!({"content": "link time dominates"})),
            (
                "text_delta",
                json!({"content": "Linking is the bottleneck."}),
            ),
            (
                "message_complete",
                json!({"message_id": "m1", "full_response": "Checking the build. Linking is the bottleneck."}),
            ),
        ],
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("link time dominates"),
        "the delegated agent's second thought never rendered:\n{frame}"
    );
    assert!(
        frame.contains("profile the build first") && frame.contains("Linking is the bottleneck."),
        "letting the second thought through lost earlier turn content:\n{frame}"
    );
}

/// US-203, the counterweight: a provider that streams its reasoning and *then*
/// replays the whole block at stream end must not paint it twice. This is what
/// the original `saw_text_delta` guard bought, and it has to survive the
/// narrowing.
#[test]
fn an_end_of_stream_reasoning_replay_is_not_painted_twice() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "hello");
    relay_session_turn(
        &mut story,
        &[
            ("thinking", json!({"content": "weigh "})),
            ("thinking", json!({"content": "the tradeoffs"})),
            ("text_delta", json!({"content": "Here goes."})),
            ("thinking", json!({"content": "weigh the tradeoffs"})),
            (
                "message_complete",
                json!({"message_id": "m1", "full_response": "Here goes."}),
            ),
        ],
    );

    let frame = story.fresh_screen();
    assert_eq!(
        frame.matches("the tradeoffs").count(),
        1,
        "the end-of-stream reasoning replay rendered a second copy:\n{frame}"
    );
}
