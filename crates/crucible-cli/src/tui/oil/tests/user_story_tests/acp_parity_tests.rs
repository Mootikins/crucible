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

use super::support::StoryRuntime;
use super::vocab::{announce_tool_call, relay_session_event, send_user_message};

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
