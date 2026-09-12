//! US-206 (a reply the model did not finish says so).
//!
//! The translation is unit-tested in `chat_runner/tests/translate.rs`; this is
//! the render half — the assertion that the note actually reaches a frame.
//! That is the whole point of it: the daemon knows the provider cut the answer
//! off, and until it is on screen the user reads a truncated reply as a
//! finished one.

use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

use super::support::StoryRuntime;

/// Drive the real wire event through the real translation into the app, so a
/// change on either side of that seam shows up here.
fn pump_reply(story: &mut StoryRuntime, text: &str, stop_reason: Option<&str>) {
    let mut data = serde_json::json!({
        "message_id": "msg-1",
        "full_response": text,
    });
    if let Some(reason) = stop_reason {
        data["stop_reason"] = serde_json::Value::String(reason.to_string());
    }
    for msg in session_event_to_chat_msgs("message_complete", &data) {
        story.send(msg);
    }
}

#[test]
fn a_truncated_reply_says_so_on_screen() {
    let mut story = StoryRuntime::new(80, 24);
    pump_reply(&mut story, "Half an ans", Some("max_tokens"));

    let screen = story.screen();
    assert!(
        screen.contains("output limit"),
        "a cut-off reply must say why it stops:\n{screen}"
    );
}

#[test]
fn a_refused_reply_says_so_on_screen() {
    let mut story = StoryRuntime::new(80, 24);
    pump_reply(&mut story, "I cannot help with that.", Some("refusal"));

    let screen = story.screen();
    assert!(
        screen.contains("declined"),
        "a refusal must be named, not read as an ordinary answer:\n{screen}"
    );
}

/// The reply itself stays on screen beside the note. The note is a second
/// node, so it must not take the place of the text it describes.
#[test]
fn the_partial_reply_stays_beside_the_note() {
    let mut story = StoryRuntime::new(80, 24);
    pump_reply(&mut story, "Half an ans", Some("max_tokens"));

    let screen = story.screen();
    assert!(
        screen.contains("Half an ans"),
        "the model's own words must survive the note:\n{screen}"
    );
}

/// A finished reply draws nothing extra, and so does one from a daemon too old
/// to name a reason. A fallback that guessed would put a warning under every
/// reply in an old recording.
#[test]
fn a_finished_reply_draws_no_note() {
    for reason in [Some("end_turn"), None] {
        let mut story = StoryRuntime::new(80, 24);
        pump_reply(&mut story, "A whole answer", reason);

        let screen = story.screen();
        assert!(
            !screen.contains("output limit") && !screen.contains("declined"),
            "nothing to warn about, reason {reason:?}:\n{screen}"
        );
    }
}
