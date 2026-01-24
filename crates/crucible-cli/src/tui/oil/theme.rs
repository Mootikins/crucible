//! Semantic color tokens and style presets for the TUI.
//!
//! This module provides a single source of truth for all colors and styles used
//! throughout the chat interface. Instead of scattering `Color::Rgb(40, 44, 52)`
//! throughout the codebase, use semantic tokens like `colors::INPUT_BG`.
//!
//! # Design Principles
//!
//! 1. **Semantic naming**: Colors are named by purpose, not appearance
//! 2. **Single source of truth**: All colors defined here, used everywhere
//! 3. **Composable presets**: Common style combinations as functions
//! 4. **Future-proof**: Easy to add theming later without touching every file
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::tui::oil::theme::{colors, styles};
//!
//! // Use semantic colors
//! styled("Error!", Style::new().fg(colors::ERROR));
//!
//! // Use style presets
//! styled("Hello", styles::user_prompt());
//! styled("Response", styles::assistant_response());
//! ```

use super::style::{Color, Style};

/// Semantic color tokens.
///
/// Colors are organized by purpose:
/// - **Surfaces**: Backgrounds for different UI regions
/// - **Text**: Foreground colors for different content types  
/// - **Semantic**: Status indicators (error, warning, success)
/// - **Roles**: Chat participant colors
/// - **Modes**: Chat mode indicators
pub mod colors {
    use super::Color;

    // ─────────────────────────────────────────────────────────────────────────
    // Surfaces (backgrounds)
    // ─────────────────────────────────────────────────────────────────────────

    /// Default input field background (dark gray)
    pub const INPUT_BG: Color = Color::Rgb(40, 44, 52);

    /// Command mode input background (amber tint)
    pub const COMMAND_BG: Color = Color::Rgb(60, 50, 20);

    /// Shell mode input background (red tint)
    pub const SHELL_BG: Color = Color::Rgb(60, 30, 30);

    /// Popup/overlay background
    pub const POPUP_BG: Color = Color::Rgb(30, 34, 42);

    /// Code block background
    pub const CODE_BG: Color = Color::Rgb(35, 39, 47);

    /// Thinking block background
    pub const THINKING_BG: Color = Color::Rgb(45, 40, 55);

    // ─────────────────────────────────────────────────────────────────────────
    // Text colors
    // ─────────────────────────────────────────────────────────────────────────

    /// Primary text color
    pub const TEXT_PRIMARY: Color = Color::White;

    /// Secondary/muted text
    pub const TEXT_MUTED: Color = Color::DarkGray;

    /// Accent text (links, highlights)
    pub const TEXT_ACCENT: Color = Color::Cyan;

    /// Dimmed text (timestamps, metadata)
    pub const TEXT_DIM: Color = Color::Gray;

    // ─────────────────────────────────────────────────────────────────────────
    // Semantic colors (status indicators)
    // ─────────────────────────────────────────────────────────────────────────

    /// Success indicator
    pub const SUCCESS: Color = Color::Green;

    /// Error indicator
    pub const ERROR: Color = Color::Red;

    /// Warning indicator
    pub const WARNING: Color = Color::Yellow;

    /// Info indicator
    pub const INFO: Color = Color::Cyan;

    // ─────────────────────────────────────────────────────────────────────────
    // Chat roles
    // ─────────────────────────────────────────────────────────────────────────

    /// User message color
    pub const ROLE_USER: Color = Color::Green;

    /// Assistant message color
    pub const ROLE_ASSISTANT: Color = Color::Cyan;

    /// System message color
    pub const ROLE_SYSTEM: Color = Color::Yellow;

    /// Tool/function call color
    pub const ROLE_TOOL: Color = Color::Magenta;

    // ─────────────────────────────────────────────────────────────────────────
    // Chat modes
    // ─────────────────────────────────────────────────────────────────────────

    /// Normal mode badge background
    pub const MODE_NORMAL: Color = Color::Green;

    /// Plan mode badge background
    pub const MODE_PLAN: Color = Color::Blue;

    /// Auto mode badge background
    pub const MODE_AUTO: Color = Color::Yellow;

    // ─────────────────────────────────────────────────────────────────────────
    // UI elements
    // ─────────────────────────────────────────────────────────────────────────

    /// Spinner/loading indicator
    pub const SPINNER: Color = Color::Cyan;

    /// Selected item in popup
    pub const SELECTED: Color = Color::Cyan;

    /// Border color
    pub const BORDER: Color = Color::DarkGray;

    /// Prompt character color
    pub const PROMPT: Color = Color::Cyan;

    /// Model name in status bar
    pub const MODEL_NAME: Color = Color::Cyan;

    /// Notification text
    pub const NOTIFICATION: Color = Color::Yellow;

    // ─────────────────────────────────────────────────────────────────────────
    // Markdown rendering
    // ─────────────────────────────────────────────────────────────────────────

    /// Inline code color
    pub const CODE_INLINE: Color = Color::Yellow;

    /// Code block fallback (when no syntax highlighting)
    pub const CODE_FALLBACK: Color = Color::Green;

    /// Fence markers (```)
    pub const FENCE_MARKER: Color = Color::DarkGray;

    /// Blockquote prefix (│)
    pub const BLOCKQUOTE_PREFIX: Color = Color::DarkGray;

    /// Blockquote text
    pub const BLOCKQUOTE_TEXT: Color = Color::Gray;

    /// Link color
    pub const LINK: Color = Color::Blue;

    /// Heading level 1
    pub const HEADING_1: Color = Color::Cyan;

    /// Heading level 2
    pub const HEADING_2: Color = Color::Blue;

    /// Heading level 3
    pub const HEADING_3: Color = Color::Magenta;

    /// Bullet prefix for assistant messages
    pub const BULLET_PREFIX: Color = Color::DarkGray;
}

/// Pre-composed style presets for common UI patterns.
///
/// These combine colors with text attributes (bold, dim, etc.) for consistency.
/// Use these instead of building styles inline.
pub mod styles {
    use super::{colors, Color, Style};

    // ─────────────────────────────────────────────────────────────────────────
    // Chat roles
    // ─────────────────────────────────────────────────────────────────────────

    /// Style for user message prefix/content
    pub fn user_prompt() -> Style {
        Style::new().fg(colors::ROLE_USER).bold()
    }

    /// Style for assistant response
    pub fn assistant_response() -> Style {
        Style::new().fg(colors::ROLE_ASSISTANT)
    }

    /// Style for system messages
    pub fn system_message() -> Style {
        Style::new().fg(colors::ROLE_SYSTEM).italic()
    }

    /// Style for tool calls
    pub fn tool_call() -> Style {
        Style::new().fg(colors::ROLE_TOOL).dim()
    }

    /// Style for tool results
    pub fn tool_result() -> Style {
        Style::new().fg(colors::TEXT_MUTED)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Status indicators
    // ─────────────────────────────────────────────────────────────────────────

    /// Style for error messages
    pub fn error() -> Style {
        Style::new().fg(colors::ERROR).bold()
    }

    /// Style for warning messages
    pub fn warning() -> Style {
        Style::new().fg(colors::WARNING)
    }

    /// Style for success messages
    pub fn success() -> Style {
        Style::new().fg(colors::SUCCESS)
    }

    /// Style for info messages
    pub fn info() -> Style {
        Style::new().fg(colors::INFO)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Text variations
    // ─────────────────────────────────────────────────────────────────────────

    /// Muted/secondary text
    pub fn muted() -> Style {
        Style::new().fg(colors::TEXT_MUTED)
    }

    /// Dimmed text (less prominent than muted)
    pub fn dim() -> Style {
        Style::new().fg(colors::TEXT_DIM).dim()
    }

    /// Accent text (links, highlights)
    pub fn accent() -> Style {
        Style::new().fg(colors::TEXT_ACCENT)
    }

    /// Bold accent (important highlights)
    pub fn accent_bold() -> Style {
        Style::new().fg(colors::TEXT_ACCENT).bold()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // UI elements
    // ─────────────────────────────────────────────────────────────────────────

    /// Style for input prompt character (>, :, !)
    pub fn prompt() -> Style {
        Style::new().fg(colors::PROMPT)
    }

    /// Style for spinner/loading
    pub fn spinner() -> Style {
        Style::new().fg(colors::SPINNER)
    }

    /// Style for model name in status bar
    pub fn model_name() -> Style {
        Style::new().fg(colors::MODEL_NAME)
    }

    /// Style for notifications
    pub fn notification() -> Style {
        Style::new().fg(colors::NOTIFICATION)
    }

    /// Style for selected item (inverted)
    pub fn selected() -> Style {
        Style::new().fg(Color::Black).bg(colors::SELECTED)
    }

    /// Style for popup item description
    pub fn popup_description() -> Style {
        Style::new().fg(colors::TEXT_DIM).dim()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Mode badges
    // ─────────────────────────────────────────────────────────────────────────

    /// Style for NORMAL mode badge
    pub fn mode_normal() -> Style {
        Style::new().bg(colors::MODE_NORMAL).fg(Color::Black).bold()
    }

    /// Style for PLAN mode badge
    pub fn mode_plan() -> Style {
        Style::new().bg(colors::MODE_PLAN).fg(Color::Black).bold()
    }

    /// Style for AUTO mode badge
    pub fn mode_auto() -> Style {
        Style::new().bg(colors::MODE_AUTO).fg(Color::Black).bold()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Code/thinking blocks
    // ─────────────────────────────────────────────────────────────────────────

    /// Style for code block background
    pub fn code_block() -> Style {
        Style::new().bg(colors::CODE_BG)
    }

    /// Style for thinking block header
    pub fn thinking_header() -> Style {
        Style::new().fg(colors::TEXT_DIM).italic()
    }

    /// Style for thinking block content
    pub fn thinking_content() -> Style {
        Style::new().fg(colors::TEXT_DIM).dim()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Diff display
    // ─────────────────────────────────────────────────────────────────────────

    /// Style for diff deletions
    pub fn diff_delete() -> Style {
        Style::new().fg(colors::ERROR)
    }

    /// Style for diff insertions
    pub fn diff_insert() -> Style {
        Style::new().fg(colors::SUCCESS)
    }

    /// Style for diff context lines
    pub fn diff_context() -> Style {
        Style::new().fg(colors::TEXT_DIM)
    }

    /// Style for diff hunk headers
    pub fn diff_hunk_header() -> Style {
        Style::new().fg(colors::INFO)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Markdown rendering
    // ─────────────────────────────────────────────────────────────────────────

    pub fn inline_code() -> Style {
        Style::new().fg(colors::CODE_INLINE)
    }

    pub fn code_fallback() -> Style {
        Style::new().fg(colors::CODE_FALLBACK)
    }

    pub fn fence_marker() -> Style {
        Style::new().fg(colors::FENCE_MARKER)
    }

    pub fn blockquote_prefix() -> Style {
        Style::new().fg(colors::BLOCKQUOTE_PREFIX)
    }

    pub fn blockquote_text() -> Style {
        Style::new().fg(colors::BLOCKQUOTE_TEXT).italic()
    }

    pub fn link() -> Style {
        Style::new().fg(colors::LINK).underline()
    }

    pub fn heading_1() -> Style {
        Style::new().fg(colors::HEADING_1).bold()
    }

    pub fn heading_2() -> Style {
        Style::new().fg(colors::HEADING_2).bold()
    }

    pub fn heading_3() -> Style {
        Style::new().fg(colors::HEADING_3).bold()
    }

    pub fn bullet_prefix() -> Style {
        Style::new().fg(colors::BULLET_PREFIX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_constants_are_distinct() {
        // Ensure we don't accidentally use the same color for different purposes
        assert_ne!(colors::ERROR, colors::SUCCESS);
        assert_ne!(colors::ROLE_USER, colors::ROLE_ASSISTANT);
        assert_ne!(colors::INPUT_BG, colors::COMMAND_BG);
    }

    #[test]
    fn style_presets_build_correctly() {
        let user = styles::user_prompt();
        assert_eq!(user.fg, Some(colors::ROLE_USER));
        assert!(user.bold);

        let err = styles::error();
        assert_eq!(err.fg, Some(colors::ERROR));
        assert!(err.bold);
    }

    #[test]
    fn mode_styles_have_contrasting_fg() {
        // Mode badges should have black text for contrast
        assert_eq!(styles::mode_normal().fg, Some(Color::Black));
        assert_eq!(styles::mode_plan().fg, Some(Color::Black));
        assert_eq!(styles::mode_auto().fg, Some(Color::Black));
    }

    #[test]
    fn muted_and_dim_are_different() {
        let muted = styles::muted();
        let dim = styles::dim();

        // Both should have different characteristics
        assert!(!muted.dim);
        assert!(dim.dim);
    }
}
