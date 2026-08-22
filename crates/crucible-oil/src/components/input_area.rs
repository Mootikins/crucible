use crate::style::Color;

pub const INPUT_MAX_CONTENT_LINES: usize = 3;

/// Trait for styling the input area based on mode or context
pub trait InputStyle {
    /// Background color for this input style
    fn bg_color(&self) -> Color;

    /// Prompt text to display (e.g., " > ", " : ", " ! ")
    fn prompt(&self) -> &'static str;

    /// Get the display content (with mode prefix stripped if applicable)
    fn display_content<'a>(&self, content: &'a str) -> &'a str {
        content
    }

    /// Get the display cursor position (adjusted for mode prefix if applicable)
    fn display_cursor(&self, cursor: usize) -> usize {
        cursor
    }
}
