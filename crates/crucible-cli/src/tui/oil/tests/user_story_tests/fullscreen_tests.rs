//! US-804: Read, select and copy in the full-screen mode (prototype).
//!
//! The full-screen mode builds each frame outside the app, so these tests
//! build it with `FullscreenView` or `FullscreenShell`, then write it with
//! the same row diff that `Terminal::present` uses, into vt100. The
//! assertions read the vt100 screen: its text and its inverse cells.

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
use crate::tui::oil::event::Event;
use crate::tui::oil::fullscreen::shell::{ChatPane, FullscreenShell, ShellAction};
use crate::tui::oil::fullscreen::{FullscreenView, ViewAction};
use crate::tui::oil::tests::vt100_runtime::Vt100TestRuntime;
use crate::tui::oil::theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use crucible_oil::focus::FocusContext;

const WIDTH: u16 = 60;
const HEIGHT: u16 = 24;

fn ctx_for<'a>(focus: &'a FocusContext) -> ViewContext<'a> {
    ViewContext::with_terminal_size(focus, theme::active(), (WIDTH, HEIGHT))
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

/// Each screen row as vt100 shows it, trailing spaces dropped.
fn screen_rows(vt: &Vt100TestRuntime) -> Vec<String> {
    vt.vt_screen()
        .rows(0, WIDTH)
        .map(|row| row.trim_end().to_string())
        .collect()
}

/// The screen row and column where `needle` starts.
fn find(vt: &Vt100TestRuntime, needle: &str) -> (u16, u16) {
    for (y, row) in screen_rows(vt).iter().enumerate() {
        if let Some(byte) = row.find(needle) {
            let col = unicode_width::UnicodeWidthStr::width(&row[..byte]);
            return (col as u16, y as u16);
        }
    }
    panic!("{needle:?} is not on screen:\n{}", vt.screen_contents());
}

/// The inverse cells of screen row `y`.
fn inverse_cols(vt: &Vt100TestRuntime, y: u16) -> Vec<u16> {
    let screen = vt.vt_screen();
    (0..WIDTH)
        .filter(|&x| screen.cell(y, x).is_some_and(|cell| cell.inverse()))
        .collect()
}

/// The columns of the text of a screen row: after the margin and the
/// bullet, up to the last visible character.
fn text_cols(row: &str) -> Vec<u16> {
    let body = row.trim_start_matches([' ', '\u{25CF}']);
    let lead = row.chars().count() - body.chars().count();
    (lead as u16..row.chars().count() as u16).collect()
}

fn shell_with(app: OilChatApp) -> FullscreenShell {
    FullscreenShell::new(
        vec![ChatPane {
            name: "chat".into(),
            app,
            view: FullscreenView::new(),
        }],
        None,
    )
}

/// A drag in the demo's shell, under its tab row, highlights the text under
/// the pointer, only its text, and copies the same text.
#[test]
fn a_drag_highlights_and_copies_only_the_text_under_the_pointer() {
    let paragraph = "Start of a paragraph that is long enough to wrap over \
                     three rows at sixty columns, so that the drag crosses \
                     two wraps before it reaches the END.";
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::TextDelta(format!("{paragraph}\n\nAfter it.")));
    app.on_message(ChatAppMsg::StreamComplete);
    let mut shell = shell_with(app);
    let focus = FocusContext::new();
    let mut vt = Vt100TestRuntime::new(WIDTH, HEIGHT);
    vt.present_fullscreen(&shell.frame(&ctx_for(&focus)));
    assert!(vt.vt_screen().alternate_screen());

    let (start_col, start_row) = find(&vt, "Start");
    let (end_col, end_row) = find(&vt, "END.");
    assert!(end_row >= start_row + 2, "the paragraph wraps");
    shell.handle_event(&mouse(
        MouseEventKind::Down(MouseButton::Left),
        start_col,
        start_row,
    ));
    shell.handle_event(&mouse(
        MouseEventKind::Drag(MouseButton::Left),
        WIDTH - 1,
        end_row,
    ));
    let copied = shell.handle_event(&mouse(
        MouseEventKind::Up(MouseButton::Left),
        WIDTH - 1,
        end_row,
    ));
    assert_eq!(
        copied,
        ShellAction::View(ViewAction::Copy(paragraph.into()))
    );

    vt.present_fullscreen(&shell.frame(&ctx_for(&focus)));
    let rows = screen_rows(&vt);
    // Row 0 is the tab row; it draws the active tab inverted.
    for y in 1..HEIGHT {
        let expected = if (start_row..=end_row).contains(&y) {
            text_cols(&rows[y as usize])
        } else {
            Vec::new()
        };
        assert_eq!(
            inverse_cols(&vt, y),
            expected,
            "row {y} {:?}: the highlight covers its text only",
            rows[y as usize]
        );
    }
    assert_eq!(
        inverse_cols(&vt, end_row).last(),
        Some(&(end_col + 3)),
        "the highlight ends at the last character"
    );
}

/// PageUp holds the reader while the answer streams, and a label says how
/// many rows are below.
#[test]
fn page_up_holds_the_reader_while_an_answer_streams() {
    let mut app = OilChatApp::default();
    for i in 0..40 {
        app.add_system_message(format!("line {i}"));
    }
    let mut view = FullscreenView::new();
    let focus = FocusContext::new();
    let mut vt = Vt100TestRuntime::new(WIDTH, HEIGHT);
    vt.present_fullscreen(&view.frame(&mut app, &ctx_for(&focus)));
    assert!(
        vt.screen_contents().contains("line 39"),
        "it follows the end"
    );

    let page_up = Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
    assert_eq!(view.handle_event(&page_up, &app), ViewAction::Handled);
    vt.present_fullscreen(&view.frame(&mut app, &ctx_for(&focus)));
    let held = screen_rows(&vt)[0].clone();
    assert!(vt.screen_contents().contains("rows below"));

    app.on_message(ChatAppMsg::UserMessage("more".into()));
    for word in ["streamed ", "words ", "arrive ", "below"] {
        app.on_message(ChatAppMsg::TextDelta(word.into()));
        vt.present_fullscreen(&view.frame(&mut app, &ctx_for(&focus)));
        assert_eq!(
            screen_rows(&vt)[0],
            held,
            "streamed rows do not move the reader"
        );
    }
}
