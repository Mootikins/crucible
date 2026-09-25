//! US-911: a published list reaches the actual terminal frame.
use super::support::StoryRuntime;
use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;
use crate::tui::oil::fullscreen::FullscreenView;
use crate::tui::oil::tests::vt100_runtime::Vt100TestRuntime;
use crate::tui::oil::theme;
use crucible_oil::focus::FocusContext;

/// The list as the daemon sends it: the engine's plugin-turn item for a
/// plugin set to `ask`, and two items that plugins published.
fn daemon_list() -> serde_json::Value {
    serde_json::json!({"status": [
        {"id":"plugin_turns:goal","text":"goal · ask","priority":0,"color_group":"warn",
         "action":"plugin_approval","pinned":true,"plugin":"goal","kind":"plugin_turns"},
        {"id":"sync","text":"sync idle","priority":10,"color_group":"ok",
         "action":null,"pinned":false,"plugin":"sync","kind":"published"},
        {"id":"index","text":"index ready","priority":20,"color_group":"hue-4",
         "action":null,"pinned":false,"plugin":"index","kind":"published"}
    ]})
}

fn relay(app: &mut OilChatApp, data: &serde_json::Value) {
    for msg in session_event_to_chat_msgs("status_items_changed", data) {
        app.on_message(msg);
    }
}

#[test]
fn a_status_replacement_reaches_the_narrow_frame_and_clears() {
    let mut story = StoryRuntime::new(40, 24);
    for msg in session_event_to_chat_msgs("status_items_changed", &daemon_list()) {
        story.send(msg);
    }
    let screen = story.screen();
    assert!(screen.contains("goal · ask"), "{screen}");
    for msg in
        session_event_to_chat_msgs("status_items_changed", &serde_json::json!({"status": []}))
    {
        story.send(msg);
    }
    assert!(!story.screen().contains("goal · ask"));
}

/// The text and the foreground color of each cell of the screen row that
/// holds `needle`.
fn status_row(vt: &Vt100TestRuntime, width: u16, needle: &str) -> Vec<(String, vt100::Color)> {
    let screen = vt.vt_screen();
    let (_, height) = screen.size();
    let y = (0..height)
        .find(|&y| screen.contents_between(y, 0, y, width).contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} is not on screen:\n{}", vt.screen_contents()));
    (0..width)
        .filter_map(|x| screen.cell(y, x))
        .map(|cell| (cell.contents().to_string(), cell.fgcolor()))
        .collect()
}

/// US-804 with US-911: the full-screen mode draws the status items as the
/// inline mode does. The row that holds them has the same text and the same
/// colors in both modes: the pinned plugin-turn item in its `warn` color,
/// the published items that fit, and the `+N` of those that do not.
#[test]
fn the_full_screen_mode_draws_the_status_items_as_the_inline_mode_does() {
    const WIDTH: u16 = 40;
    const HEIGHT: u16 = 20;

    let mut inline_app = OilChatApp::default();
    relay(&mut inline_app, &daemon_list());
    let mut inline = Vt100TestRuntime::new(WIDTH, HEIGHT);
    inline.render_frame(&mut inline_app);

    let mut full_app = OilChatApp::default();
    relay(&mut full_app, &daemon_list());
    let mut full = Vt100TestRuntime::new(WIDTH, HEIGHT);
    let focus = FocusContext::new();
    let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (WIDTH, HEIGHT));
    full.present_fullscreen(&FullscreenView::new().frame(&mut full_app, &ctx));

    let inline_row = status_row(&inline, WIDTH, "goal · ask");
    let full_row = status_row(&full, WIDTH, "goal · ask");
    let text = |row: &[(String, vt100::Color)]| -> String {
        row.iter()
            .map(|(c, _)| c.as_str())
            .collect::<String>()
            .trim_end()
            .to_string()
    };
    assert!(
        text(&inline_row).contains("sync idle"),
        "{}",
        text(&inline_row)
    );
    assert!(
        !text(&inline_row).contains("index ready"),
        "the item that does not fit folds: {}",
        text(&inline_row)
    );
    assert!(text(&inline_row).contains("+1"), "{}", text(&inline_row));
    assert_eq!(text(&full_row), text(&inline_row));
    assert_eq!(full_row, inline_row, "the same colors, cell by cell");
    let pinned = text(&full_row).find("goal").expect("the pinned item");
    let column = text(&full_row)[..pinned].chars().count();
    assert_ne!(
        full_row[column].1,
        vt100::Color::Default,
        "the pinned item has its color"
    );
}
