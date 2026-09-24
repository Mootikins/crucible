use super::*;
use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::theme;
use crossterm::event::{KeyModifiers, MouseEvent};
use crucible_oil::focus::FocusContext;

pub(super) fn frame_at(
    view: &mut FullscreenView,
    app: &OilChatApp,
    width: u16,
    height: u16,
) -> Frame {
    let focus = FocusContext::new();
    let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (width, height));
    view.frame(app, &ctx)
}

pub(super) fn screen_text(frame: &Frame) -> Vec<String> {
    (0..frame.grid.height())
        .map(|y| {
            crucible_oil::ansi::strip_ansi(&frame.grid.row_ansi(y))
                .trim_end()
                .to_string()
        })
        .collect()
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

pub(super) fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

/// The first non-blank transcript row on screen.
fn top_text(view: &FullscreenView) -> String {
    let transcript = view.transcript();
    (view.scroll().top()..transcript.len())
        .find_map(|i| {
            let row = transcript.row(i)?;
            let text = crucible_oil::ansi::strip_ansi(&row.ansi);
            (!text.trim().is_empty()).then(|| text.trim().to_string())
        })
        .unwrap_or_default()
}

#[test]
fn the_prompt_sits_at_the_bottom_under_the_latest_rows() {
    let app = fixtures::app_with_exchanges(3);
    let mut view = FullscreenView::new();
    let frame = frame_at(&mut view, &app, 100, 30);
    let rows = screen_text(&frame);

    let (_, cursor_row) = frame.cursor.expect("the prompt shows a cursor");
    assert!(
        cursor_row as usize >= rows.len() - 6,
        "the prompt is near the bottom: {cursor_row}"
    );
    assert!(
        rows.iter().any(|r| r.contains("family")),
        "the last answer's last line is on screen: {rows:#?}"
    );
    assert!(view.scroll().follows());
}

#[test]
fn a_short_transcript_starts_at_the_top() {
    let mut app = OilChatApp::default();
    app.add_system_message("hello".into());
    let mut view = FullscreenView::new();
    let rows = screen_text(&frame_at(&mut view, &app, 60, 20));
    assert!(rows[0].contains("hello"), "{rows:#?}");
}

#[test]
fn page_up_holds_the_reader_while_text_streams() {
    let mut app = fixtures::app_with_exchanges(5);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &app, 100, 30);

    assert_eq!(view.handle_event(&key(KeyCode::PageUp), &app), ViewAction::Handled);
    frame_at(&mut view, &app, 100, 30);
    let held = top_text(&view);
    assert!(!view.scroll().follows());

    app.on_message(ChatAppMsg::UserMessage("more".into()));
    for delta in fixtures::stream_deltas(9).into_iter().take(40) {
        app.on_message(ChatAppMsg::TextDelta(delta));
        frame_at(&mut view, &app, 100, 30);
    }
    assert_eq!(top_text(&view), held, "streamed rows must not move a reader");
}

#[test]
fn the_wheel_scrolls_and_the_bottom_turns_follow_on() {
    let app = fixtures::app_with_exchanges(5);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &app, 100, 30);
    let bottom = view.scroll().top();

    view.handle_event(&mouse(MouseEventKind::ScrollUp, 5, 5), &app);
    assert_eq!(view.scroll().top(), bottom - 3);
    assert!(!view.scroll().follows());

    view.handle_event(&mouse(MouseEventKind::ScrollDown, 5, 5), &app);
    assert!(view.scroll().follows(), "reaching the bottom follows again");
}

#[test]
fn a_scrolled_view_shows_how_many_rows_are_below() {
    let app = fixtures::app_with_exchanges(5);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &app, 100, 30);
    view.handle_event(&key(KeyCode::PageUp), &app);
    let rows = screen_text(&frame_at(&mut view, &app, 100, 30));
    assert!(rows.iter().any(|r| r.contains("rows below")), "{rows:#?}");
}

/// Pass criterion 3: a resize while text streams keeps the scroll position.
#[test]
fn a_resize_while_text_streams_keeps_the_reader_at_the_same_text() {
    let mut app = fixtures::app_with_exchanges(12);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &app, 160, 40);
    for _ in 0..8 {
        view.handle_event(&key(KeyCode::PageUp), &app);
    }
    frame_at(&mut view, &app, 160, 40);
    let held = top_text(&view);

    app.on_message(ChatAppMsg::UserMessage("stream while resizing".into()));
    let deltas = fixtures::stream_deltas(20);
    for (i, delta) in deltas.into_iter().enumerate() {
        app.on_message(ChatAppMsg::TextDelta(delta));
        // A drag of the window edge: many sizes, one after another.
        let width = [160, 140, 120, 100, 90, 120, 160][i % 7];
        let rows = screen_text(&frame_at(&mut view, &app, width, 40));
        let first_words: String = held.split_whitespace().take(3).collect::<Vec<_>>().join(" ");
        assert!(
            rows.iter().any(|r| r.contains(&first_words)),
            "at width {width} the held text {first_words:?} left the screen: {rows:#?}"
        );
        assert!(!view.scroll().follows(), "a resize must not jump to the bottom");
    }
    frame_at(&mut view, &app, 160, 40);
    assert_eq!(top_text(&view), held, "back at the first width, the same top row");
}

#[test]
fn a_resize_at_the_bottom_stays_at_the_bottom() {
    let mut app = fixtures::app_with_exchanges(4);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &app, 160, 40);
    app.on_message(ChatAppMsg::UserMessage("q".into()));
    app.on_message(ChatAppMsg::TextDelta("streaming tail".into()));
    let rows = screen_text(&frame_at(&mut view, &app, 90, 40));
    assert!(view.scroll().follows());
    assert!(rows.iter().any(|r| r.contains("streaming tail")), "{rows:#?}");
}

/// Pass criteria 4 and 5 through the real bytes: a streamed frame is one
/// synchronized update, never clears the screen, and writes a few rows.
#[test]
fn streamed_frames_are_single_synchronized_updates_without_a_clear() {
    use crucible_oil::screen::ScreenDiff;
    let mut app = fixtures::app_with_exchanges(6);
    let mut view = FullscreenView::new();
    let mut diff = ScreenDiff::new();
    let mut parser = vt100::Parser::new(40, 120, 0);
    parser.process(b"\x1b[?1049h");
    let mut out = Vec::new();
    let first = frame_at(&mut view, &app, 120, 40);
    diff.present(&mut out, &first.grid, first.cursor).unwrap();
    parser.process(&out);

    app.on_message(ChatAppMsg::UserMessage("stream".into()));
    let mut row_counts = Vec::new();
    for delta in fixtures::stream_deltas(7).into_iter().take(60) {
        app.on_message(ChatAppMsg::TextDelta(delta));
        let frame = frame_at(&mut view, &app, 120, 40);
        out.clear();
        let stats = diff.present(&mut out, &frame.grid, frame.cursor).unwrap();
        if stats.rows_written == 0 {
            assert!(out.is_empty());
            continue;
        }
        row_counts.push(stats.rows_written);
        let bytes = String::from_utf8(out.clone()).unwrap();
        assert!(bytes.starts_with("\x1b[?2026h") && bytes.ends_with("\x1b[?2026l"));
        assert_eq!(bytes.matches("\x1b[?2026h").count(), 1);
        assert!(!bytes.contains("\x1b[2J") && !bytes.contains("\x1b[3J"));
        parser.process(&out);

        // The terminal shows exactly the frame.
        let expected = screen_text(&frame);
        let shown: Vec<String> = parser
            .screen()
            .rows(0, 120)
            .map(|r| r.trim_end().to_string())
            .collect();
        assert_eq!(shown, expected);
    }
    assert!(!row_counts.is_empty());
}
