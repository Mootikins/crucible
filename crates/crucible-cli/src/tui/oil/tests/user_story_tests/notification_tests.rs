//! US-701 (toast lifecycle) + US-702 (messages drawer).
//!
//! The `NotificationArea` store is unit-tested in
//! `components/notification_area.rs`; here we verify the rendered
//! behaviour through the real frame path: toasts stacking in the status
//! bar, the `:messages` drawer listing full history, and keypress
//! dismissal. Wall-clock toast expiry is exercised by an `#[ignore]`d
//! slow test (the 3s timeout is not injectable headlessly).

use std::thread::sleep;
use std::time::Duration;

use crossterm::event::KeyCode;
use crucible_core::types::Notification;

use super::support::StoryRuntime;

fn add(story: &mut StoryRuntime, n: Notification) {
    story.app().add_notification(n);
}

#[test]
fn messages_drawer_lists_all_notifications_in_order() {
    let mut story = StoryRuntime::new(80, 24);
    add(&mut story, Notification::toast("first entry saved"));
    add(&mut story, Notification::warning("context at 85 percent"));
    add(&mut story, Notification::toast("third entry done"));

    story.app().show_messages();
    let screen = story.screen();

    for msg in [
        "first entry saved",
        "context at 85 percent",
        "third entry done",
    ] {
        assert!(
            screen.contains(msg),
            "drawer should list notification {msg:?}:\n{screen}"
        );
    }
}

#[test]
fn latest_toast_shows_in_status_bar() {
    let mut story = StoryRuntime::new(80, 24);
    add(&mut story, Notification::toast("older toast"));
    add(&mut story, Notification::toast("newest toast"));

    let screen = story.screen();
    assert!(
        screen.contains("newest toast"),
        "status bar should surface the most recent toast:\n{screen}"
    );
}

#[test]
fn keypress_dismisses_open_drawer() {
    let mut story = StoryRuntime::new(80, 24);
    add(&mut story, Notification::warning("older warning entry"));
    add(&mut story, Notification::warning("newer warning entry"));

    story.app().show_messages();
    assert!(
        story.screen().contains("older warning entry"),
        "drawer should list both entries while open"
    );

    // Any key closes the drawer (handle_key's first branch).
    story.key(KeyCode::Esc);

    let closed = story.screen();
    assert!(
        !closed.contains("older warning entry"),
        "dismissing the drawer should stop rendering the stacked history:\n{closed}"
    );
}

#[test]
#[ignore = "slow: exercises the 3s wall-clock toast expiry"]
fn toast_auto_dismisses_after_timeout() {
    let mut story = StoryRuntime::new(80, 24);
    add(&mut story, Notification::toast("ephemeral toast"));
    assert!(story.screen().contains("ephemeral toast"));

    sleep(Duration::from_millis(3_100));

    // expire_toasts runs inside render_frame (via screen()).
    let after = story.screen();
    assert!(
        !after.contains("ephemeral toast"),
        "toast should auto-dismiss after its 3s timeout:\n{after}"
    );
}
