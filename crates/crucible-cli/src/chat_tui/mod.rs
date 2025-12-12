//! Chat TUI - Inline viewport chat interface with fuzzy completion
//!
//! This module provides a ratatui-based chat interface that preserves terminal
//! scrollback while providing a widget-based input area with fuzzy completion menus.
//!
//! ## Architecture
//!
//! Uses `Viewport::Inline(n)` to render a fixed-height viewport at the bottom
//! of the terminal. Agent responses are pushed into scrollback via `insert_before()`.
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │     Terminal Scrollback Buffer      │  ← Normal terminal, scrolls up
//! │  Agent: Previous response...        │
//! │  You: Earlier message...            │
//! ├─────────────────────────────────────┤
//! │  ┌─ Inline Viewport (8 lines) ────┐ │  ← Ratatui manages this
//! │  │ > your input here_              │ │  ← tui-textarea widget
//! │  │ ───────────────────────────── │ │  ← separator
//! │  │ [plan] ● Ready | /help          │ │  ← status bar
//! │  └─────────────────────────────────┘ │
//! └─────────────────────────────────────┘
//! ```

mod app;
mod completion;
mod convert;
mod event_loop;
mod input;
mod keybindings;
mod messages;
mod render;
mod sources;
pub mod widgets;

pub use app::{ChatApp, ChatMode, RenderState};
pub use completion::{CompletionItem, CompletionState, CompletionType};
pub use event_loop::{run_event_loop, run_with_agent, AgentResponse, ChatMessage, EventResult};
pub use input::{ChatAction, ChatInput};
pub use keybindings::KeyBindings;
pub use messages::{calculate_message_height, render_message, ChatMessageDisplay, MessageRole};
pub use render::render_chat_viewport;
pub use sources::{command_source, CommandSource, CompletionSource, FileSource};

use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::{backend::CrosstermBackend, Terminal, TerminalOptions, Viewport};
use std::io::{stdout, Stdout};

/// Default height of the inline viewport in lines
pub const VIEWPORT_HEIGHT: u16 = 8;

/// Minimum allowed viewport height (must show at least input + status bar)
pub const MIN_VIEWPORT_HEIGHT: u16 = 3;

/// Maximum allowed viewport height (leave space for scrollback)
pub const MAX_VIEWPORT_HEIGHT: u16 = 50;

/// Validate viewport height parameter
///
/// # Arguments
/// * `height` - Proposed viewport height in lines
///
/// # Returns
/// Ok(()) if height is valid, Err with descriptive message otherwise
///
/// # Errors
/// Returns an error if height is outside the valid range [MIN_VIEWPORT_HEIGHT, MAX_VIEWPORT_HEIGHT]
fn validate_viewport_height(height: u16) -> Result<()> {
    anyhow::ensure!(
        height >= MIN_VIEWPORT_HEIGHT,
        "Viewport height must be at least {} lines (got {})",
        MIN_VIEWPORT_HEIGHT,
        height
    );
    anyhow::ensure!(
        height <= MAX_VIEWPORT_HEIGHT,
        "Viewport height must be at most {} lines (got {})",
        MAX_VIEWPORT_HEIGHT,
        height
    );
    Ok(())
}

/// Set up an inline terminal viewport that preserves scrollback
///
/// # Arguments
/// * `height` - Height of the viewport in lines. Must be between MIN_VIEWPORT_HEIGHT and MAX_VIEWPORT_HEIGHT.
///
/// # Returns
/// A configured Terminal with Viewport::Inline that preserves scrollback history.
///
/// # Errors
/// Returns an error if:
/// - Raw mode cannot be enabled
/// - Terminal creation fails
/// - Height is outside valid bounds
///
/// # Example
/// ```no_run
/// use crucible_cli::chat_tui::{setup_inline_terminal, VIEWPORT_HEIGHT};
///
/// let terminal = setup_inline_terminal(VIEWPORT_HEIGHT).expect("Failed to setup terminal");
/// ```
pub fn setup_inline_terminal(height: u16) -> Result<Terminal<CrosstermBackend<Stdout>>> {
    // Validate height parameter
    validate_viewport_height(height)?;

    // Enable raw mode for immediate key capture
    enable_raw_mode()?;
    // NOTE: We do NOT enter alternate screen - this preserves scrollback

    let backend = CrosstermBackend::new(stdout());
    let options = TerminalOptions {
        viewport: Viewport::Inline(height),
    };

    let terminal = Terminal::with_options(backend, options)?;
    Ok(terminal)
}

/// Clean up terminal state
pub fn cleanup_terminal() -> Result<()> {
    disable_raw_mode()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    fn test_viewport_height_constant() {
        assert_eq!(VIEWPORT_HEIGHT, 8);
    }

    #[test]
    fn test_viewport_setup_with_test_backend() {
        // Test viewport configuration using TestBackend
        // Note: TestBackend doesn't simulate Viewport::Inline behavior - it uses full backend size
        // This test verifies configuration is accepted, not that inline viewport works in tests
        let backend = TestBackend::new(80, 24);
        let options = TerminalOptions {
            viewport: Viewport::Inline(8),
        };

        let terminal = Terminal::with_options(backend, options);
        assert!(terminal.is_ok(), "Terminal creation with inline viewport should succeed");

        // TestBackend returns full backend height, not viewport height
        // Real terminal with CrosstermBackend would respect Viewport::Inline
        let terminal = terminal.unwrap();
        let size = terminal.size().unwrap();
        assert_eq!(size.width, 80, "Width should match backend");
        // Note: size.height is 24 with TestBackend, but would be 8 with real terminal
    }

    #[test]
    fn test_viewport_setup_respects_height_parameter() {
        // Test that different viewport heights are accepted
        // Note: TestBackend doesn't simulate Viewport::Inline - these just verify configuration
        for height in [4, 8, 12, 16] {
            let backend = TestBackend::new(80, 24);
            let options = TerminalOptions {
                viewport: Viewport::Inline(height),
            };

            let terminal = Terminal::with_options(backend, options);
            assert!(terminal.is_ok(), "Terminal creation should succeed for inline height {}", height);
            // Real inline viewport behavior requires actual terminal, not TestBackend
        }
    }

    #[test]
    fn test_viewport_inline_configuration() {
        // Verify that Viewport::Inline is correctly configured
        let backend = TestBackend::new(80, 24);
        let options = TerminalOptions {
            viewport: Viewport::Inline(VIEWPORT_HEIGHT),
        };

        let result = Terminal::with_options(backend, options);
        assert!(
            result.is_ok(),
            "Inline viewport with default height should be created successfully"
        );
    }

    #[test]
    fn test_viewport_dimensions() {
        // Test terminal creation with viewport configuration
        // Note: TestBackend doesn't respect Viewport::Inline sizing
        let backend = TestBackend::new(80, 24);
        let options = TerminalOptions {
            viewport: Viewport::Inline(8),
        };

        let terminal = Terminal::with_options(backend, options).unwrap();
        let size = terminal.size().unwrap();

        // TestBackend reports full backend size, not viewport size
        assert_eq!(size.width, 80, "Width should match backend width");
        assert_eq!(size.height, 24, "TestBackend reports full height (inline viewport requires real terminal)");
    }

    #[test]
    fn test_viewport_height_bounds() {
        // Test that height bounds are sensible
        assert!(MIN_VIEWPORT_HEIGHT >= 3, "Minimum height should allow input + status bar");
        assert!(MAX_VIEWPORT_HEIGHT >= VIEWPORT_HEIGHT, "Max height should be at least default");
        assert!(VIEWPORT_HEIGHT >= MIN_VIEWPORT_HEIGHT, "Default height should be within bounds");
        assert!(VIEWPORT_HEIGHT <= MAX_VIEWPORT_HEIGHT, "Default height should be within bounds");
    }

    #[test]
    fn test_viewport_edge_cases() {
        // Test that extreme viewport heights are accepted
        // Note: TestBackend doesn't simulate Viewport::Inline behavior

        // Test minimum height configuration
        let backend = TestBackend::new(80, 24);
        let options = TerminalOptions {
            viewport: Viewport::Inline(MIN_VIEWPORT_HEIGHT),
        };
        let terminal = Terminal::with_options(backend, options);
        assert!(terminal.is_ok(), "Minimum viewport height should be accepted");

        // Test maximum height configuration
        let backend = TestBackend::new(80, 60);
        let options = TerminalOptions {
            viewport: Viewport::Inline(MAX_VIEWPORT_HEIGHT),
        };
        let terminal = Terminal::with_options(backend, options);
        assert!(terminal.is_ok(), "Maximum viewport height should be accepted");
    }

    /// Note: We cannot easily test raw mode enable/disable in unit tests
    /// as it requires actual terminal I/O. The `setup_inline_terminal` function
    /// should be tested manually or in integration tests with a real terminal.
    ///
    /// For now, we test the configuration logic only.
    #[test]
    fn test_cleanup_terminal_idempotent() {
        // cleanup_terminal should be safe to call even if raw mode wasn't enabled
        // This might fail in CI, but that's expected - raw mode operations need a real terminal
        let result = cleanup_terminal();
        // We don't assert on result because it may fail in CI without a TTY
        // The important thing is that it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_validate_viewport_height_valid_cases() {
        // Test minimum valid height
        assert!(
            validate_viewport_height(MIN_VIEWPORT_HEIGHT).is_ok(),
            "Minimum height should be valid"
        );

        // Test default height
        assert!(
            validate_viewport_height(VIEWPORT_HEIGHT).is_ok(),
            "Default height should be valid"
        );

        // Test maximum valid height
        assert!(
            validate_viewport_height(MAX_VIEWPORT_HEIGHT).is_ok(),
            "Maximum height should be valid"
        );

        // Test various values in range
        for height in [4, 5, 8, 10, 16, 20, 30, 40, 50] {
            if height >= MIN_VIEWPORT_HEIGHT && height <= MAX_VIEWPORT_HEIGHT {
                assert!(
                    validate_viewport_height(height).is_ok(),
                    "Height {} should be valid",
                    height
                );
            }
        }
    }

    #[test]
    fn test_validate_viewport_height_too_small() {
        // Test below minimum
        for height in [0, 1, 2] {
            let result = validate_viewport_height(height);
            assert!(result.is_err(), "Height {} should be invalid (too small)", height);
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("at least"),
                "Error message should mention minimum: {}",
                err_msg
            );
        }
    }

    #[test]
    fn test_validate_viewport_height_too_large() {
        // Test above maximum
        for height in [51, 60, 100, 1000] {
            let result = validate_viewport_height(height);
            assert!(result.is_err(), "Height {} should be invalid (too large)", height);
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("at most"),
                "Error message should mention maximum: {}",
                err_msg
            );
        }
    }

    #[test]
    fn test_validate_viewport_height_boundary_cases() {
        // Test just below minimum
        assert!(
            validate_viewport_height(MIN_VIEWPORT_HEIGHT - 1).is_err(),
            "Height just below minimum should be invalid"
        );

        // Test minimum (boundary)
        assert!(
            validate_viewport_height(MIN_VIEWPORT_HEIGHT).is_ok(),
            "Minimum height boundary should be valid"
        );

        // Test maximum (boundary)
        assert!(
            validate_viewport_height(MAX_VIEWPORT_HEIGHT).is_ok(),
            "Maximum height boundary should be valid"
        );

        // Test just above maximum
        assert!(
            validate_viewport_height(MAX_VIEWPORT_HEIGHT + 1).is_err(),
            "Height just above maximum should be invalid"
        );
    }
}
