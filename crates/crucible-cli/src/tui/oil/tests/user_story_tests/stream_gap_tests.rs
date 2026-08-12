//! US-907 (the transcript admits when it is incomplete).
//!
//! The translation is unit-tested in `chat_runner/tests/stream_gap.rs`; this is
//! the render half — the assertion that the warning actually reaches a frame.
//! That is the whole point of the marker: the daemon knows events were lost, and
//! until it is on screen the user does not.

use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

use super::support::StoryRuntime;

/// Drive the real wire event through the real translation into the app, so a
/// change on either side of that seam shows up here.
fn pump_gap(story: &mut StoryRuntime, dropped: Option<u64>) {
    let data = match dropped {
        Some(n) => serde_json::json!({ "dropped": n }),
        None => serde_json::json!({}),
    };
    for msg in session_event_to_chat_msgs("stream_gap", &data) {
        story.send(msg);
    }
}

/// The count has to survive to the status bar, which shows one truncated line —
/// so it leads the message rather than trailing it.
#[test]
fn a_dropped_event_count_reaches_the_status_bar() {
    let mut story = StoryRuntime::new(80, 24);
    pump_gap(&mut story, Some(12));

    let screen = story.screen();
    assert!(
        screen.contains("12 events dropped"),
        "the gap warning must be on screen, count first:\n{screen}"
    );
}

/// The full sentence — what happened and what to do about it — is in the
/// messages drawer, where there is room for it.
#[test]
fn the_drawer_carries_the_whole_explanation_and_the_remedy() {
    let mut story = StoryRuntime::new(80, 24);
    pump_gap(&mut story, Some(12));

    story.app().show_messages();
    let screen = story.screen();
    assert!(
        screen.contains("incomplete"),
        "the drawer must say the conversation is incomplete:\n{screen}"
    );
    assert!(
        screen.contains("reload"),
        "a warning with no remedy leaves the user stuck:\n{screen}"
    );
}

/// A marker with no count still has to appear. `dropped` is the daemon's own
/// field, so its absence means a version skew — not a reason to hide data loss.
#[test]
fn a_gap_with_no_count_still_appears() {
    let mut story = StoryRuntime::new(80, 24);
    pump_gap(&mut story, None);

    let screen = story.screen();
    assert!(
        screen.contains("Events dropped"),
        "the gap must be visible even without a count:\n{screen}"
    );
}

/// The negative control: nothing about an ordinary turn produces this warning,
/// so its presence means something.
#[test]
fn an_ordinary_event_produces_no_gap_warning() {
    let mut story = StoryRuntime::new(80, 24);
    for msg in session_event_to_chat_msgs("text_delta", &serde_json::json!({ "content": "hi" })) {
        story.send(msg);
    }

    story.app().show_messages();
    let screen = story.screen();
    assert!(
        !screen.contains("dropped"),
        "a healthy stream must not claim a gap:\n{screen}"
    );
}
