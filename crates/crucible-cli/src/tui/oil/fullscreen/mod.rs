//! The full-screen mode of the chat TUI, the default of `cru chat`.
//!
//! The native (inline) mode, `cru chat --inline`, prints the transcript
//! into the main screen and lets the terminal own the scroll. This mode
//! draws on the alternate screen, so the app owns the scroll, selection,
//! copy and the exit history. Both modes draw the same `ChatNode`s with the
//! same oil primitives.
//!
//! A frame has three parts: the chrome above the transcript (status regions),
//! the transcript area, and the chrome below it (the prompt and the status
//! regions). The chrome is laid out every frame; it is small. The transcript
//! rows come from the app's kept rows, the cache that the native view uses:
//! [`transcript::Transcript`] numbers them, and the reader's place picks the
//! rows on screen.
//!
//! A width change does not lay out the whole transcript: at 5,000 rows that
//! took 160–215 ms. A frame lays out only the nodes with a row on screen,
//! from the reader's place. The other nodes have an estimate of their
//! height. A scroll lays out the nodes that come on screen before it moves,
//! a copy or a dump lays out the nodes that it reaches, and the runner lays
//! out the rest between frames ([`FullscreenView::lay_out_idle`]).

pub mod clipboard;
#[cfg(test)]
pub(crate) mod fixtures;
pub mod scroll;
pub mod selection;
pub mod shell;
pub mod transcript;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::event::Event;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use crucible_oil::cell_grid::CellGrid;
use crucible_oil::node::{col, Node};
use crucible_oil::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crucible_oil::render::{render_tree_to_grid, NATURAL_HEIGHT};
use crucible_oil::style::Gap;
use scroll::Scroll;
use selection::{highlight_cols, selected_text, text_span, Point, Selection, Unit};
use std::ops::Range;
use std::time::{Duration, Instant};
use transcript::{Anchor, Transcript};

/// Rows one wheel step moves.
const WHEEL_ROWS: isize = 3;
/// Presses closer together than this count as a double or triple click.
const MULTI_CLICK: Duration = Duration::from_millis(500);
/// The time for one batch of layouts between frames. A key that arrives
/// during a batch waits for it, so a batch stays well under a 16 ms frame.
pub const IDLE_BUDGET: Duration = Duration::from_millis(4);

/// One full-screen frame: the cells of the whole screen and the cursor.
pub struct Frame {
    pub grid: CellGrid,
    pub cursor: Option<(u16, u16)>,
}

/// What the runner must do after the view took an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewAction {
    /// The event is not for the view; give it to the app.
    Ignored,
    /// The view changed; draw a frame.
    Handled,
    /// A selection ended; copy this text.
    Copy(String),
    /// Print these rows to the main screen, into the terminal's scrollback.
    Dump(Vec<String>),
    /// Turn mouse reporting on or off, so the terminal's own selection
    /// works again.
    ToggleMouse,
}

/// Prints the transcript into the terminal's scrollback.
pub const DUMP_KEY: KeyCode = KeyCode::F(3);
/// Turns mouse reporting on and off.
pub const MOUSE_KEY: KeyCode = KeyCode::F(2);

/// Counts presses on one cell in quick succession.
#[derive(Debug, Default)]
struct Clicks {
    last: Option<(Instant, u16, u16)>,
    count: u8,
}

impl Clicks {
    fn press(&mut self, column: u16, row: u16, now: Instant) -> u8 {
        let repeat = self.last.is_some_and(|(at, c, r)| {
            now.duration_since(at) < MULTI_CLICK && c == column && r == row
        });
        self.count = if repeat { self.count % 3 + 1 } else { 1 };
        self.last = Some((now, column, row));
        self.count
    }
}

/// Where the transcript sits on the screen at the last frame.
#[derive(Debug, Default, Clone, Copy)]
struct Area {
    top: usize,
    height: usize,
}

/// Where the reader is in the transcript.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// At the bottom, following new rows.
    #[default]
    Bottom,
    /// The top row of the screen is a row of a node, so a layout of the
    /// nodes above it does not move the text on screen. Only a scroll sets
    /// it. A reflow maps from the anchor of the last scroll, not from the
    /// last reflow, so a drag of the window edge does not drift: each
    /// mapping rounds, and rounding again from a rounded row walks the
    /// reader away. A place in the last page does not follow, so the next
    /// reflow still knows it.
    At(Anchor),
}

/// The display state of the full-screen mode for one chat session.
#[derive(Default)]
pub struct FullscreenView {
    transcript: Transcript,
    place: Place,
    area: Area,
    selection: Option<Selection>,
    /// Transcript entries already printed to the main screen, so neither the
    /// dump key nor the exit prints one twice.
    dumped: usize,
    /// Whether the left button is down after a press in the transcript.
    dragging: bool,
    /// Whether the pointer moved since the press.
    moved: bool,
    clicks: Clicks,
}

impl FullscreenView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    /// The scroll position at the current layout.
    pub fn scroll(&self) -> Scroll {
        Scroll::at(self.top(), self.place == Place::Bottom)
    }

    /// The first transcript row on screen.
    fn top(&self) -> usize {
        let bottom = self.transcript.len().saturating_sub(self.area.height);
        match self.place {
            Place::Bottom => bottom,
            Place::At(anchor) => self.transcript.resolve(anchor).min(bottom),
        }
    }

    /// The transcript rows on screen.
    fn screen_rows(&self) -> Range<usize> {
        let top = self.top();
        top..top + self.area.height
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    /// Build the frame for `app` at the terminal size in `ctx`.
    pub fn frame(&mut self, app: &mut OilChatApp, ctx: &ViewContext<'_>) -> Frame {
        let (width, height) = ctx.terminal_size;
        if let Some(modal) = app.modal_view(ctx) {
            return compose(&modal, width, height);
        }
        let ctx = app.frame_context(ctx);
        let chrome = app.chrome(&ctx);

        // The native view stacks these with `gap(1)`; keep its spacing.
        let top = (!chrome.top.is_empty())
            .then(|| render_tree_to_grid(&col(chrome.top).gap(Gap::row(1)), width, NATURAL_HEIGHT));
        let bottom_tree = col(std::iter::once(chrome.footer).chain(chrome.bottom)).gap(Gap::row(1));
        let bottom = render_tree_to_grid(&bottom_tree, width, NATURAL_HEIGHT);

        let height = height as usize;
        let top_rows = top.as_ref().map_or(0, |t| t.grid.height());
        let area_top = if top_rows > 0 { top_rows + 1 } else { 0 };
        let bottom_rows = bottom.grid.height().min(height);
        let area_bottom = height.saturating_sub(bottom_rows + 1).max(area_top);
        self.area = Area {
            top: area_top,
            height: area_bottom - area_top,
        };

        self.sync_transcript(app, &ctx);

        let mut grid = CellGrid::new(width as usize, height);
        if let Some(top) = &top {
            for y in 0..top_rows.min(height) {
                grid.copy_row_from(y, &top.grid, y);
            }
        }
        self.draw_transcript(&mut grid);
        let bottom_start = height - bottom_rows;
        let bottom_skip = bottom.grid.height() - bottom_rows;
        for y in 0..bottom_rows {
            grid.copy_row_from(bottom_start + y, &bottom.grid, bottom_skip + y);
        }
        draw_overlays(&mut grid, &chrome.overlay, width);

        let cursor = bottom.cursor.and_then(|(x, y)| {
            let y = (y as usize).checked_sub(bottom_skip)?;
            Some((x, (bottom_start + y) as u16))
        });
        Frame { grid, cursor }
    }

    /// Bring the transcript up to date, and lay out the nodes on screen.
    /// A width change reflows only those; the place keeps the reader at the
    /// same text.
    fn sync_transcript(&mut self, app: &mut OilChatApp, ctx: &ViewContext<'_>) {
        if self.transcript.width() != ctx.terminal_size.0 {
            // The selected cells moved; a selection must not grab others.
            self.selection = None;
            self.dragging = false;
        }
        self.transcript.sync(app, ctx);
        self.lay_out_rows(app, Self::screen_rows);
    }

    /// Lay out every node that has only an estimate and a row in
    /// `rows(self)`, one node at a time, the node nearest the place first:
    /// from the bottom up, or out from the anchored node. A layout changes
    /// the range, so `rows` is read again after each node. An estimate that
    /// is too small puts too many nodes in the range; this order lays out
    /// only the nodes that the exact rows need. Each pass lays out one
    /// node, so this ends.
    fn lay_out_rows(&mut self, app: &mut OilChatApp, rows: impl Fn(&Self) -> Range<usize>) {
        let near = match self.place {
            Place::Bottom => self.transcript.entry_count(),
            Place::At(anchor) => anchor.entry(),
        };
        loop {
            let missing = self.transcript.estimated_in(rows(self));
            let Some(&entry) = missing.iter().min_by_key(|&&e| e.abs_diff(near)) else {
                return;
            };
            self.lay_out(app, &[entry]);
        }
    }

    /// Lay out `entries`, and keep the selection on its text: a selection
    /// is in row numbers, and a layout moves the rows below it.
    fn lay_out(&mut self, app: &mut OilChatApp, entries: &[usize]) {
        let before = self
            .selection
            .is_some()
            .then(|| self.transcript.index().clone());
        self.transcript.lay_out(app, entries);
        if let (Some(before), Some(selection)) = (before, self.selection.as_mut()) {
            let transcript = &self.transcript;
            selection.map_rows(|row| transcript.relocate(&before, row));
        }
    }

    /// Whether some nodes have only an estimate of their height.
    pub fn has_idle_work(&self) -> bool {
        self.transcript.estimated() > 0
    }

    /// Lay out nodes that have only an estimate, nearest the screen first,
    /// for about `budget`: at least one node. The runner calls this between
    /// frames when no input and no stream is pending, so the estimates
    /// soon become exact. The text on screen does not move. Returns whether
    /// work is left.
    pub fn lay_out_idle(&mut self, app: &mut OilChatApp, budget: Duration) -> bool {
        let deadline = Instant::now() + budget;
        let near = self.transcript.entries_in(self.screen_rows()).start;
        while let Some(entry) = self.transcript.nearest_estimated(near) {
            self.lay_out(app, &[entry]);
            if Instant::now() >= deadline {
                break;
            }
        }
        self.has_idle_work()
    }

    fn draw_transcript(&self, grid: &mut CellGrid) {
        let top = self.top();
        let transcript = &self.transcript;
        let width = transcript.width() as usize;
        let rows = |r: usize| transcript.row(r);
        // The copy reads the same text span, so the highlight shows what a
        // copy takes.
        let span = self
            .selection
            .and_then(|s| text_span(s.bounds(), width, rows));
        for y in 0..self.area.height {
            if let Some(row) = transcript.row(top + y) {
                grid.blit_line(row.ansi, 0, self.area.top + y);
            }
            if let Some(cols) = span.and_then(|span| highlight_cols(span, top + y, width, rows)) {
                grid.invert(self.area.top + y, cols);
            }
        }
        if self.place != Place::Bottom && self.area.height > 0 {
            let below = self.transcript.len().saturating_sub(top + self.area.height);
            let label = format!(" \u{2193} {below} rows below \u{b7} PgDn ");
            let x = grid
                .width()
                .saturating_sub(unicode_width::UnicodeWidthStr::width(label.as_str()));
            let y = self.area.top + self.area.height - 1;
            grid.blit_line(&format!("\x1b[7m{label}\x1b[0m"), x, y);
        }
    }

    /// The rows not yet printed to the main screen, and mark them printed.
    ///
    /// Only finished entries print, unless `unfinished` is set: a node that
    /// still changes has no final rows yet. The exit sets it, because nothing
    /// changes after the exit. Rows keep the width of the last frame. The
    /// nodes that have only an estimate are laid out first.
    pub fn take_dump(&mut self, app: &mut OilChatApp, unfinished: bool) -> Vec<String> {
        let end = if unfinished {
            self.transcript.entry_count()
        } else {
            self.transcript.finished_prefix()
        };
        if end <= self.dumped {
            return Vec::new();
        }
        let entries: Vec<usize> = (self.dumped..end).collect();
        self.lay_out(app, &entries);
        let rows = self.transcript.styled_rows(self.dumped..end);
        self.dumped = end;
        rows
    }

    /// Take a scroll key or a mouse report. Everything else goes to the app.
    pub fn handle_event(&mut self, event: &Event, app: &mut OilChatApp) -> ViewAction {
        // A modal or a permission prompt owns the keys, PageUp included.
        if app.has_fullscreen_modal() || app.interaction_visible() {
            return ViewAction::Ignored;
        }
        match event {
            Event::Key(key) => self.handle_key(key, app),
            Event::Mouse(mouse) => self.handle_mouse(mouse, app),
            _ => ViewAction::Ignored,
        }
    }

    fn handle_key(&mut self, key: &KeyEvent, app: &mut OilChatApp) -> ViewAction {
        let page = self.area.height.saturating_sub(1).max(1) as isize;
        match key.code {
            KeyCode::Esc if self.selection.is_some() => {
                self.selection = None;
                self.dragging = false;
            }
            DUMP_KEY => return ViewAction::Dump(self.take_dump(app, false)),
            MOUSE_KEY => return ViewAction::ToggleMouse,
            KeyCode::PageUp => self.scroll_by(app, -page),
            KeyCode::PageDown => self.scroll_by(app, page),
            _ => return ViewAction::Ignored,
        }
        ViewAction::Handled
    }

    fn handle_mouse(&mut self, mouse: &MouseEvent, app: &mut OilChatApp) -> ViewAction {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_by(app, -WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.scroll_by(app, WHEEL_ROWS),
            MouseEventKind::Down(MouseButton::Left) => return self.press(mouse),
            MouseEventKind::Drag(MouseButton::Left) if self.dragging => self.drag(mouse, app),
            MouseEventKind::Up(MouseButton::Left) if self.dragging => return self.release(app),
            _ => return ViewAction::Ignored,
        }
        ViewAction::Handled
    }

    /// The buffer cell under a screen cell in the transcript area.
    fn point_at(&self, column: u16, row: u16) -> Option<Point> {
        let y = (row as usize).checked_sub(self.area.top)?;
        (y < self.area.height).then(|| Point {
            row: self.top() + y,
            col: column as usize,
        })
    }

    fn press(&mut self, mouse: &MouseEvent) -> ViewAction {
        let Some(point) = self.point_at(mouse.column, mouse.row) else {
            // A press outside the transcript clears the selection and goes
            // on to the app.
            self.selection = None;
            return ViewAction::Ignored;
        };
        let clicks = self.clicks.press(mouse.column, mouse.row, Instant::now());
        let transcript = &self.transcript;
        self.selection = Some(Selection::start(
            point,
            Unit::from_clicks(clicks),
            transcript.width() as usize,
            |r| transcript.row(r),
        ));
        self.dragging = true;
        self.moved = false;
        ViewAction::Handled
    }

    /// Extend the selection to the pointer. Past the top or bottom edge of
    /// the transcript, scroll one row toward the pointer.
    fn drag(&mut self, mouse: &MouseEvent, app: &mut OilChatApp) {
        let row = mouse.row as usize;
        if row < self.area.top {
            self.scroll_by(app, -1);
        } else if row >= self.area.top + self.area.height {
            self.scroll_by(app, 1);
        }
        let last = (self.area.top + self.area.height).saturating_sub(1);
        let clamped = row.clamp(self.area.top, last.max(self.area.top)) as u16;
        let Some(point) = self.point_at(mouse.column, clamped) else {
            return;
        };
        let transcript = &self.transcript;
        if let Some(selection) = self.selection.as_mut() {
            selection.extend(point, transcript.width() as usize, |r| transcript.row(r));
        }
        self.moved = true;
    }

    /// End the gesture. A plain click selects nothing; anything else
    /// copies, as a terminal does.
    fn release(&mut self, app: &mut OilChatApp) -> ViewAction {
        self.dragging = false;
        let Some(selection) = self.selection else {
            return ViewAction::Handled;
        };
        if selection.unit() == Unit::Cell && !self.moved {
            self.selection = None;
            return ViewAction::Handled;
        }
        match self.selected_text(app) {
            Some(text) if !text.is_empty() => ViewAction::Copy(text),
            _ => ViewAction::Handled,
        }
    }

    /// The text of the selection, as the source had it. A copy must be
    /// exact, so the nodes in the selection are laid out first.
    pub fn selected_text(&mut self, app: &mut OilChatApp) -> Option<String> {
        self.lay_out_rows(app, |view| {
            view.selection.map_or(0..0, |selection| {
                let bounds = selection.bounds();
                bounds.start.row..bounds.end.row + 1
            })
        });
        let selection = self.selection?;
        let transcript = &self.transcript;
        Some(selected_text(
            selection.bounds(),
            transcript.width() as usize,
            |r| transcript.row(r),
        ))
    }

    /// Scroll by `delta` rows; negative scrolls toward the start. The
    /// nodes on screen after the move are laid out before it, so the move
    /// counts their real rows. A scroll that reaches the bottom follows.
    fn scroll_by(&mut self, app: &mut OilChatApp, delta: isize) {
        let moved = move |view: &Self| {
            let top = view.moved_top(delta);
            top..top + view.area.height
        };
        self.lay_out_rows(app, moved);
        let top = self.moved_top(delta);
        let bottom = self.transcript.len().saturating_sub(self.area.height);
        // Wheel scrolls do not end a selection: it lives in buffer rows.
        self.place = match self.transcript.anchor_at(top) {
            Some(anchor) if top < bottom => Place::At(anchor),
            _ => Place::Bottom,
        };
    }

    /// The top row after a scroll by `delta` rows at the current layout.
    fn moved_top(&self, delta: isize) -> usize {
        let bottom = self.transcript.len().saturating_sub(self.area.height);
        self.top().saturating_add_signed(delta).min(bottom)
    }
}

/// Lay `tree` out into a `width` x `height` screen. Content taller than the
/// screen shows its bottom rows. Overlays draw over the result.
fn compose(tree: &Node, width: u16, height: u16) -> Frame {
    let main = filter_overlays(tree.clone());
    let rendered = render_tree_to_grid(&main, width, height);
    let mut grid = CellGrid::new(width as usize, height as usize);
    let skip = rendered.grid.height().saturating_sub(height as usize);
    for y in 0..(height as usize).min(rendered.grid.height()) {
        grid.copy_row_from(y, &rendered.grid, skip + y);
    }
    let cursor = rendered.cursor.and_then(|(x, y)| {
        let y = (y as usize).checked_sub(skip)?;
        (y < height as usize).then_some((x, y as u16))
    });
    draw_overlays(&mut grid, tree, width);
    Frame { grid, cursor }
}

/// Draw the overlays in `tree` over `grid`, anchored to its bottom.
fn draw_overlays(grid: &mut CellGrid, tree: &Node, width: u16) {
    let height = grid.height();
    for overlay in extract_overlays(tree) {
        let child = render_tree_to_grid(&overlay.child, width, NATURAL_HEIGHT);
        let OverlayAnchor::FromBottom(offset) = overlay.anchor;
        let bottom = height.saturating_sub(offset);
        let top = bottom.saturating_sub(child.grid.content_height());
        for (i, y) in (top..bottom).enumerate() {
            grid.overlay_row_from(y, &child.grid, i);
        }
    }
}

#[cfg(test)]
mod bench;
#[cfg(test)]
mod tests;
