//! The full-screen mode of the chat TUI (prototype, opt-in with
//! `cru chat --fullscreen`).
//!
//! The native mode prints the transcript into the main screen and lets the
//! terminal own the scroll. This mode draws on the alternate screen, so the
//! app owns the scroll, selection, copy and the exit history. Both modes draw
//! the same `ChatNode`s with the same oil primitives.

pub mod fixtures;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::chat_app::OilChatApp;
use crucible_oil::cell_grid::CellGrid;
use crucible_oil::node::Node;
use crucible_oil::overlay::{extract_overlays, filter_overlays, OverlayAnchor};
use crucible_oil::render::{render_tree_to_grid, NATURAL_HEIGHT};

/// One full-screen frame: the cells of the whole screen and the cursor.
pub struct Frame {
    pub grid: CellGrid,
    pub cursor: Option<(u16, u16)>,
}

/// The display state of the full-screen mode for one chat session.
#[derive(Default)]
pub struct FullscreenView {}

impl FullscreenView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the frame for `app` at the terminal size in `ctx`.
    ///
    /// This first version lays the whole view out every frame, transcript
    /// included, and shows its bottom rows.
    pub fn frame(&mut self, app: &OilChatApp, ctx: &ViewContext<'_>) -> Frame {
        let (width, height) = ctx.terminal_size;
        let tree = app.view(ctx);
        compose(&tree, width, height)
    }
}

/// Lay `tree` out into a `width` x `height` screen. Content taller than the
/// screen shows its bottom rows. Overlays draw over the result.
fn compose(tree: &Node, width: u16, height: u16) -> Frame {
    let overlays = extract_overlays(tree);
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
    for overlay in overlays {
        let child = render_tree_to_grid(&overlay.child, width, NATURAL_HEIGHT);
        let OverlayAnchor::FromBottom(offset) = overlay.anchor;
        let bottom = (height as usize).saturating_sub(offset);
        let top = bottom.saturating_sub(child.grid.content_height());
        for (i, y) in (top..bottom).enumerate() {
            grid.overlay_row_from(y, &child.grid, i);
        }
    }
    Frame { grid, cursor }
}

#[cfg(test)]
mod bench;
