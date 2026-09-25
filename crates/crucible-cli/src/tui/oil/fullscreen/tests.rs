use super::*;
use crate::tui::oil::chat_app::ChatAppMsg;
use crate::tui::oil::theme;
use crossterm::event::{KeyModifiers, MouseEvent};
use crucible_oil::focus::FocusContext;

pub(super) fn frame_at(
    view: &mut FullscreenView,
    app: &mut OilChatApp,
    width: u16,
    height: u16,
) -> Frame {
    let focus = FocusContext::new();
    let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (width, height));
    view.frame(app, &ctx)
}

pub(super) fn frame_at_ctx(
    width: u16,
    height: u16,
    f: impl FnOnce(&ViewContext<'_>) -> Frame,
) -> Frame {
    let focus = FocusContext::new();
    let ctx = ViewContext::with_terminal_size(&focus, theme::active(), (width, height));
    f(&ctx)
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
            let text = crucible_oil::ansi::strip_ansi(row.ansi);
            (!text.trim().is_empty()).then(|| text.trim().to_string())
        })
        .unwrap_or_default()
}

#[test]
fn the_prompt_sits_at_the_bottom_under_the_latest_rows() {
    let mut app = fixtures::app_with_exchanges(3);
    let mut view = FullscreenView::new();
    let frame = frame_at(&mut view, &mut app, 100, 30);
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
    let rows = screen_text(&frame_at(&mut view, &mut app, 60, 20));
    assert!(rows[0].contains("hello"), "{rows:#?}");
}

#[test]
fn page_up_holds_the_reader_while_text_streams() {
    let mut app = fixtures::app_with_exchanges(5);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 100, 30);

    assert_eq!(
        view.handle_event(&key(KeyCode::PageUp), &mut app),
        ViewAction::Handled
    );
    frame_at(&mut view, &mut app, 100, 30);
    let held = top_text(&view);
    assert!(!view.scroll().follows());

    app.on_message(ChatAppMsg::UserMessage("more".into()));
    for delta in fixtures::stream_deltas(9).into_iter().take(40) {
        app.on_message(ChatAppMsg::TextDelta(delta));
        frame_at(&mut view, &mut app, 100, 30);
    }
    assert_eq!(
        top_text(&view),
        held,
        "streamed rows must not move a reader"
    );
}

#[test]
fn the_wheel_scrolls_and_the_bottom_turns_follow_on() {
    let mut app = fixtures::app_with_exchanges(5);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 100, 30);
    let bottom = view.scroll().top();

    view.handle_event(&mouse(MouseEventKind::ScrollUp, 5, 5), &mut app);
    assert_eq!(view.scroll().top(), bottom - 3);
    assert!(!view.scroll().follows());

    view.handle_event(&mouse(MouseEventKind::ScrollDown, 5, 5), &mut app);
    assert!(view.scroll().follows(), "reaching the bottom follows again");
}

#[test]
fn a_scrolled_view_shows_how_many_rows_are_below() {
    let mut app = fixtures::app_with_exchanges(5);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 100, 30);
    view.handle_event(&key(KeyCode::PageUp), &mut app);
    let rows = screen_text(&frame_at(&mut view, &mut app, 100, 30));
    assert!(rows.iter().any(|r| r.contains("rows below")), "{rows:#?}");
}

/// Pass criterion 3: a resize while text streams keeps the scroll position.
#[test]
fn a_resize_while_text_streams_keeps_the_reader_at_the_same_text() {
    let mut app = fixtures::app_with_exchanges(12);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 160, 40);
    for _ in 0..8 {
        view.handle_event(&key(KeyCode::PageUp), &mut app);
    }
    frame_at(&mut view, &mut app, 160, 40);
    let held = top_text(&view);

    app.on_message(ChatAppMsg::UserMessage("stream while resizing".into()));
    let deltas = fixtures::stream_deltas(20);
    for (i, delta) in deltas.into_iter().enumerate() {
        app.on_message(ChatAppMsg::TextDelta(delta));
        // A drag of the window edge: many sizes, one after another.
        let width = [160, 140, 120, 100, 90, 120, 160][i % 7];
        let rows = screen_text(&frame_at(&mut view, &mut app, width, 40));
        let first_words: String = held
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            rows.iter().any(|r| r.contains(&first_words)),
            "at width {width} the held text {first_words:?} left the screen: {rows:#?}"
        );
        assert!(
            !view.scroll().follows(),
            "a resize must not jump to the bottom"
        );
    }
    frame_at(&mut view, &mut app, 160, 40);
    assert_eq!(
        top_text(&view),
        held,
        "back at the first width, the same top row"
    );
}

#[test]
fn a_resize_at_the_bottom_stays_at_the_bottom() {
    let mut app = fixtures::app_with_exchanges(4);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 160, 40);
    app.on_message(ChatAppMsg::UserMessage("q".into()));
    app.on_message(ChatAppMsg::TextDelta("streaming tail".into()));
    let rows = screen_text(&frame_at(&mut view, &mut app, 90, 40));
    assert!(view.scroll().follows());
    assert!(
        rows.iter().any(|r| r.contains("streaming tail")),
        "{rows:#?}"
    );
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
    let first = frame_at(&mut view, &mut app, 120, 40);
    diff.present(&mut out, &first.grid, first.cursor).unwrap();
    parser.process(&out);

    app.on_message(ChatAppMsg::UserMessage("stream".into()));
    let mut row_counts = Vec::new();
    for delta in fixtures::stream_deltas(7).into_iter().take(60) {
        app.on_message(ChatAppMsg::TextDelta(delta));
        let frame = frame_at(&mut view, &mut app, 120, 40);
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

/// Screen row and column where `needle` starts.
fn find_on_screen(frame: &Frame, needle: &str) -> (u16, u16) {
    for (y, row) in screen_text(frame).iter().enumerate() {
        if let Some(byte) = row.find(needle) {
            let col = unicode_width::UnicodeWidthStr::width(&row[..byte]);
            return (col as u16, y as u16);
        }
    }
    panic!("{needle:?} is not on screen: {:#?}", screen_text(frame));
}

fn drag_copy(
    view: &mut FullscreenView,
    app: &mut OilChatApp,
    from: (u16, u16),
    to: (u16, u16),
) -> String {
    use crossterm::event::MouseButton;
    view.handle_event(
        &mouse(MouseEventKind::Down(MouseButton::Left), from.0, from.1),
        app,
    );
    view.handle_event(
        &mouse(MouseEventKind::Drag(MouseButton::Left), to.0, to.1),
        app,
    );
    match view.handle_event(
        &mouse(MouseEventKind::Up(MouseButton::Left), to.0, to.1),
        app,
    ) {
        ViewAction::Copy(text) => text,
        other => panic!("a drag must copy, got {other:?}"),
    }
}

/// Pass criterion 2: a drag over a wrapped paragraph with wide characters
/// copies the source text exactly.
#[test]
fn a_drag_over_a_wrapped_paragraph_copies_the_source_text() {
    let paragraph = "Start of the paragraph with 日本語のテキスト and a family \
                     \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} emoji, long enough to wrap \
                     over several rows at forty columns, until the END.";
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::TextDelta(paragraph.into()));
    app.on_message(ChatAppMsg::StreamComplete);
    let mut view = FullscreenView::new();
    let frame = frame_at(&mut view, &mut app, 40, 30);

    let from = find_on_screen(&frame, "Start");
    let (end_col, end_row) = find_on_screen(&frame, "END.");
    let copied = drag_copy(&mut view, &mut app, from, (end_col + 3, end_row));
    assert_eq!(copied, paragraph);
}

#[test]
fn a_drag_over_a_wrapped_user_message_copies_the_source_text() {
    let message = "a user question that is long enough to wrap onto a second row and a third";
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage(message.into()));
    let mut view = FullscreenView::new();
    let frame = frame_at(&mut view, &mut app, 30, 30);

    let from = find_on_screen(&frame, "a user");
    let (end_col, end_row) = find_on_screen(&frame, "third");
    let copied = drag_copy(&mut view, &mut app, from, (end_col + 4, end_row));
    assert_eq!(copied, message);
}

#[test]
fn the_selection_is_drawn_inverted() {
    let mut app = OilChatApp::default();
    app.add_system_message("select me".into());
    let mut view = FullscreenView::new();
    let frame = frame_at(&mut view, &mut app, 40, 20);
    let (col, row) = find_on_screen(&frame, "select");
    drag_copy(&mut view, &mut app, (col, row), (col + 5, row));
    let frame = frame_at(&mut view, &mut app, 40, 20);
    assert!(frame.grid.row(row as usize)[col as usize]
        .style
        .contains("\x1b[7m"));
    assert!(!frame.grid.row(row as usize)[col as usize + 7]
        .style
        .contains("\x1b[7m"));
}

#[test]
fn a_plain_click_selects_and_copies_nothing() {
    use crossterm::event::MouseButton;
    let mut app = fixtures::app_with_exchanges(1);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 80, 30);
    view.handle_event(
        &mouse(MouseEventKind::Down(MouseButton::Left), 4, 3),
        &mut app,
    );
    let up = view.handle_event(
        &mouse(MouseEventKind::Up(MouseButton::Left), 4, 3),
        &mut app,
    );
    assert_eq!(up, ViewAction::Handled);
    assert!(view.selection().is_none());
}

#[test]
fn a_double_and_a_triple_click_copy_a_word_and_a_line() {
    use crossterm::event::MouseButton;
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::TextDelta(
        "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda".into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    let mut view = FullscreenView::new();
    let frame = frame_at(&mut view, &mut app, 30, 20);
    let (col, row) = find_on_screen(&frame, "gamma");

    let down = mouse(MouseEventKind::Down(MouseButton::Left), col + 1, row);
    let up = mouse(MouseEventKind::Up(MouseButton::Left), col + 1, row);
    view.handle_event(&down, &mut app);
    view.handle_event(&up, &mut app);
    view.handle_event(&down, &mut app);
    assert_eq!(
        view.handle_event(&up, &mut app),
        ViewAction::Copy("gamma".into())
    );
    view.handle_event(&down, &mut app);
    match view.handle_event(&up, &mut app) {
        ViewAction::Copy(line) => assert!(
            line.trim_start().trim_start_matches('●').trim()
                == "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda",
            "{line:?}"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_dump_key_prints_finished_entries_once() {
    let mut app = fixtures::app_with_exchanges(2);
    app.on_message(ChatAppMsg::UserMessage("q".into()));
    app.on_message(ChatAppMsg::TextDelta("still streaming".into()));
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 80, 30);

    let ViewAction::Dump(rows) = view.handle_event(&key(DUMP_KEY), &mut app) else {
        panic!("the dump key dumps");
    };
    let text: Vec<String> = rows
        .iter()
        .map(|r| crucible_oil::ansi::strip_ansi(r))
        .collect();
    assert!(text.iter().any(|r| r.contains("Answer 1")), "{text:#?}");
    assert!(
        !text.iter().any(|r| r.contains("still streaming")),
        "an unfinished node waits"
    );

    let ViewAction::Dump(again) = view.handle_event(&key(DUMP_KEY), &mut app) else {
        panic!();
    };
    assert!(again.is_empty(), "nothing is printed twice");

    // The exit prints the rest, the streaming answer included.
    let rest: Vec<String> = view
        .take_dump(&mut app, true)
        .iter()
        .map(|r| crucible_oil::ansi::strip_ansi(r))
        .collect();
    assert!(
        rest.iter().any(|r| r.contains("still streaming")),
        "{rest:#?}"
    );
    assert!(!rest.iter().any(|r| r.contains("Answer 1")));
    assert_eq!(rest[0], "", "a blank row separates it from the dumped part");
}

#[test]
fn the_exit_dump_reproduces_the_transcript_rows() {
    let mut app = fixtures::app_with_exchanges(3);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 90, 30);
    let rows = view.take_dump(&mut app, true);
    assert_eq!(rows.len(), view.transcript().len());
    let mut parser = vt100::Parser::new(rows.len() as u16 + 1, 90, 0);
    for row in &rows {
        parser.process(format!("{row}\x1b[0m\r\n").as_bytes());
    }
    let shown: Vec<String> = parser
        .screen()
        .rows(0, 90)
        .map(|r| r.trim_end().to_string())
        .collect();
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(
            shown[i],
            crucible_oil::ansi::strip_ansi(row).trim_end(),
            "row {i}"
        );
    }
}

#[test]
fn the_mouse_key_asks_to_toggle_capture() {
    let mut app = fixtures::app_with_exchanges(1);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 80, 30);
    assert_eq!(
        view.handle_event(&key(MOUSE_KEY), &mut app),
        ViewAction::ToggleMouse
    );
}

/// Found in the demo: a wider reflow put the held row in the last page,
/// which turned follow on and lost the place for every later resize.
#[test]
fn a_reflow_through_the_last_page_keeps_the_reader() {
    let mut app = fixtures::app_with_exchanges(6);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 90, 40);
    // Three rows up: a wider layout has fewer rows below, so the held row
    // lands in the last page.
    view.handle_event(&mouse(MouseEventKind::ScrollUp, 5, 5), &mut app);
    frame_at(&mut view, &mut app, 90, 40);
    let held = top_text(&view);

    for width in [200, 60, 90] {
        frame_at(&mut view, &mut app, width, 40);
    }
    assert!(!view.scroll().follows());
    assert_eq!(top_text(&view), held);
}

/// The columns of screen row `y` that the highlight inverts.
fn inverted_cols(frame: &Frame, y: usize) -> Vec<usize> {
    frame
        .grid
        .row(y)
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.style.contains("\x1b[7m"))
        .map(|(x, _)| x)
        .collect()
}

/// Found in the demo: a drag over paragraphs inverted a box of whole rows,
/// the margin gutter and the padding after the text included. The
/// highlight must cover only the text, as the copy does.
#[test]
fn the_highlight_covers_only_the_text_not_the_gutter() {
    let first = "First paragraph that is long enough to wrap over two rows at forty.";
    let second = "Second paragraph, short.";
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::TextDelta(format!("{first}\n\n{second}")));
    app.on_message(ChatAppMsg::StreamComplete);
    let mut view = FullscreenView::new();
    let frame = frame_at(&mut view, &mut app, 40, 30);
    let (start_col, start_row) = find_on_screen(&frame, "First");
    let (_, second_row) = find_on_screen(&frame, "Second");
    let (end_col, _) = find_on_screen(&frame, "short.");
    let copied = drag_copy(
        &mut view,
        &mut app,
        (start_col, start_row),
        (end_col + 6, second_row),
    );
    assert_eq!(copied, format!("{first}\n\n{second}"));

    let frame = frame_at(&mut view, &mut app, 40, 30);
    let rows = screen_text(&frame);
    let selected = start_row as usize..=second_row as usize;
    for (y, text) in rows
        .iter()
        .enumerate()
        .filter(|(y, _)| selected.contains(y))
    {
        // The gutter is the margin and the bullet of the first row.
        let body = text.trim_start_matches([' ', '\u{25CF}']);
        let lead = text.chars().count() - body.chars().count();
        let expected: Vec<usize> = (lead..text.chars().count()).collect();
        assert_eq!(
            inverted_cols(&frame, y),
            expected,
            "row {y} {text:?}: only its text is inverted"
        );
    }
}

/// A press in the gutter starts the selection at the text of that row,
/// and a release in the gutter ends it after the text of the row above.
#[test]
fn a_drag_from_and_to_the_gutter_snaps_to_the_text() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::TextDelta(
        "alpha row\n\nbeta row\n\ngamma row".into(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);
    let mut view = FullscreenView::new();
    let frame = frame_at(&mut view, &mut app, 40, 30);
    let (_, alpha) = find_on_screen(&frame, "alpha");
    let (_, gamma) = find_on_screen(&frame, "gamma");
    let copied = drag_copy(&mut view, &mut app, (0, alpha), (1, gamma));
    assert_eq!(copied, "alpha row\n\nbeta row");

    let frame = frame_at(&mut view, &mut app, 40, 30);
    assert_eq!(inverted_cols(&frame, gamma as usize), Vec::<usize>::new());
    assert_eq!(
        inverted_cols(&frame, alpha as usize),
        (3..12).collect::<Vec<_>>()
    );
}

/// Lay out every node that has only an estimate, as idle time does.
fn settle(view: &mut FullscreenView, app: &mut OilChatApp) {
    while view.has_idle_work() {
        view.lay_out_idle(app, Duration::ZERO);
    }
}

/// The transcript rows on screen, without the last row: the label of a
/// scrolled view draws over it.
fn transcript_on_screen(view: &FullscreenView, frame: &Frame) -> Vec<String> {
    let rows = screen_text(frame);
    let area = view.area;
    rows[area.top..area.top + area.height - 1].to_vec()
}

/// Every row of the transcript as text, after every node is laid out.
fn exact_rows(exchanges: usize, width: u16, height: u16) -> Vec<String> {
    let mut app = fixtures::app_with_exchanges(exchanges);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, width, height);
    settle(&mut view, &mut app);
    let transcript = view.transcript();
    (0..transcript.len())
        .map(|i| {
            transcript
                .row(i)
                .map(|row| {
                    crucible_oil::ansi::strip_ansi(row.ansi)
                        .trim_end()
                        .to_string()
                })
                .unwrap_or_default()
        })
        .collect()
}

/// Where `window` starts in `rows`, when it occurs once.
fn window_at(rows: &[String], window: &[String]) -> usize {
    let at: Vec<usize> = (0..=rows.len().saturating_sub(window.len()))
        .filter(|&i| rows[i..i + window.len()] == *window)
        .collect();
    assert_eq!(
        at.len(),
        1,
        "the screen occurs once in the exact rows: {window:#?}"
    );
    at[0]
}

/// Problem 1 of the prototype: a width change laid out all 230 nodes of a
/// 5,000-row transcript again (160–215 ms). It must lay out only the nodes
/// that fill the screen, from the bottom up.
#[test]
fn a_width_change_lays_out_only_the_nodes_on_screen() {
    fn nodes_on_screen(view: &FullscreenView) -> u64 {
        let top = view.scroll().top();
        view.transcript()
            .entries_in(top..top + view.area.height)
            .len() as u64
    }

    let mut app = fixtures::app_with_exchanges(115);
    let mut view = FullscreenView::new();
    // The first frame has no rows kept yet: each node has an estimate of
    // one row, and still only the nodes on screen are laid out.
    frame_at(&mut view, &mut app, 200, 60);
    assert_eq!(
        app.transcript_layouts(),
        nodes_on_screen(&view),
        "first frame"
    );
    settle(&mut view, &mut app);
    assert!(view.transcript().len() > 4_900, "about 5,000 rows");

    let before = app.transcript_layouts();
    let frame = frame_at(&mut view, &mut app, 160, 60);
    let laid_out = app.transcript_layouts() - before;
    assert_eq!(
        laid_out,
        nodes_on_screen(&view),
        "{laid_out} nodes laid out for one screen"
    );
    assert!(view.scroll().follows());

    // The screen is the one that a full layout gives.
    let mut full_app = fixtures::app_with_exchanges(115);
    let mut full = FullscreenView::new();
    frame_at(&mut full, &mut full_app, 160, 60);
    settle(&mut full, &mut full_app);
    assert_eq!(
        screen_text(&frame),
        screen_text(&frame_at(&mut full, &mut full_app, 160, 60))
    );
}

/// The place of a reader is a node and a row in it. Idle time replaces the
/// estimates above the screen, and the text on screen does not move.
#[test]
fn a_width_change_keeps_the_top_on_its_node_while_estimates_are_replaced() {
    let mut app = fixtures::app_with_exchanges(40);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 160, 40);
    settle(&mut view, &mut app);
    for _ in 0..20 {
        view.handle_event(&key(KeyCode::PageUp), &mut app);
    }
    frame_at(&mut view, &mut app, 160, 40);
    let held = top_text(&view);

    let before = app.transcript_layouts();
    let frame = frame_at(&mut view, &mut app, 110, 40);
    assert!(view.has_idle_work(), "nodes off screen wait for idle time");
    let place = view.transcript().anchor_at(view.scroll().top());
    let screen = transcript_on_screen(&view, &frame);

    settle(&mut view, &mut app);
    let frame = frame_at(&mut view, &mut app, 110, 40);
    assert!(
        app.transcript_layouts() - before > 30,
        "idle time laid out the rest"
    );
    assert_eq!(view.transcript().anchor_at(view.scroll().top()), place);
    assert_eq!(transcript_on_screen(&view, &frame), screen);
    assert!(!view.scroll().follows());

    frame_at(&mut view, &mut app, 160, 40);
    assert_eq!(
        top_text(&view),
        held,
        "back at the first width, the same top row"
    );
}

/// A scroll into nodes that have only an estimate lays them out before it
/// moves, so each page is the exact page above the last one.
#[test]
fn a_scroll_into_nodes_without_rows_lays_them_out_first() {
    let (width, height) = (130, 50);
    let exact = exact_rows(40, width, height);
    let mut app = fixtures::app_with_exchanges(40);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 200, height);
    settle(&mut view, &mut app);
    let frame = frame_at(&mut view, &mut app, width, height);
    let resized = app.transcript_layouts();
    let mut at = window_at(&exact, &transcript_on_screen(&view, &frame));

    let page = view.area.height - 1;
    for _ in 0..12 {
        view.handle_event(&key(KeyCode::PageUp), &mut app);
        let top = view.scroll().top();
        assert!(
            view.transcript()
                .estimated_in(top..top + view.area.height)
                .is_empty(),
            "the event laid out the nodes that came into range"
        );
        let frame = frame_at(&mut view, &mut app, width, height);
        let now = window_at(&exact, &transcript_on_screen(&view, &frame));
        assert_eq!(now, at - page, "one exact page up");
        at = now;
    }
    assert!(
        app.transcript_layouts() > resized,
        "the scroll laid out nodes"
    );
}

/// A copy that reaches nodes without rows lays them out first, so the
/// copied text is the text of a full layout.
#[test]
fn a_copy_that_reaches_nodes_without_rows_lays_them_out_first() {
    fn select_to_screen_end(view: &mut FullscreenView) {
        let width = view.transcript().width() as usize;
        let end = Point {
            row: view.scroll().top() + view.area.height - 1,
            col: width,
        };
        let transcript = &view.transcript;
        let mut selection = Selection::start(Point { row: 0, col: 0 }, Unit::Cell, width, |r| {
            transcript.row(r)
        });
        selection.extend(end, width, |r| transcript.row(r));
        view.selection = Some(selection);
    }

    let mut app = fixtures::app_with_exchanges(20);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 120, 40);
    settle(&mut view, &mut app);
    frame_at(&mut view, &mut app, 90, 40);
    assert!(view.has_idle_work());
    select_to_screen_end(&mut view);
    let copied = view.selected_text(&mut app).unwrap();

    let mut full_app = fixtures::app_with_exchanges(20);
    let mut full = FullscreenView::new();
    frame_at(&mut full, &mut full_app, 90, 40);
    settle(&mut full, &mut full_app);
    frame_at(&mut full, &mut full_app, 90, 40);
    select_to_screen_end(&mut full);
    let expected = full.selected_text(&mut full_app).unwrap();

    assert!(expected.contains("Question 0") && expected.contains("Answer 19"));
    assert_eq!(copied, expected);
}

/// The exit dump lays out every node that has only an estimate.
#[test]
fn the_exit_dump_after_a_width_change_is_the_full_layout() {
    let mut app = fixtures::app_with_exchanges(20);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 120, 40);
    settle(&mut view, &mut app);
    frame_at(&mut view, &mut app, 90, 40);
    let rows = view.take_dump(&mut app, true);
    assert_eq!(rows.len(), view.transcript().len());

    let mut full_app = fixtures::app_with_exchanges(20);
    let mut full = FullscreenView::new();
    frame_at(&mut full, &mut full_app, 90, 40);
    settle(&mut full, &mut full_app);
    assert_eq!(rows, full.take_dump(&mut full_app, true));
}

/// Idle time lays out the rest one batch at a time, nearest the screen
/// first, and the screen does not change.
#[test]
fn idle_time_lays_out_the_rest_in_batches_without_moving_the_screen() {
    let mut app = fixtures::app_with_exchanges(20);
    let mut view = FullscreenView::new();
    frame_at(&mut view, &mut app, 120, 40);
    settle(&mut view, &mut app);
    let screen = screen_text(&frame_at(&mut view, &mut app, 90, 40));

    let before = app.transcript_layouts();
    let mut batches = 0;
    while view.has_idle_work() {
        view.lay_out_idle(&mut app, Duration::ZERO);
        batches += 1;
        assert!(batches <= 40, "idle work ends");
    }
    assert!(batches > 30, "{batches} batches for 40 nodes");
    assert_eq!(
        app.transcript_layouts() - before,
        batches,
        "a zero budget lays out one node per batch"
    );
    assert_eq!(screen_text(&frame_at(&mut view, &mut app, 90, 40)), screen);
}

/// A selection is in row numbers. Idle time lays out the nodes above it
/// while the button is down, and the selection must stay on its text.
#[test]
fn idle_time_during_a_drag_keeps_the_selection_on_its_text() {
    use crossterm::event::MouseButton;
    let needle = "Item 3: a list entry";
    let copy_after = |idle: bool| {
        let mut app = fixtures::app_with_exchanges(20);
        let mut view = FullscreenView::new();
        frame_at(&mut view, &mut app, 120, 40);
        settle(&mut view, &mut app);
        let frame = frame_at(&mut view, &mut app, 90, 40);
        let (col, row) = find_on_screen(&frame, needle);
        let end = col + needle.chars().count() as u16;
        view.handle_event(
            &mouse(MouseEventKind::Down(MouseButton::Left), col, row),
            &mut app,
        );
        view.handle_event(
            &mouse(MouseEventKind::Drag(MouseButton::Left), end, row),
            &mut app,
        );
        if idle {
            assert!(view.has_idle_work());
            settle(&mut view, &mut app);
        }
        match view.handle_event(
            &mouse(MouseEventKind::Up(MouseButton::Left), end, row),
            &mut app,
        ) {
            ViewAction::Copy(text) => text,
            other => panic!("a drag must copy, got {other:?}"),
        }
    };
    let copied = copy_after(false);
    assert!(copied.starts_with(needle), "{copied:?}");
    assert_eq!(copy_after(true), copied);
}
