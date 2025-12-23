//! Scrollback-based TUI runner (ARCHIVED - for research reference)
//!
//! This module preserves the architectural approach of the original TUI implementation
//! that used terminal scrollback instead of alternate screen mode.
//!
//! ## Architecture Overview
//!
//! The scrollback approach differs from the ratatui alternate-screen approach:
//!
//! | Aspect | Scrollback (this) | Alternate Screen (RatatuiRunner) |
//! |--------|-------------------|----------------------------------|
//! | Screen mode | Normal terminal | Alternate screen buffer |
//! | History | Native scrollback | Internal buffer |
//! | Rendering | Bottom widget only | Full viewport control |
//! | Output | Print to stdout | Render to frame |
//!
//! ## Key Concepts
//!
//! 1. **Bottom Widget**: Only the input area, status line, and streaming indicator
//!    are rendered as a "widget" at the bottom of the terminal. Completed messages
//!    are printed directly to stdout and become part of terminal scrollback.
//!
//! 2. **Viewport Overflow**: When the viewport fills up, content is "spilled" to
//!    scrollback, becoming part of native terminal history.
//!
//! 3. **Markdown Rendering**: Assistant responses are rendered with markdown/syntax
//!    highlighting before being printed to scrollback.
//!
//! ## Original Structure
//!
//! ```text
//! TuiRunner
//! ├── state: TuiState           // Input buffer, mode, streaming state
//! ├── viewport: ViewportState   // Content tracking for overflow
//! ├── renderer: MarkdownRenderer // Syntax highlighting
//! ├── width/height: u16         // Terminal dimensions
//! └── popup_provider            // Command/agent completion
//! ```
//!
//! ## Event Loop Pattern
//!
//! ```text
//! loop {
//!     // 1. Poll terminal events (non-blocking, ~60fps)
//!     if event::poll(16ms)? {
//!         handle_key_event()?;
//!         handle_resize()?;
//!     }
//!
//!     // 2. Poll ring buffer for session events
//!     if let Some(content) = state.poll_events(&ring) {
//!         print_assistant_response(&content)?;  // To scrollback
//!     }
//!
//!     // 3. Render bottom widget only
//!     render_widget()?;
//! }
//! ```
//!
//! ## Terminal Setup
//!
//! Unlike RatatuiRunner, this approach:
//! - Uses `enable_raw_mode()` only (no alternate screen)
//! - Clears screen initially but stays in normal mode
//! - Preserves terminal scrollback history
//!
//! ## Pros and Cons
//!
//! **Pros:**
//! - Native scrollback (can scroll up with terminal)
//! - Simpler rendering (only bottom widget)
//! - Less memory (terminal manages history)
//!
//! **Cons:**
//! - No mouse scroll control
//! - Can't re-render history
//! - Widget position calculations are complex
//! - Harder to style conversation consistently
//!
//! ## Why We Switched to RatatuiRunner
//!
//! The alternate-screen approach (RatatuiRunner) provides:
//! - Full control over the viewport
//! - Consistent styling throughout conversation
//! - Mouse scroll support
//! - Easier widget composition with ratatui
//! - Better tool call visualization

#![allow(dead_code)] // This is reference code

use std::sync::Arc;

/// Skeleton of the original scrollback-based TUI runner.
///
/// See module documentation for architectural details.
pub struct ScrollbackRunner {
    /// TUI state (input buffer, mode, streaming)
    _state: (), // Was: TuiState
    /// Viewport for overflow management
    _viewport: (), // Was: ViewportState
    /// Markdown renderer
    _renderer: (), // Was: MarkdownRenderer
    /// Terminal dimensions
    _width: u16,
    _height: u16,
    /// Popup provider
    _popup_provider: Arc<()>,
}

impl ScrollbackRunner {
    /// Original terminal setup sequence:
    /// ```ignore
    /// enable_raw_mode()?;
    /// execute!(stdout,
    ///     terminal::Clear(terminal::ClearType::All),
    ///     cursor::MoveTo(0, 0),
    ///     cursor::Hide
    /// )?;
    /// ```
    pub fn setup_terminal() {
        // Reference only - see RatatuiRunner for active implementation
    }

    /// Original cleanup sequence:
    /// ```ignore
    /// disable_raw_mode()?;
    /// execute!(stdout,
    ///     cursor::MoveTo(0, height.saturating_sub(1)),
    ///     cursor::Show
    /// )?;
    /// writeln!(stdout)?;
    /// ```
    pub fn cleanup_terminal() {
        // Reference only - see RatatuiRunner for active implementation
    }

    /// Print user message to scrollback.
    ///
    /// Original approach:
    /// 1. Calculate widget position from bottom
    /// 2. Clear widget area
    /// 3. Insert line at widget top (scrolls content up)
    /// 4. Print formatted message
    /// 5. Track in viewport
    pub fn print_user_message(_message: &str) {
        // Reference only
    }

    /// Print assistant response with markdown rendering.
    ///
    /// Original approach:
    /// 1. Render markdown with syntax highlighting
    /// 2. Calculate lines needed
    /// 3. Insert lines at widget top
    /// 4. Print header + rendered content
    /// 5. Track in viewport
    pub fn print_assistant_response(_content: &str) {
        // Reference only
    }

    /// Render only the bottom widget area.
    ///
    /// Widget components (from bottom):
    /// - Status line (mode, error)
    /// - Input line with cursor
    /// - Streaming indicator (if active)
    /// - Optional popup overlay
    pub fn render_widget() {
        // Reference only
    }

    /// Handle viewport overflow on resize.
    ///
    /// When terminal shrinks, excess content is "spilled" to scrollback
    /// to maintain viewport invariants.
    pub fn handle_resize(_width: u16, _height: u16) {
        // Reference only
    }
}

#[cfg(test)]
mod tests {
    //! Tests preserved for reference - see runner.rs for active tests

    #[test]
    fn scrollback_runner_is_reference_only() {
        // This module exists for architectural documentation
        // Active implementation is in RatatuiRunner
    }
}
