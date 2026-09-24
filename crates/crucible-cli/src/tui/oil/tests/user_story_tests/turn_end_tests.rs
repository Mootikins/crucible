//! US-207 (a turn ends once, and a plugin can start the next one).
//!
//! A turn ENDS. The daemon says so with `turn_finished`, whatever the status,
//! and the console ends its turn there. A `turn:complete` handler that wants
//! more work gets a NEW turn, which reaches the console as another
//! `user_message`. These stories drive the real wire events through the real
//! translation into the app.

use serde_json::json;

use super::support::StoryRuntime;
use super::vocab::{relay_session_turn, send_user_message};

/// Each status ends the turn, a failure too: the console does not wait
/// forever. A cancel from ANOTHER client arrives this way as well.
#[test]
fn every_turn_finished_status_ends_the_turn() {
    for status in [
        "completed",
        "cancelled",
        "handler_cancelled",
        "timed_out",
        "failed",
    ] {
        let mut story = StoryRuntime::new(80, 24);
        send_user_message(&mut story, "hello");
        relay_session_turn(
            &mut story,
            &[
                ("text_delta", json!({"content": "partial"})),
                ("turn_finished", json!({"status": status, "error": "why"})),
            ],
        );
        assert!(!story.app().is_streaming(), "{status} must end the turn");
    }
}

/// A failed turn says why. The console shows the error that `turn_finished`
/// carries, so the turn does not stop with no cause on the screen.
#[test]
fn a_failed_turn_shows_its_error() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "hello");
    relay_session_turn(
        &mut story,
        &[(
            "turn_finished",
            json!({"status": "failed", "error": "agent turn error: LLM timeout"}),
        )],
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("agent turn error: LLM timeout"),
        "the console must show why the turn failed:\n{frame}"
    );
}

/// A turn that a handler cancelled says why, as a failed turn does. The loop
/// guard of an ACP turn is one such handler.
#[test]
fn a_handler_cancelled_turn_shows_its_reason() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "hello");
    relay_session_turn(
        &mut story,
        &[(
            "turn_finished",
            json!({
                "status": "handler_cancelled",
                "error": "Tool 'Read' is blocked for this stream after repeated failures.",
            }),
        )],
    );

    let frame = story.fresh_screen();
    assert!(
        frame.contains("Tool 'Read' is blocked"),
        "the console must show why a handler cancelled the turn:\n{frame}"
    );
}

/// The turn a `turn:complete` handler asks for reads as a turn of its own:
/// its message and its reply both render, under the first turn's reply.
#[test]
fn the_turn_a_handler_asks_for_renders_as_its_own_turn() {
    let mut story = StoryRuntime::new(80, 24);
    send_user_message(&mut story, "do the whole job");
    relay_session_turn(
        &mut story,
        &[
            ("text_delta", json!({"content": "First part."})),
            (
                "message_complete",
                json!({"message_id": "m1", "full_response": "First part."}),
            ),
            ("turn_finished", json!({"status": "completed"})),
            (
                "user_message",
                json!({"message_id": "m2", "content": "keep going",
                    "origin": {"kind": "plugin", "name": "goal"}}),
            ),
            ("text_delta", json!({"content": "Second part."})),
            (
                "message_complete",
                json!({"message_id": "m2", "full_response": "Second part."}),
            ),
            ("turn_finished", json!({"status": "completed"})),
        ],
    );

    let frame = story.fresh_screen();
    for needle in ["First part.", "keep going", "Second part."] {
        assert!(
            frame.contains(needle),
            "the console must draw {needle:?}:\n{frame}"
        );
    }
    assert!(
        !story.app().is_streaming(),
        "the second turn ends too:\n{frame}"
    );
}
