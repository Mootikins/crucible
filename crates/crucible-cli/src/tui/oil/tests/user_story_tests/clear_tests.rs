//! Clearing context keeps the transcript visible around its divider.

use super::support::StoryRuntime;
use crate::tui::oil::chat_app::ChatAppMsg;

#[test]
fn clear_marker_keeps_prior_turn_visible_in_the_tui() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::UserMessage("before clear".into()));
    story.send(ChatAppMsg::StreamComplete);
    story.send(ChatAppMsg::SystemNotice(
        "↻ alpha cleared the context".into(),
    ));
    story.send(ChatAppMsg::UserMessage("after clear".into()));
    let screen = story.screen();
    assert!(screen.contains("before clear"), "{screen}");
    assert!(screen.contains("alpha cleared the context"), "{screen}");
    assert!(screen.contains("after clear"), "{screen}");
}

#[test]
fn plugin_turn_shows_its_owner_and_full_text() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::SystemNotice(
        "↻ alpha\ncontinue with the detailed plan".into(),
    ));
    let screen = story.screen();
    assert!(screen.contains("alpha"), "{screen}");
    assert!(
        screen.contains("continue with the detailed plan"),
        "{screen}"
    );
}
