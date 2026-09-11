//! US-908 (a plugin's surface, drawn by the TUI).
//!
//! The cursor and refresh logic are unit-tested in `components/surface_modal.rs`
//! and the open/close reducer in `chat_app/tests.rs`. This is the render half:
//! proof that a declared surface reaches a frame, that the transcript is not
//! drawn behind it, and — the one that matters most — that a plugin pushing rows
//! never puts a panel over what the user is doing.

use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::chat_runner::session_event_to_chat_msgs;
use crate::tui::oil::components::SurfaceModalRow;

use super::support::StoryRuntime;

fn rows() -> Vec<SurfaceModalRow> {
    vec![
        SurfaceModalRow {
            id: "s1".into(),
            text: "crucible".into(),
            detail: None,
            mark: Some("busy".into()),
        },
        SurfaceModalRow {
            id: "s2".into(),
            text: "web-fix".into(),
            detail: Some("waiting".into()),
            mark: Some("blocked".into()),
        },
    ]
}

fn loaded(open_if_closed: bool) -> ChatAppMsg {
    named("sessions", open_if_closed)
}

/// One surface, named. The title stays "Sessions" for every one of them: a
/// title is a label a plugin picks, and a name is what the wire talks about.
fn named(name: &str, open_if_closed: bool) -> ChatAppMsg {
    ChatAppMsg::SurfaceLoaded {
        name: name.into(),
        title: "Sessions".into(),
        rows: rows(),
        version: 1,
        open_if_closed,
    }
}

/// The declared rows, the title and the footer hints all reach the frame.
#[test]
fn a_surface_reaches_the_frame_with_its_rows_and_hints() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(loaded(true));

    let screen = story.screen();
    assert!(screen.contains("Sessions"), "the title:\n{screen}");
    assert!(screen.contains("crucible"), "the first row:\n{screen}");
    assert!(screen.contains("web-fix"), "the second row:\n{screen}");
    assert!(screen.contains("waiting"), "the detail:\n{screen}");
    assert!(screen.contains("2 rows"), "the count:\n{screen}");
    assert!(screen.contains("close"), "the key hints:\n{screen}");
}

/// The plugin declares `busy` and `blocked`; the TUI owns the glyph. If the
/// plugin ever shipped the character, this is the assertion that would have to
/// change to accommodate it.
#[test]
fn the_tui_chooses_the_glyph_for_a_declared_mark() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(loaded(true));

    let screen = story.screen();
    assert!(screen.contains('●'), "busy renders as a dot:\n{screen}");
    assert!(screen.contains('⏸'), "blocked renders paused:\n{screen}");
}

/// The modal tree *is* the frame: `OilChatApp::view` returns early for an open
/// surface, so the rows drawn are the surface's and nothing else composes with
/// them.
///
/// Note what this cannot check. `Vt100TestRuntime::render_frame` calls the inline
/// render path, while production switches to `Terminal::render_fullscreen` via
/// `has_fullscreen_modal`. So the fullscreen *switch* is a T1 assertion
/// (`a_loaded_surface_opens_full_screen`), not something any T2 frame here can
/// see — the shell modal has the same blind spot.
#[test]
fn the_surface_tree_is_the_whole_frame() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(loaded(true));

    let screen = story.screen();
    assert!(screen.contains("Sessions"), "the surface drew:\n{screen}");
    assert!(
        !screen.contains('❯') && !screen.contains("Type a message"),
        "no prompt composed with it:\n{screen}"
    );
}

/// **The one that matters.** A `surface_changed` arrives whenever a plugin pushes
/// rows, at a moment the user did not choose. Driven through the real wire
/// translation, so a change on either side of that seam shows up here.
///
/// RED-verify by making the reducer open unconditionally — which is the shape the
/// bug had during development.
#[test]
fn a_plugin_pushing_rows_never_takes_the_screen() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::UserMessage("mid-sentence".into()));

    // The real event, through the real translation.
    for msg in session_event_to_chat_msgs(
        "surface_changed",
        &serde_json::json!({ "plugin": "p", "name": "sessions", "version": 2 }),
    ) {
        story.send(msg);
    }
    // And the fetch it triggers, as a background refresh.
    story.send(loaded(false));

    let screen = story.screen();
    assert!(
        screen.contains("mid-sentence"),
        "the user's screen is untouched:\n{screen}"
    );
    assert!(
        !screen.contains("Sessions"),
        "no panel appeared unbidden:\n{screen}"
    );
}

/// The same event refreshes a panel the user *did* open.
#[test]
fn a_plugin_pushing_rows_refreshes_an_open_surface() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(loaded(true));

    story.send(ChatAppMsg::SurfaceLoaded {
        name: "sessions".into(),
        title: "Sessions".into(),
        rows: vec![SurfaceModalRow {
            id: "s9".into(),
            text: "docs".into(),
            detail: None,
            mark: Some("ok".into()),
        }],
        version: 2,
        open_if_closed: false,
    });

    let screen = story.screen();
    assert!(screen.contains("docs"), "the new row is drawn:\n{screen}");
    assert!(
        !screen.contains("web-fix"),
        "the old rows are gone:\n{screen}"
    );
}

/// An empty surface says so. Drawing nothing looks like a broken panel.
#[test]
fn an_empty_surface_says_so() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::SurfaceLoaded {
        name: "sessions".into(),
        title: "Sessions".into(),
        rows: vec![],
        version: 1,
        open_if_closed: true,
    });

    let screen = story.screen();
    assert!(
        screen.contains("nothing here yet"),
        "an empty surface explains itself:\n{screen}"
    );
}

/// A plugin uninstall withdraws the surface, so the panel must leave the screen
/// and give the transcript back.
///
/// `fresh_screen` draws into a new terminal, exactly as a real full redraw
/// does. `screen` keeps what earlier frames painted, so it cannot prove that
/// rows went away.
///
/// RED-verify by making the reducer ignore the withdrawal, which is the shape
/// the defect had: the rows stayed drawn for a plugin that was gone.
#[test]
fn a_withdrawn_surface_stops_being_drawn() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::UserMessage("mid-sentence".into()));
    story.send(loaded(true));
    assert!(
        story.fresh_screen().contains("crucible"),
        "the panel drew first"
    );

    story.send(ChatAppMsg::SurfaceWithdrawn("sessions".into()));

    let screen = story.fresh_screen();
    assert!(
        !screen.contains("web-fix"),
        "the withdrawn rows are gone:\n{screen}"
    );
    assert!(
        screen.contains("mid-sentence"),
        "the transcript is back:\n{screen}"
    );
}

/// **The negative.** One plugin goes away while the user reads another plugin's
/// panel. That panel must stay on screen, with its rows.
///
/// Both surfaces carry the title "Sessions". A reducer that compared titles
/// would clear the screen here.
#[test]
fn a_withdrawal_of_another_surface_leaves_the_panel_drawn() {
    let mut story = StoryRuntime::new(80, 24);
    story.send(named("reviews", true));

    story.send(ChatAppMsg::SurfaceWithdrawn("sessions".into()));

    let screen = story.fresh_screen();
    assert!(
        screen.contains("crucible") && screen.contains("web-fix"),
        "another plugin's rows stay drawn:\n{screen}"
    );
    assert!(screen.contains("2 rows"), "the count stands:\n{screen}");
}
