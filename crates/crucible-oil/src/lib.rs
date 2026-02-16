//! Oil-style declarative TUI framework
//!
//! A minimal Elm/Dioxus-inspired UI framework for terminal applications.
//! Key concepts:
//!
//! - **Declarative**: Build UI as a tree of nodes, framework handles rendering
//! - **Scrollback-native**: `Static` nodes graduate to terminal scrollback
//! - **Composable**: Nest nodes freely, layout is automatic
//!
//! # Architecture
//!
//! ```text
//! State -> view() -> Node tree -> Runtime -> Terminal
//!   ^                                |
//!   |                                v
//!   +---- update() <---- Event <--- Input
//! ```

pub mod ansi;
pub mod cell_grid;
pub mod components;
pub mod decrypt;
pub mod focus;
pub mod layout;
pub mod node;
pub mod overlay;
#[cfg(any(test, feature = "test-utils"))]
pub mod proptest_strategies;
pub mod render;
pub mod style;
pub mod taffy_layout;
pub mod template;

pub use cell_grid::{CellGrid, StyledCell};
pub use components::{
    clamp_input_lines, popup_item, popup_item_full, popup_item_with_desc, wrap_content, Drawer,
    DrawerKind, InputArea, InputStyle, PopupOverlay, FOCUS_POPUP, INPUT_MAX_CONTENT_LINES,
    POPUP_MAX_VISIBLE,
};
pub use focus::*;
pub use layout::flex::*;
pub use layout::Rect;
pub use layout::*;
pub use layout::{build_layout_tree, render_layout_tree, render_layout_tree_filtered};
pub use layout::{LayoutBox, LayoutContent, LayoutTree, PopupItem};
pub use node::*;
pub use overlay::{composite_overlays, Overlay, OverlayAnchor};
pub use render::*;
pub use style::*;

// Re-export commonly used decrypt functions
pub use decrypt::{decrypt_text, DecryptConfig, CIPHER_CHARS};

pub mod utils;

// Re-export utils for convenience
pub use utils::{truncate_to_chars, truncate_to_width};
