//! Ink-style declarative TUI framework
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

#![allow(dead_code)]

mod ansi;
mod compositor;
mod diff;
mod focus;
mod layout;
mod line_buffer;
mod node;
mod overlay;
#[cfg(any(test, feature = "test-utils"))]
pub mod proptest_strategies;
mod render;
mod span;
mod style;
mod taffy_layout;
pub mod template;

pub use compositor::{Compositor, ContentSource, StaticCompositor};
pub use diff::*;
pub use focus::*;
pub use layout::*;
pub use line_buffer::{DiffOp, LineBuffer, LineDiff, RenderedLine};
pub use node::*;
pub use overlay::{composite_overlays, Overlay, OverlayAnchor};
pub use render::*;
pub use span::{OwnedSpan, OwnedSpanLine, Span, SpanLine};
pub use style::*;

pub mod utils {
    pub use crate::ansi::{strip_ansi, visible_width, visual_rows};
}
