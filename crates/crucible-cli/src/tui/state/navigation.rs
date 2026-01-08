//! Navigation utilities for TUI state
//!
//! Provides word boundary detection and cursor movement helpers.

/// Find the byte position of the start of the previous word.
///
/// Skips trailing whitespace, then finds where the word begins.
/// Used for backward word navigation (Ctrl+Left or Alt+B).
pub fn find_word_start_backward(s: &str) -> usize {
    let mut chars = s.char_indices().rev().peekable();

    // Skip trailing whitespace
    while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
        chars.next();
    }

    // Skip word characters
    while chars.peek().is_some_and(|(_, c)| !c.is_whitespace()) {
        chars.next();
    }

    // Return position after the whitespace (start of the word we skipped)
    chars.next().map(|(i, c)| i + c.len_utf8()).unwrap_or(0)
}

/// Find the byte offset to the start of the next word.
///
/// Skips current word, then whitespace.
/// Used for forward word navigation (Ctrl+Right or Alt+F).
pub fn find_word_start_forward(s: &str) -> usize {
    let mut chars = s.char_indices().peekable();

    // Skip current word
    while chars.peek().is_some_and(|(_, c)| !c.is_whitespace()) {
        chars.next();
    }

    // Skip whitespace
    while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
        chars.next();
    }

    chars.peek().map(|(i, _)| *i).unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_word_start_backward_simple() {
        assert_eq!(find_word_start_backward("hello world"), 6);
        assert_eq!(find_word_start_backward("hello"), 0);
    }

    #[test]
    fn test_find_word_start_backward_with_trailing_space() {
        assert_eq!(find_word_start_backward("hello "), 0);
    }

    #[test]
    fn test_find_word_start_forward_simple() {
        assert_eq!(find_word_start_forward("hello world"), 6);
        assert_eq!(find_word_start_forward("hello"), 5);
    }

    #[test]
    fn test_find_word_start_forward_multiple_spaces() {
        assert_eq!(find_word_start_forward("hello  world"), 7); // After extra space
    }
}
