//! US-106: Bracketed paste (multi-line input).
//!
//! GAP found: bracketed paste is not wired end-to-end. The terminal's
//! `CtEvent::Paste` is mapped to `Event::Tick` in
//! `chat_runner/runner.rs::convert_event` (the catch-all arm), and the
//! `Event` enum has no `Paste` variant — so a real paste never reaches
//! the input buffer as one block. `EnableBracketedPaste` is never issued
//! either, so terminals deliver paste as individual key events.
//!
//! What *is* implemented is the multi-line substrate a correct paste would
//! use: `InputBuffer::insert_str` holds N lines in one buffer without
//! submitting, and Ctrl+J inserts a literal newline mid-input. Those are
//! covered here; the missing paste-event plumbing is reported as a gap.

use crate::tui::oil::event::InputBuffer;

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
