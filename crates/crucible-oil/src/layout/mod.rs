mod debug;
pub mod flex;
mod query;
mod tree_render;
mod types;

pub use crate::taffy_layout::{build_layout_tree, build_layout_tree_with_engine, LayoutEngine};
pub use tree_render::render_layout_tree;
pub use types::{LayoutBox, LayoutContent, LayoutTree, PopupItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}
