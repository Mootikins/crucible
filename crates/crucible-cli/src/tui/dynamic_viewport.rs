//! Dynamic viewport sizing for inline terminals.
//!
//! This module provides `DynamicViewport`, a wrapper around ratatui's `Terminal`
//! that adds support for dynamically resizing the inline viewport height.
//!
//! This is a workaround until ratatui merges PR #1964 (set_viewport_height).
//! The implementation is based on that PR's logic.

use crossterm::{cursor, execute, terminal};
use ratatui::{backend::CrosstermBackend, Terminal, TerminalOptions, Viewport};
use std::io::{self, Stdout, Write};

/// A wrapper around `Terminal` that supports dynamic viewport height changes.
///
/// In inline mode, the viewport is a fixed-height region at the current cursor
/// position. This wrapper adds the ability to grow or shrink that region
/// dynamically, which is needed for features like:
/// - Expanding/collapsing todo lists
/// - Growing input areas for multiline input
/// - Modal dialogs of varying sizes
pub struct DynamicViewport {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Current viewport height
    viewport_height: u16,
    /// Y position of the viewport's top edge (in absolute terminal coordinates)
    viewport_y: u16,
    /// Whether we're in inline mode (vs fullscreen)
    inline_mode: bool,
}

impl DynamicViewport {
    /// Create a new dynamic viewport in inline mode.
    pub fn new_inline(height: u16) -> io::Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(height),
            },
        )?;

        // Get current cursor position as viewport top
        let (_, cursor_y) = cursor::position()?;

        Ok(Self {
            terminal,
            viewport_height: height,
            viewport_y: cursor_y,
            inline_mode: true,
        })
    }

    /// Create a new dynamic viewport in fullscreen mode.
    ///
    /// Note: `set_viewport_height` has no effect in fullscreen mode.
    pub fn new_fullscreen() -> io::Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;

        Ok(Self {
            terminal,
            viewport_height: 0,
            viewport_y: 0,
            inline_mode: false,
        })
    }

    /// Get a reference to the underlying terminal for drawing.
    pub fn terminal(&self) -> &Terminal<CrosstermBackend<Stdout>> {
        &self.terminal
    }

    /// Get a mutable reference to the underlying terminal for drawing.
    pub fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Get the current viewport height.
    pub fn viewport_height(&self) -> u16 {
        self.viewport_height
    }

    /// Set the viewport height, growing or shrinking as needed.
    ///
    /// When growing:
    /// - If there's room below, the viewport expands downward
    /// - If at the terminal bottom, content is scrolled up to make room
    ///
    /// When shrinking:
    /// - The viewport contracts, leaving empty space below
    ///
    /// This has no effect in fullscreen mode.
    pub fn set_viewport_height(&mut self, new_height: u16) -> io::Result<()> {
        if !self.inline_mode {
            return Ok(());
        }

        if self.viewport_height == new_height {
            return Ok(());
        }

        let old_height = self.viewport_height;
        self.viewport_height = new_height;

        // Clear the current viewport
        self.terminal.clear()?;

        if new_height > old_height {
            // Growing: check if we need to scroll to make room
            let (_, term_height) = terminal::size()?;
            let viewport_bottom = self.viewport_y + new_height;

            if viewport_bottom > term_height {
                // Need to scroll up to make room
                let overflow = viewport_bottom - term_height;
                self.scroll_up(overflow)?;
                self.viewport_y = self.viewport_y.saturating_sub(overflow);
            }
        }
        // Shrinking: no scroll needed, just render less

        // Recreate terminal with new viewport height
        // This is the "naive" approach - we destroy and recreate
        // because we can't directly modify Terminal's internal viewport
        self.recreate_terminal(new_height)?;

        Ok(())
    }

    /// Grow the viewport by the specified number of lines.
    pub fn grow(&mut self, lines: u16) -> io::Result<()> {
        self.set_viewport_height(self.viewport_height.saturating_add(lines))
    }

    /// Shrink the viewport by the specified number of lines.
    pub fn shrink(&mut self, lines: u16) -> io::Result<()> {
        self.set_viewport_height(self.viewport_height.saturating_sub(lines).max(1))
    }

    /// Scroll the terminal up by the specified number of lines.
    ///
    /// This pushes content into the scrollback buffer.
    fn scroll_up(&mut self, lines: u16) -> io::Result<()> {
        let mut stdout = io::stdout();

        // Save cursor position
        execute!(stdout, cursor::SavePosition)?;

        // Move to bottom of terminal and print newlines to scroll
        let (_, term_height) = terminal::size()?;
        execute!(stdout, cursor::MoveTo(0, term_height - 1))?;

        for _ in 0..lines {
            writeln!(stdout)?;
        }

        // Restore cursor position (adjusted for scroll)
        execute!(stdout, cursor::RestorePosition)?;

        stdout.flush()?;
        Ok(())
    }

    /// Recreate the terminal with a new viewport height.
    ///
    /// This is a workaround because ratatui doesn't expose a way to
    /// change the viewport height after creation.
    fn recreate_terminal(&mut self, new_height: u16) -> io::Result<()> {
        // We need to position the cursor correctly before creating the new terminal
        let mut stdout = io::stdout();
        execute!(stdout, cursor::MoveTo(0, self.viewport_y))?;

        // Create new backend and terminal
        let backend = CrosstermBackend::new(io::stdout());
        self.terminal = Terminal::with_options(
            backend,
            TerminalOptions {
                viewport: Viewport::Inline(new_height),
            },
        )?;

        Ok(())
    }

    /// Delegate to Terminal::draw
    pub fn draw<F>(&mut self, f: F) -> io::Result<ratatui::CompletedFrame<'_>>
    where
        F: FnOnce(&mut ratatui::Frame),
    {
        self.terminal.draw(f).map_err(|e| io::Error::other(e))
    }

    /// Delegate to Terminal::insert_before
    pub fn insert_before<F>(&mut self, height: u16, draw_fn: F) -> io::Result<()>
    where
        F: FnOnce(&mut ratatui::buffer::Buffer),
    {
        self.terminal
            .insert_before(height, draw_fn)
            .map_err(|e| io::Error::other(e))
    }

    /// Delegate to Terminal::clear
    pub fn clear(&mut self) -> io::Result<()> {
        self.terminal.clear().map_err(|e| io::Error::other(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a real terminal and can't run in CI
    // They're here for manual testing

    #[test]
    #[ignore = "requires real terminal"]
    fn test_viewport_grow() {
        let mut viewport = DynamicViewport::new_inline(5).unwrap();
        assert_eq!(viewport.viewport_height(), 5);

        viewport.grow(3).unwrap();
        assert_eq!(viewport.viewport_height(), 8);
    }

    #[test]
    #[ignore = "requires real terminal"]
    fn test_viewport_shrink() {
        let mut viewport = DynamicViewport::new_inline(10).unwrap();
        assert_eq!(viewport.viewport_height(), 10);

        viewport.shrink(3).unwrap();
        assert_eq!(viewport.viewport_height(), 7);
    }
}
