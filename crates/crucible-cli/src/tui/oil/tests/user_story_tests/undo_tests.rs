//! US-902: Undo a turn.
//!
//! `/undo` dispatch is covered inline in `chat_app/command_handling.rs`.
//! Here we drive the render / event-stream half: the daemon's
//! `UndoComplete` toast, and the viewport reflecting reverted turns when
//! the daemon clears and re-emits the surviving history.

use super::support::StoryRuntime;
use crate::tui::oil::chat_app::ChatAppMsg;

/// Frame-sequence snapshot: two completed turns, then the daemon reverts
/// the last one and re-emits the survivor. Each frame is an independent
/// clean render (no streaming spinner), so the sequence is deterministic.
#[test]
fn undo_flow_frame_sequence() {
    let mut story = StoryRuntime::new(72, 16);
    story.pump_fixture("undo_flow.jsonl");
    story.capture("before undo (mistake turn live)");

    story.send(ChatAppMsg::UndoComplete {
        turns: 1,
        messages_removed: 2,
    });
    story.send(ChatAppMsg::ClearHistory);
    story.send(ChatAppMsg::UserMessage("What is 2 plus 2?".into()));
    story.send(ChatAppMsg::TextDelta("2 plus 2 equals 4.".into()));
    story.send(ChatAppMsg::StreamComplete);
    story.capture("after undo (survivor turn only)");

    insta::assert_snapshot!("undo_flow_frame_sequence", story.sequence());
}

#[test]
fn undo_toast_reports_turns_and_messages() {
    let mut story = StoryRuntime::new(80, 24);
    story.pump_fixture("undo_flow.jsonl");
    story.send(ChatAppMsg::UndoComplete {
        turns: 1,
        messages_removed: 2,
    });

    let screen = story.screen();
    assert!(
        screen.contains("Undid 1 turn"),
        "undo-complete toast should report the reverted turn count:\n{screen}"
    );
}

#[test]
fn undo_with_nothing_to_revert_still_reports_zero() {
    // The daemon signals a no-op undo as UndoComplete { turns: 0 }; the TUI
    // surfaces it as a toast rather than silently swallowing the command.
    let mut story = StoryRuntime::new(80, 24);
    story.send(ChatAppMsg::UndoComplete {
        turns: 0,
        messages_removed: 0,
    });

    let screen = story.screen();
    assert!(
        screen.contains("Undid 0 turn"),
        "a nothing-to-undo response should still surface a toast:\n{screen}"
    );
}

#[test]
fn undo_truncates_viewport_when_daemon_reverts() {
    let mut story = StoryRuntime::new(80, 24);
    story.pump_fixture("undo_flow.jsonl");

    // Pre-undo: the mistake turn (turn 2) is the live response on screen.
    let before = story.screen();
    assert!(
        before.contains("Rewriting the config"),
        "the mistake turn should be visible before undo:\n{before}"
    );

    // The daemon reverts the last turn and re-emits the surviving history:
    // UndoComplete → ClearHistory → replay of turn 1 only.
    story.send(ChatAppMsg::UndoComplete {
        turns: 1,
        messages_removed: 2,
    });
    story.send(ChatAppMsg::ClearHistory);
    story.send(ChatAppMsg::UserMessage("What is 2 plus 2?".into()));
    story.send(ChatAppMsg::TextDelta("2 plus 2 equals 4.".into()));
    story.send(ChatAppMsg::StreamComplete);

    // Render the post-revert state fresh (as a real terminal's full redraw
    // would), so we assert against the live container list, not vt100
    // scrollback left over from before the clear.
    let after = story.fresh_screen();
    assert!(
        !after.contains("Rewriting the config"),
        "the reverted turn must no longer be displayed:\n{after}"
    );
    assert!(
        after.contains("2 plus 2 equals 4"),
        "the surviving turn must still render:\n{after}"
    );
}
