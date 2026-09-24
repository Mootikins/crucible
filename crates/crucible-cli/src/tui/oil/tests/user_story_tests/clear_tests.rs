//! US-207: a plugin turn, its prompt and its clear name the plugin, live and
//! after a resume. Each event goes through the runner's translation.

use super::support::StoryRuntime;
use super::vocab::relay_session_turn;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;
use crucible_core::interaction::{InteractionRequest, PermRequest};
use serde_json::json;

#[test]
fn clear_marker_keeps_prior_turn_visible_in_the_tui() {
    let mut story = StoryRuntime::new(80, 24);
    relay_session_turn(
        &mut story,
        &[
            (
                "user_message",
                json!({"message_id": "m1", "content": "before clear"}),
            ),
            ("turn_finished", json!({"status": "completed"})),
            ("context_cleared", json!({"plugin": "alpha"})),
            (
                "user_message",
                json!({"message_id": "m2", "content": "after clear"}),
            ),
        ],
    );
    let screen = story.screen();
    assert!(screen.contains("before clear"), "{screen}");
    assert!(screen.contains("↻ alpha cleared the context"), "{screen}");
    assert!(screen.contains("after clear"), "{screen}");
}

#[test]
fn a_plugin_turn_and_its_prompt_name_the_plugin() {
    let mut story = StoryRuntime::new(80, 24);
    let origin = json!({"kind": "plugin", "name": "goal"});
    relay_session_turn(
        &mut story,
        &[(
            "user_message",
            json!({"message_id": "m1", "content": "keep going", "origin": origin}),
        )],
    );
    let mut request = PermRequest::bash(["ls"]);
    request.plugin = Some("goal".into());
    let _ = story
        .app()
        .open_interaction("r1".into(), InteractionRequest::Permission(request));
    let screen = story.screen();
    assert!(screen.contains("↻ goal"), "{screen}");
    assert!(screen.contains("keep going"), "{screen}");
    assert!(screen.contains("goal requests permission"), "{screen}");
}

/// A resume reads the log through the daemon's migration, so a log with the
/// old flat origin still shows the label.
#[test]
fn a_resumed_plugin_turn_keeps_its_label() {
    let old = vec![json!({"event": "user_message", "data": {
        "message_id": "m1", "content": "keep going", "origin": "plugin", "plugin": "goal"
    }})];
    let mut story = StoryRuntime::new(80, 24);
    for e in crucible_core::protocol::session_events::migrate_history(old) {
        for msg in session_event_to_chat_msgs(e["event"].as_str().unwrap(), &e["data"]) {
            story.send(msg);
        }
    }
    let screen = story.screen();
    assert!(screen.contains("↻ goal"), "{screen}");
    assert!(screen.contains("keep going"), "{screen}");
}
