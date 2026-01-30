//! Terminal and string width utilities
//!
//! Centralized functions for querying terminal dimensions and
//! calculating visible string width.

use crossterm::terminal;

/// Get current terminal width in columns.
///
/// Returns fallback (80) if terminal size unavailable.
pub fn terminal_width() -> usize {
    terminal::size().map(|(w, _)| w as usize).unwrap_or(80)
}

/// Get current terminal height in rows.
///
/// Returns fallback (24) if terminal size unavailable.
pub fn terminal_height() -> usize {
    terminal::size().map(|(_, h)| h as usize).unwrap_or(24)
}

/// Get terminal dimensions as (width, height).
///
/// Returns fallback (80, 24) if terminal size unavailable.
pub fn terminal_size() -> (usize, usize) {
    terminal::size()
        .map(|(w, h)| (w as usize, h as usize))
        .unwrap_or((80, 24))
}

/// Get current cursor position as (column, row).
///
/// Returns None if position unavailable (e.g., not a TTY, or
/// the terminal doesn't support cursor position reporting).
///
/// Note: This function may block briefly while waiting for
/// the terminal's response.
pub fn cursor_position() -> Option<(u16, u16)> {
    crossterm::cursor::position().ok()
}

// Re-export visible_width from ansi module for discoverability
pub use crucible_oil::ansi::visible_width;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_width_returns_positive() {
        let width = terminal_width();
        assert!(width > 0);
    }

    #[test]
    fn terminal_height_returns_positive() {
        let height = terminal_height();
        assert!(height > 0);
    }

    #[test]
    fn terminal_size_returns_positive() {
        let (w, h) = terminal_size();
        assert!(w > 0);
        assert!(h > 0);
    }

    #[test]
    fn visible_width_ascii() {
        assert_eq!(visible_width("hello"), 5);
    }

    #[test]
    fn visible_width_with_ansi() {
        assert_eq!(visible_width("\x1b[31mhello\x1b[0m"), 5);
    }

    #[test]
    fn visible_width_cjk() {
        // CJK characters are double-width
        assert_eq!(visible_width("你好"), 4);
    }
}
