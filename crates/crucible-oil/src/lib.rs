//! Oil-style declarative TUI framework
//!
//! A minimal Elm/Dioxus-inspired UI framework for terminal applications.
//! Key concepts:
//!
//! - **Declarative**: Build UI as a tree of nodes, framework handles rendering
//! - **Drain-based graduation**: Completed content drains to stdout at the app layer
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
pub mod bounded;
pub mod cell_grid;
pub mod components;
pub mod focus;
pub mod layout;
pub mod node;
pub mod output;
pub mod overlay;
pub mod planning;
#[cfg(any(test, feature = "test-utils"))]
pub mod proptest_strategies;
pub mod render;
pub(crate) mod render_helpers;
pub mod runtime;
pub mod style;
pub mod taffy_layout;
pub mod template;
pub mod terminal;
pub mod viewport;

pub use bounded::{bounded, bounded_head};
pub use cell_grid::{CellGrid, StyledCell};
pub use components::{
    clamp_input_lines, popup_item, popup_item_full, popup_item_with_desc, wrap_content, Drawer,
    DrawerKind, InputArea, InputStyle, PopupOverlay, FOCUS_POPUP, INPUT_MAX_CONTENT_LINES,
    POPUP_MAX_VISIBLE,
};
pub use focus::{FocusContext, FocusId};
pub use layout::flex::{
    calculate_column_heights, calculate_row_widths, ChildMeasurement, FlexLayoutInput,
    FlexLayoutResult,
};
pub use layout::Rect;
pub use layout::{
    build_layout_tree, render_layout_tree, LayoutBox, LayoutContent, LayoutTree, PopupItem,
};
pub use node::{
    badge, bullet_list, col, divider, fixed, flex, fragment, horizontal_rule, if_else, key_value,
    maybe, numbered_list, overlay_from_bottom, overlay_from_bottom_right, popup, progress_bar, raw,
    row, show, spacer, spinner, styled, text, text_input, when, BoxNode, Direction, InputNode,
    Node, OverlayNode, PopupItemNode, PopupNode, RawNode, Size, SpinnerNode, TextNode,
    DEFAULT_POPUP_BG, DEFAULT_POPUP_SELECTED_BG,
};
pub use output::OutputBuffer;
pub use overlay::{composite_overlays, Overlay, OverlayAnchor};
pub use planning::{FramePlan, FramePlanner, FrameSnapshot, FrameTrace, Graduation, RenderedOverlay};
pub use render::{
    render_to_plain_text, render_to_string, render_with_cursor, CursorInfo, RenderResult,
};
pub use runtime::{FrameRenderer, TestRuntime};
pub use style::{AlignItems, Border, Color, Gap, JustifyContent, Padding, Style};
pub use terminal::Terminal;

pub mod utils;

// Re-export utils for convenience
pub use utils::{truncate_to_chars, truncate_to_width};
pub use viewport::{clamp_lines_bottom, clamp_lines_top, ensure_min_height, pad_lines_to};
