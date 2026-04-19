//! Markdown to Oil Node renderer
//!
//! Parses markdown using markdown-it and converts to oil Node trees
//! for styled terminal rendering.
//!
//! # Render Styles
//!
//! Two render styles are supported:
//!
//! - **Viewport**: Content is pre-wrapped to fit the terminal width. Use for content
//!   that will be rendered in-place and redrawn (streaming, popups).
//!
//! - **Natural**: Text uses large width (terminal wraps), but tables use terminal width
//!   for correct column sizing. Use for graduated/scrollback content that won't be redrawn.

use crucible_oil::node::*;
use once_cell::sync::Lazy;
use regex::Regex;

mod blockquote;
mod code;
mod context;
mod list;
mod render;
mod table;

use context::parse_and_render_internal;

#[cfg(test)]
mod tests;

#[allow(dead_code)] // WIP: NATURAL_TEXT_WIDTH not yet used
const NATURAL_TEXT_WIDTH: usize = 10000;

/// Regex to match HTML <br> tags in various forms: <br>, <br/>, <br />, <BR>, etc.
static BR_TAG_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)<br\s*/?\s*>").expect("valid regex"));

pub const ASSISTANT_BULLET: &str = " ● ";
pub const ASSISTANT_BULLET_WIDTH: usize = 3;
pub const CONTENT_PADDING: usize = ASSISTANT_BULLET_WIDTH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Margins {
    pub left: usize,
    pub right: usize,
    pub show_bullet: bool,
}

impl Margins {
    pub fn assistant() -> Self {
        Self {
            left: CONTENT_PADDING,
            right: CONTENT_PADDING,
            show_bullet: true,
        }
    }

    pub fn assistant_continuation() -> Self {
        Self {
            left: CONTENT_PADDING,
            right: CONTENT_PADDING,
            show_bullet: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderStyle {
    /// Pre-wrap all content to terminal width. For viewport/streaming content.
    Viewport { width: usize, margins: Margins },
    /// Pre-wrap to terminal width for consistent left/right alignment.
    /// For graduated/scrollback content.
    Natural {
        terminal_width: usize,
        margins: Margins,
    },
}

impl RenderStyle {
    pub fn viewport(width: usize) -> Self {
        RenderStyle::Viewport {
            width,
            margins: Margins::default(),
        }
    }

    pub fn viewport_with_margins(width: usize, margins: Margins) -> Self {
        RenderStyle::Viewport { width, margins }
    }

    pub fn natural(terminal_width: usize) -> Self {
        RenderStyle::Natural {
            terminal_width,
            margins: Margins::default(),
        }
    }

    pub fn natural_with_margins(terminal_width: usize, margins: Margins) -> Self {
        RenderStyle::Natural {
            terminal_width,
            margins,
        }
    }

    fn text_width(&self) -> usize {
        match self {
            RenderStyle::Viewport { width, margins }
            | RenderStyle::Natural {
                terminal_width: width,
                margins,
            } => width.saturating_sub(margins.left + margins.right),
        }
    }

    fn table_width(&self) -> usize {
        match self {
            RenderStyle::Viewport { width, margins } => {
                width.saturating_sub(margins.left + margins.right)
            }
            RenderStyle::Natural {
                terminal_width,
                margins,
            } => terminal_width.saturating_sub(margins.left + margins.right),
        }
    }

    fn margins(&self) -> Margins {
        match self {
            RenderStyle::Viewport { margins, .. } | RenderStyle::Natural { margins, .. } => {
                *margins
            }
        }
    }

    fn blockquote_width(&self) -> usize {
        self.table_width()
    }
}

/// Convert markdown text to an oil Node tree
pub fn markdown_to_node(markdown: &str) -> Node {
    markdown_to_node_with_width(markdown, 80)
}

/// Convert markdown text to an oil Node tree with explicit width (viewport style)
pub fn markdown_to_node_with_width(markdown: &str, width: usize) -> Node {
    markdown_to_node_styled(markdown, RenderStyle::viewport(width))
}

/// Convert markdown with explicit render style
pub fn markdown_to_node_styled(markdown: &str, style: RenderStyle) -> Node {
    parse_and_render_internal(
        markdown,
        style.text_width(),
        style.table_width(),
        style.blockquote_width(),
        style.margins(),
    )
}

/// Convert markdown text to an oil Node tree with separate widths for text and tables.
/// Prefer `markdown_to_node_styled` for clearer intent.
pub fn markdown_to_node_with_widths(markdown: &str, text_width: usize, table_width: usize) -> Node {
    parse_and_render_internal(
        markdown,
        text_width,
        table_width,
        table_width,
        Margins::default(),
    )
}
