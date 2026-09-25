//! US-209 (the precognition notice lists the notes it pulled).
//!
//! The wire event goes through the real translation into the app, and the
//! assertions read the rendered frame. The daemon sends the score as a full
//! `f64`; only the display rounds it.

use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

use super::support::StoryRuntime;

fn pump_precognition(story: &mut StoryRuntime) {
    let data = serde_json::json!({
        "notes_count": 3,
        "query_summary": "how do kilns resolve links",
        "notes": [
            { "title": "Kilns", "kiln": "docs", "score": 0.834_567_123_4 },
            { "title": "Wikilinks", "score": 0.715_000_1 },
            { "title": "Help/Concepts/Link Resolution", "kiln": "docs", "score": 0.5 },
        ],
    });
    for msg in session_event_to_chat_msgs("precognition_complete", &data) {
        story.send(msg);
    }
}

#[test]
fn the_precognition_notice_puts_each_note_on_its_own_line() {
    let mut story = StoryRuntime::new(80, 12);
    pump_precognition(&mut story);

    let screen = story.fresh_screen();
    assert!(screen.contains("0.83  Kilns (docs)"), "{screen}");
    assert!(screen.contains("0.72  Wikilinks"), "{screen}");
    assert!(
        screen.contains("0.50  Help/Concepts/Link Resolution (docs)"),
        "{screen}"
    );
    assert!(
        !screen.contains("0.8345"),
        "the frame must not show the raw float: {screen}"
    );
    insta::assert_snapshot!(screen);
}
