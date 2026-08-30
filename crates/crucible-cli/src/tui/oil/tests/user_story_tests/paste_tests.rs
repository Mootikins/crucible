//! US-106: Bracketed paste (multi-line input).
//!
//! Bracketed paste reaches the input as one event, so multiline content stays
//! literal and cannot submit or execute line-by-line.

use crate::tui::oil::event::{Event, InputBuffer};

use super::support::StoryRuntime;

#[test]
fn insert_str_holds_multiline_text_in_one_buffer() {
    let mut buf = InputBuffer::new();
    buf.insert_str("first line\nsecond line\nthird line");

    assert_eq!(
        buf.content().lines().count(),
        3,
        "a multi-line paste should occupy a single buffer with N lines"
    );
    assert!(buf.content().contains('\n'));
}

#[test]
fn insert_str_never_submits() {
    let mut buf = InputBuffer::new();
    buf.insert_str("line one\nline two");
    // insert_str returns `()` — unlike Enter it cannot trigger a send.
    assert_eq!(buf.content(), "line one\nline two");
}

#[test]
fn bracketed_paste_inserts_multiline_text_without_submitting() {
    let mut story = StoryRuntime::new(80, 24);

    let action = story
        .app()
        .update(Event::Paste("first line\nsecond line".to_string()));

    assert!(matches!(action, crate::tui::oil::app::Action::Continue));
    assert_eq!(story.app().input_content(), "first line\nsecond line");
}

#[test]
fn command_prefixed_paste_stays_literal() {
    // A pasted `:`/`!`-prefixed block must remain buffered text, never
    // executed line-by-line. The buffer holds it verbatim until submit.
    let mut buf = InputBuffer::new();
    buf.insert_str(":set thinking\n:set verbose");
    assert_eq!(buf.content(), ":set thinking\n:set verbose");
}

#[test]
fn ctrl_j_inserts_newline_without_submitting() {
    let mut story = StoryRuntime::new(80, 24);
    story.text("line one");
    story.key_ctrl('j');
    story.text("line two");

    // No Enter was pressed, so nothing is submitted; both lines live in one
    // buffer — the manual equivalent of a multi-line paste.
    assert_eq!(story.app().input_content(), "line one\nline two");
}
