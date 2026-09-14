//! US-505: the completion popup draws over the transcript and never moves the
//! prompt.
//!
//! The popup is a bottom-anchored overlay. It composites over the rows above
//! the prompt, and the frame reserves those rows whether the popup is open or
//! not — so opening one covers transcript rows instead of pushing the prompt
//! down the screen.

use super::support::StoryRuntime;
use crate::tui::oil::chat_app::{ChatAppMsg, KilnSummary};
use crossterm::event::KeyCode;

/// A session with a banner and one finished exchange — short enough that the
/// popup is taller than the transcript, which is where the frame used to grow.
fn short_session() -> StoryRuntime {
    let mut story = StoryRuntime::new(80, 24);
    story.app().announce_kilns(&[KilnSummary {
        name: "crucible".into(),
        path: "/home/u/crucible".into(),
    }]);
    story
        .send(ChatAppMsg::UserMessage("question".into()))
        .send(ChatAppMsg::TextDelta("answer".into()))
        .send(ChatAppMsg::StreamComplete);
    story
}

/// The row the input sits on, counted from the bottom of the frame.
fn prompt_row_from_bottom(screen: &str) -> usize {
    let lines: Vec<&str> = screen.lines().collect();
    let at = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with('>'))
        .unwrap_or_else(|| panic!("no prompt row in:\n{screen}"));
    lines.len() - at
}

#[test]
fn opening_the_popup_leaves_the_prompt_where_it_was() {
    let mut story = short_session();

    let before = story.screen();
    story.text("/");
    let after = story.screen();

    assert!(after.contains("/mode"), "the popup did not open:\n{after}");
    assert_eq!(
        before.lines().count(),
        after.lines().count(),
        "the popup changed the frame height.\nbefore:\n{before}\nafter:\n{after}"
    );
    assert_eq!(
        prompt_row_from_bottom(&before),
        prompt_row_from_bottom(&after),
        "the popup moved the prompt.\nbefore:\n{before}\nafter:\n{after}"
    );
}

#[test]
fn the_popup_draws_over_the_transcript() {
    let mut story = short_session();

    let before = story.screen();
    assert!(
        before.contains("answer"),
        "no transcript to cover:\n{before}"
    );

    story.text("/");
    let after = story.screen();

    assert!(
        !after.contains("answer"),
        "the popup must draw over the rows above the prompt, not beside them:\n{after}"
    );
    assert!(
        after.contains("kiln attached"),
        "only the rows under the popup may go — the rest of the transcript \
         must stay where it was:\n{after}"
    );
    assert_eq!(
        before.lines().count(),
        after.lines().count(),
        "covering rows must cost the frame nothing:\nbefore:\n{before}\nafter:\n{after}"
    );

    // Closing it gives the transcript back.
    story.key(KeyCode::Backspace);
    let closed = story.screen();
    assert!(
        closed.contains("answer"),
        "the transcript must come back when the popup closes:\n{closed}"
    );
}

/// The reserve is measured against the popup the app actually renders, not
/// against a constant that can drift from it.
#[test]
fn the_reserve_holds_the_whole_popup() {
    let mut story = short_session();
    story.text("/");
    let _ = story.screen();

    let (rows, offset) = story.overlay_extent().expect("the popup rendered");
    let reserved = story.min_viewport_rows();

    assert!(
        rows + offset <= reserved as usize,
        "the popup wants {rows} rows above an offset of {offset}, and the frame \
         reserves only {reserved} — the frame has to grow to hold it"
    );
}

/// The prompt's top edge is a half block, which lights half its row. Under an
/// open panel popup that reads as an unpainted seam between the two.
#[test]
fn no_half_lit_row_sits_between_the_popup_and_the_prompt() {
    let edge = crate::tui::oil::theme::active()
        .decorations
        .half_block_bottom;
    let mut story = short_session();

    let before = story.screen();
    assert!(
        before.lines().any(|line| line.starts_with(edge)),
        "the prompt draws a half-block edge when no popup is open:\n{before}"
    );

    story.text("/");
    let after = story.screen();
    let lines: Vec<&str> = after.lines().collect();
    let prompt_at = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with('>'))
        .expect("a prompt row");
    let seam = lines[prompt_at - 1];

    assert!(
        !seam.starts_with(edge),
        "the row between the popup and the prompt is still half lit: {seam:?}"
    );
}
