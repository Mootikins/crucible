//! Paste handling for TUI input
//!
//! Manages multi-line paste detection, paste indicators, and pending paste operations.

use once_cell::sync::Lazy;
use regex::Regex;

/// Multi-line pasted content
#[derive(Debug, Clone)]
pub enum PastedContent {
    /// Multi-line text paste
    Text {
        /// The full pasted content
        content: String,
        /// Number of lines
        line_count: usize,
        /// Number of characters
        char_count: usize,
    },
    // Future: Image { data: Vec<u8>, mime: String }
}

impl PastedContent {
    /// Create a new text paste from a string
    pub fn text(content: String) -> Self {
        use crate::tui::scroll_utils::LineCount;
        let line_count = LineCount::count(&content);
        let char_count = content.chars().count();
        Self::Text {
            content,
            line_count,
            char_count,
        }
    }

    /// Get the content as a string
    pub fn content(&self) -> &str {
        match self {
            Self::Text { content, .. } => content,
        }
    }

    /// Format a summary of this paste for display
    pub fn summary(&self) -> String {
        match self {
            Self::Text {
                line_count,
                char_count,
                ..
            } => format!("[{} lines, {} chars]", line_count, char_count),
        }
    }
}

/// Regex to match paste indicator patterns like `[2 lines, 45 chars]`
static PASTE_INDICATOR_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\[\d+ lines?, \d+ chars?\]").expect("paste indicator regex should compile")
});

/// Manages pending paste operations in the input buffer
pub struct PasteHandler {
    /// Pending paste content
    pub pending_pastes: Vec<PastedContent>,
}

impl Default for PasteHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PasteHandler {
    pub fn new() -> Self {
        Self {
            pending_pastes: Vec::new(),
        }
    }

    /// Get a formatted summary of all pending pastes
    pub fn summary(&self) -> Option<String> {
        if self.pending_pastes.is_empty() {
            return None;
        }

        let total_lines: usize = self
            .pending_pastes
            .iter()
            .map(|p| match p {
                PastedContent::Text { line_count, .. } => *line_count,
            })
            .sum();

        let total_chars: usize = self
            .pending_pastes
            .iter()
            .map(|p| match p {
                PastedContent::Text { char_count, .. } => *char_count,
            })
            .sum();

        if self.pending_pastes.len() == 1 {
            Some(self.pending_pastes[0].summary())
        } else {
            Some(format!(
                "[{} pastes: {} lines, {} chars]",
                self.pending_pastes.len(),
                total_lines,
                total_chars
            ))
        }
    }

    /// Clear all pending pastes
    pub fn clear(&mut self) -> bool {
        let was_empty = self.pending_pastes.is_empty();
        self.pending_pastes.clear();
        !was_empty
    }

    /// Add a new paste
    pub fn push(&mut self, paste: PastedContent) {
        self.pending_pastes.push(paste);
    }

    /// Find paste indicator containing or immediately after the given byte position
    ///
    /// Returns `Some((start_byte, end_byte, index))` if the position is at the end of
    /// an indicator (would delete into it) or inside one. The index corresponds to
    /// the Nth indicator in the input (0-indexed), which maps to `pending_pastes[index]`.
    pub fn find_indicator_at(&self, input: &str, pos: usize) -> Option<(usize, usize, usize)> {
        for (idx, mat) in PASTE_INDICATOR_RE.find_iter(input).enumerate() {
            // Check if cursor is inside the indicator or just after it (about to delete into it)
            if pos > mat.start() && pos <= mat.end() {
                return Some((mat.start(), mat.end(), idx));
            }
        }
        None
    }

    /// Remove a paste at the given index
    pub fn remove(&mut self, index: usize) -> Option<PastedContent> {
        if index < self.pending_pastes.len() {
            Some(self.pending_pastes.remove(index))
        } else {
            None
        }
    }

    /// Drain all pastes and return them
    pub fn drain(&mut self) -> std::vec::Drain<'_, PastedContent> {
        self.pending_pastes.drain(..)
    }

    /// Check if there are any pending pastes
    pub fn is_empty(&self) -> bool {
        self.pending_pastes.is_empty()
    }

    /// Get the number of pending pastes
    pub fn len(&self) -> usize {
        self.pending_pastes.len()
    }

    /// Get reference to pending pastes
    pub fn pastes(&self) -> &[PastedContent] {
        &self.pending_pastes
    }

    /// Get mutable reference to pending pastes
    pub fn pastes_mut(&mut self) -> &mut [PastedContent] {
        &mut self.pending_pastes
    }
}

/// Helper to build a message with pastes appended to typed input
pub fn build_message_with_pastes(
    pending_pastes: &mut Vec<PastedContent>,
    typed_input: String,
) -> String {
    let mut message = String::new();

    // Add all pending pastes first
    for paste in pending_pastes.drain(..) {
        message.push_str(paste.content());
        if !paste.content().ends_with('\n') {
            message.push('\n');
        }
    }

    // Add the typed content
    message.push_str(&typed_input);

    message
}

/// Helper to build a delete operation for a paste indicator
pub struct PasteIndicatorDelete {
    /// The new input string after deletion
    pub new_input: String,
    /// The new cursor position after deletion
    pub new_cursor: usize,
}

/// Build the result of deleting a paste indicator from input
///
/// # Arguments
/// * `input` - The current input string
/// * `indicator_start` - Start byte position of the indicator
/// * `indicator_end` - End byte position of the indicator
///
/// # Returns
/// A `PasteIndicatorDelete` struct with the new input and cursor position
pub fn build_indicator_delete(
    input: &str,
    indicator_start: usize,
    indicator_end: usize,
) -> PasteIndicatorDelete {
    let mut new_input = String::with_capacity(input.len() - (indicator_end - indicator_start));
    new_input.push_str(&input[..indicator_start]);

    // Handle space before indicator (if present)
    let trim_space_before = indicator_start > 0
        && input.as_bytes().get(indicator_start.saturating_sub(1)) == Some(&b' ');
    let new_cursor = if trim_space_before {
        new_input.pop(); // Remove trailing space before indicator
        indicator_start - 1
    } else {
        indicator_start
    };

    new_input.push_str(&input[indicator_end..]);

    PasteIndicatorDelete {
        new_input,
        new_cursor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paste_summary_single() {
        let paste = PastedContent::text("hello\nworld".to_string());
        assert_eq!(paste.summary(), "[2 lines, 11 chars]");
    }

    #[test]
    fn test_paste_content() {
        let paste = PastedContent::text("hello\nworld".to_string());
        assert_eq!(paste.content(), "hello\nworld");
    }

    #[test]
    fn test_handler_summary_empty() {
        let handler = PasteHandler::new();
        assert!(handler.summary().is_none());
    }

    #[test]
    fn test_handler_summary_single() {
        let mut handler = PasteHandler::new();
        handler.push(PastedContent::text("test".to_string()));
        let summary = handler.summary();
        assert!(summary.is_some());
        assert!(summary.unwrap().contains("1 lines"));
    }

    #[test]
    fn test_handler_clear() {
        let mut handler = PasteHandler::new();
        handler.push(PastedContent::text("test".to_string()));
        assert!(!handler.is_empty());
        assert!(handler.clear());
        assert!(handler.is_empty());
        assert!(!handler.clear()); // Already empty
    }

    #[test]
    fn test_find_indicator_at() {
        let handler = PasteHandler::new();
        let input = "some text [2 lines, 10 chars] more";
        let result = handler.find_indicator_at(input, 15);
        assert!(result.is_some());
    }

    #[test]
    fn test_build_message_with_pastes() {
        let mut pastes = vec![
            PastedContent::text("paste1\n".to_string()),
            PastedContent::text("paste2\n".to_string()),
        ];
        let typed = "typed input".to_string();
        let result = build_message_with_pastes(&mut pastes, typed);
        assert_eq!(result, "paste1\npaste2\ntyped input");
        assert!(pastes.is_empty());
    }
}
