//! The full-screen mode of the chat TUI (prototype, opt-in with
//! `cru chat --fullscreen`).
//!
//! The native mode prints the transcript into the main screen and lets the
//! terminal own the scroll. This mode draws on the alternate screen, so the
//! app owns the scroll, selection, copy and the exit history. Both modes draw
//! the same `ChatNode`s with the same oil primitives.
//!
//! A frame has three parts: the chrome above the transcript (status regions),
//! the transcript area, and the chrome below it (the prompt and the status
//! regions). The chrome is laid out every frame; it is small. The transcript
//! comes from [`transcript::Transcript`], which keeps the rows of every
//! finished node, and [`scroll::Scroll`] picks the rows on screen.

pub mod fixtures;
pub mod scroll;
pub mod transcript;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::event::Event;
use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use crucible_oil::cell_grid::CellGrid;
use crucible_oil::node::{col, Node};
use crucible_oil::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crucible_oil::render::{render_tree_to_grid, NATURAL_HEIGHT};
use crucible_oil::style::Gap;
use scroll::Scroll;
use transcript::Transcript;

/// Rows one wheel step moves.
const WHEEL_ROWS: isize = 3;

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
}

/// Where the transcript sits on the screen at the last frame.
#[derive(Debug, Default, Clone, Copy)]
struct Area {
    top: usize,
    height: usize,
}

/// The display state of the full-screen mode for one chat session.
#[derive(Default)]
pub struct FullscreenView {
    transcript: Transcript,
    scroll: Scroll,
    area: Area,
    /// The reader's place, taken at the first reflow after a manual scroll.
    /// Later reflows map from it, not from the last reflow, so a drag of the
    /// window edge does not drift: each mapping rounds, and rounding again
    /// from a rounded row walks the reader away.
    anchor: Option<transcript::Anchor>,
}

impl FullscreenView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lay every cached node out again at the next frame, as after a theme
    /// change.
    pub fn invalidate(&mut self) {
        self.transcript.invalidate();
    }

    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    pub fn scroll(&self) -> Scroll {
        self.scroll
    }

    /// Build the frame for `app` at the terminal size in `ctx`.
    pub fn frame(&mut self, app: &OilChatApp, ctx: &ViewContext<'_>) -> Frame {
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

    /// Bring the transcript up to date. A width change reflows it and puts
    /// the reader back at the same text.
    fn sync_transcript(&mut self, app: &OilChatApp, ctx: &ViewContext<'_>) {
        let reflow = self.transcript.width() != ctx.terminal_size.0;
        let anchor = if reflow && !self.scroll.follows() {
            self.anchor
                .or_else(|| self.transcript.anchor_at(self.scroll.top()))
        } else {
            None
        };
        self.anchor = self.anchor.or(anchor);
        self.transcript.sync(app.container_list().nodes(), ctx);
        let total = self.transcript.len();
        if let Some(anchor) = anchor {
            self.scroll
                .set_top(self.transcript.resolve(anchor), total, self.area.height);
        }
        self.scroll.fit(total, self.area.height);
    }

    fn draw_transcript(&self, grid: &mut CellGrid) {
        let top = self.scroll.top();
        for y in 0..self.area.height {
            if let Some(row) = self.transcript.row(top + y) {
                grid.blit_line(&row.ansi, 0, self.area.top + y);
            }
        }
        if !self.scroll.follows() && self.area.height > 0 {
            let below = self
                .transcript
                .len()
                .saturating_sub(top + self.area.height);
            let label = format!(" \u{2193} {below} rows below \u{b7} PgDn ");
            let x = grid
                .width()
                .saturating_sub(unicode_width::UnicodeWidthStr::width(label.as_str()));
            let y = self.area.top + self.area.height - 1;
            grid.blit_line(&format!("\x1b[7m{label}\x1b[0m"), x, y);
        }
    }

    /// Take a scroll key or a mouse report. Everything else goes to the app.
    pub fn handle_event(&mut self, event: &Event, app: &OilChatApp) -> ViewAction {
        // A modal or a permission prompt owns the keys, PageUp included.
        if app.has_fullscreen_modal() || app.interaction_visible() {
            return ViewAction::Ignored;
        }
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => ViewAction::Ignored,
        }
    }

    fn handle_key(&mut self, key: &KeyEvent) -> ViewAction {
        let page = self.area.height.saturating_sub(1).max(1) as isize;
        match key.code {
            KeyCode::PageUp => self.scroll_by(-page),
            KeyCode::PageDown => self.scroll_by(page),
            _ => return ViewAction::Ignored,
        }
        ViewAction::Handled
    }

    fn handle_mouse(&mut self, mouse: &MouseEvent) -> ViewAction {
        match mouse.kind {
            MouseEventKind::ScrollUp => self.scroll_by(-WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.scroll_by(WHEEL_ROWS),
            _ => return ViewAction::Ignored,
        }
        ViewAction::Handled
    }

    fn scroll_by(&mut self, delta: isize) {
        self.anchor = None;
        self.scroll
            .scroll_by(delta, self.transcript.len(), self.area.height);
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
