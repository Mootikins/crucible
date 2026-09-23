//! US-911: a published list reaches the actual terminal frame.
use super::support::StoryRuntime;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;

#[test]
fn a_status_replacement_reaches_the_narrow_frame_and_clears() {
    let mut story = StoryRuntime::new(40, 24);
    let data = serde_json::json!({"status": [
      {"id":"ask","text":"goal asks","priority":10,"color_group":"warn","action":"plugin_approval","pinned":true,"plugin":"goal"}
    ]});
    for msg in session_event_to_chat_msgs("status_items_changed", &data) {
        story.send(msg);
    }
    assert!(story.screen().contains("goal asks"));
    for msg in
        session_event_to_chat_msgs("status_items_changed", &serde_json::json!({"status": []}))
    {
        story.send(msg);
    }
    assert!(!story.screen().contains("goal asks"));
}
