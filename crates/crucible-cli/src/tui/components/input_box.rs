//! Input box widget for text entry
//!
//! This widget provides a multiline text input field with cursor support,
//! suitable for command-line style input at the bottom of the TUI.
//!
//! ## Multiline Support
//!
//! - Ctrl+J inserts newlines into the buffer
//! - Input box grows based on line count up to `max_height`
//! - When content exceeds `max_height`, scrolling is enabled
//! - Cursor navigates across lines properly
//! - Long lines are visually wrapped (word-aware wrapping)

use crate::tui::{components::InteractiveWidget, event_result::EventResult, styles::presets};
use crossterm::event::Event;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Default maximum visible lines for multiline input
pub const DEFAULT_MAX_INPUT_LINES: u16 = 10;

/// Widget that renders a multiline input box with cursor support
///
/// # State
///
/// - `buffer`: The current input text (may contain newlines)
/// - `cursor_position`: Byte offset of the cursor (0 = before first char)
/// - `prompt`: Text to display before the first line (e.g., " > ")
/// - `focused`: Whether the widget has focus (affects styling)
/// - `scroll_offset`: First visible line when content exceeds max_height
/// - `max_height`: Maximum visible lines (default 10)
/// - `wrap_width`: Width for text wrapping (default 80)
///
/// # Rendering
///
/// The input is displayed with the prompt on the first line and content below.
/// When the cursor is at the end, a space is added to show the cursor position.
/// Long lines are visually wrapped at word boundaries.
/// Content scrolls within the input area when it exceeds max_height lines.
pub struct InputBoxWidget<'a> {
    buffer: &'a str,
    cursor_position: usize,
    prompt: &'a str,
    focused: bool,
    scroll_offset: usize,
    max_height: u16,
    /// Width for calculating wrapped lines (content area minus prompt)
    wrap_width: u16,
}

impl<'a> InputBoxWidget<'a> {
    /// Create a new input box widget
    ///
    /// # Arguments
    ///
    /// * `buffer` - The current input text
    /// * `cursor_position` - Byte offset of cursor within buffer
    pub fn new(buffer: &'a str, cursor_position: usize) -> Self {
        Self {
            buffer,
            cursor_position,
            prompt: " > ",
            focused: true,
            scroll_offset: 0,
            max_height: DEFAULT_MAX_INPUT_LINES,
            wrap_width: 80, // Default, will be updated at render time
        }
    }

    /// Set the wrap width for text wrapping calculations
    pub fn wrap_width(mut self, width: u16) -> Self {
        self.wrap_width = width;
        self
    }

    /// Set the prompt text (default is " > ")
    pub fn prompt(mut self, prompt: &'a str) -> Self {
        self.prompt = prompt;
        self
    }

    /// Set whether the widget is focused
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Set scroll offset for multiline content
    pub fn scroll_offset(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    /// Set maximum visible height in lines
    pub fn max_height(mut self, height: u16) -> Self {
        self.max_height = height;
        self
    }

    /// Count the number of logical lines in the buffer (separated by \n)
    pub fn logical_line_count(&self) -> usize {
        if self.buffer.is_empty() {
            1
        } else {
            self.buffer.lines().count().max(1) + if self.buffer.ends_with('\n') { 1 } else { 0 }
        }
    }

    /// Count visual lines after wrapping (for dynamic height calculation)
    ///
    /// Each logical line may wrap to multiple visual lines based on wrap_width.
    /// The prompt takes 3 characters, so content width is wrap_width - 3.
    pub fn line_count(&self) -> usize {
        if self.buffer.is_empty() {
            return 1;
        }

        let content_width = (self.wrap_width as usize).saturating_sub(3).max(10);
        let mut visual_lines = 0;

        // Use .lines() which handles trailing newlines correctly
        // Then add 1 if there's a trailing newline (for the empty line after it)
        for line in self.buffer.lines() {
            if line.is_empty() {
                visual_lines += 1;
            } else {
                // Calculate how many visual lines this logical line needs
                let char_count = line.chars().count();
                visual_lines += char_count.div_ceil(content_width);
            }
        }

        // If buffer has content but no newlines, we need at least 1 line
        // If buffer ends with newline, add 1 for the empty line after it
        if visual_lines == 0 {
            visual_lines = 1;
        } else if self.buffer.ends_with('\n') {
            visual_lines += 1;
        }

        visual_lines
    }

    /// Calculate the required height for displaying this input
    ///
    /// Returns the minimum of line_count and max_height, plus padding for borders.
    pub fn required_height(&self) -> u16 {
        let lines = self.line_count() as u16;
        lines.min(self.max_height) + 2 // +2 for top/bottom padding
    }

    /// Convert byte offset to logical (line, column) position (not wrapped)
    ///
    /// Returns (line_index, column_index) where both are 0-based.
    /// This does NOT account for visual wrapping.
    pub fn cursor_to_logical_line_col(&self) -> (usize, usize) {
        let before_cursor = &self.buffer[..self.cursor_position.min(self.buffer.len())];
        let line = before_cursor.matches('\n').count();
        let last_newline = before_cursor.rfind('\n');
        let col = match last_newline {
            Some(pos) => before_cursor.len() - pos - 1,
            None => before_cursor.len(),
        };
        (line, col)
    }

    /// Convert byte offset to visual (line, column) position with wrapping
    ///
    /// Returns (visual_line_index, visual_column) where both are 0-based.
    /// This accounts for visual wrapping based on wrap_width.
    pub fn cursor_to_line_col(&self) -> (usize, usize) {
        let content_width = (self.wrap_width as usize).saturating_sub(3).max(10);
        let before_cursor = &self.buffer[..self.cursor_position.min(self.buffer.len())];

        let mut visual_line = 0;

        // Split by newlines and calculate visual lines
        let lines: Vec<&str> = self.buffer.split('\n').collect();
        let mut bytes_processed = 0;

        for (logical_line_idx, line) in lines.iter().enumerate() {
            let line_start = bytes_processed;
            let line_end = line_start + line.len();

            // Check if cursor is on this logical line
            if self.cursor_position <= line_end || logical_line_idx == lines.len() - 1 {
                // Cursor is on this logical line
                let cursor_offset_in_line = self.cursor_position.saturating_sub(line_start);
                let chars_before_cursor: usize = line
                    .chars()
                    .take(cursor_offset_in_line)
                    .count()
                    .min(line.chars().count());

                // Calculate which visual line within this logical line
                let visual_line_in_block = chars_before_cursor / content_width;
                let visual_col = chars_before_cursor % content_width;

                return (visual_line + visual_line_in_block, visual_col);
            }

            // Calculate visual lines for this logical line
            let char_count = line.chars().count();
            if char_count == 0 {
                visual_line += 1;
            } else {
                visual_line += char_count.div_ceil(content_width);
            }

            // +1 for the newline character
            bytes_processed = line_end + 1;
        }

        // Fallback (should not reach here)
        let col = before_cursor.chars().count() % content_width;
        (visual_line, col)
    }

    /// Calculate required scroll offset to keep cursor visible
    pub fn scroll_to_cursor(&self) -> usize {
        let (cursor_line, _) = self.cursor_to_line_col();
        let visible_lines = self.max_height as usize;

        if cursor_line < self.scroll_offset {
            // Cursor above visible area
            cursor_line
        } else if cursor_line >= self.scroll_offset + visible_lines {
            // Cursor below visible area
            cursor_line.saturating_sub(visible_lines - 1)
        } else {
            // Cursor is visible
            self.scroll_offset
        }
    }
}

impl Widget for InputBoxWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Determine style and prompt based on prefix
        let trimmed = self.buffer.trim_start();
        let (style, display_prompt, content) = if !self.focused {
            (presets::dim(), self.prompt, self.buffer)
        } else if let Some(rest) = trimmed.strip_prefix('!') {
            // Shell passthrough: red tint, show "!" as prompt
            (presets::input_shell(), " ! ", rest)
        } else if let Some(rest) = trimmed.strip_prefix(':') {
            // REPL command: yellow tint, show ":" as prompt
            (presets::input_repl(), " : ", rest)
        } else {
            // Default style and prompt
            (presets::input_box(), self.prompt, self.buffer)
        };

        // Fill entire area with input box background
        buf.set_style(area, style);

        // Content width for wrapping (area width minus prompt)
        let prompt_len = display_prompt.chars().count();
        let content_width = (area.width as usize).saturating_sub(prompt_len).max(10);

        // Split content by logical lines and wrap each
        let mut render_lines: Vec<Line> = Vec::new();
        let logical_lines: Vec<&str> = if content.is_empty() {
            vec![""]
        } else {
            content.split('\n').collect()
        };

        for (logical_idx, logical_line) in logical_lines.iter().enumerate() {
            let is_first_logical = logical_idx == 0;
            let is_last_logical = logical_idx == logical_lines.len() - 1;

            // Wrap this logical line into visual lines
            let visual_lines = wrap_line(logical_line, content_width);

            for (visual_idx, visual_line) in visual_lines.iter().enumerate() {
                let prefix = if is_first_logical && visual_idx == 0 {
                    // First line of first logical line gets the prompt
                    Span::raw(display_prompt.to_string())
                } else {
                    // All other lines get padding to align with prompt
                    Span::raw(" ".repeat(prompt_len))
                };

                // Add space at end of last visual line of last logical line if cursor at end
                let line_text = if is_last_logical
                    && visual_idx == visual_lines.len() - 1
                    && self.cursor_position >= self.buffer.len()
                {
                    format!("{} ", visual_line)
                } else {
                    visual_line.to_string()
                };

                render_lines.push(Line::from(vec![prefix, Span::raw(line_text)]));
            }
        }

        let total_visual_lines = render_lines.len();
        let visible_lines = (self.max_height as usize).min(total_visual_lines);

        // Calculate scroll offset to keep cursor visible
        let effective_scroll = self.scroll_to_cursor();

        // Calculate vertical centering if we have fewer lines than area height
        let content_height = visible_lines as u16;
        use crate::tui::geometry::PopupGeometry;
        let start_y = PopupGeometry::center_vertically_if_fits(area, content_height);

        // Render visible lines with scroll offset
        for (i, line) in render_lines
            .into_iter()
            .skip(effective_scroll)
            .take(visible_lines)
            .enumerate()
        {
            let line_area = Rect {
                x: area.x,
                y: start_y + i as u16,
                width: area.width,
                height: 1,
            };
            let paragraph = Paragraph::new(line).style(style);
            paragraph.render(line_area, buf);
        }
    }
}

/// Wrap a single line of text to the given width
///
/// Returns a vector of visual lines. Each visual line fits within width chars.
/// Does simple character-based wrapping (not word-aware for simplicity).
fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }

    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= width {
        return vec![line.to_string()];
    }

    let mut result = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + width).min(chars.len());
        let visual_line: String = chars[start..end].iter().collect();
        result.push(visual_line);
        start = end;
    }

    result
}

impl InteractiveWidget for InputBoxWidget<'_> {
    fn handle_event(&mut self, _event: &Event) -> EventResult {
        // Input box is managed by the runner/view layer
        // This widget is display-only - actual editing happens at a higher level
        // The runner maintains the buffer and cursor state and passes them here
        EventResult::Ignored
    }

    fn focusable(&self) -> bool {
        // Input box can receive focus
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_widget_creation() {
        let widget = InputBoxWidget::new("hello", 5);
        assert_eq!(widget.buffer, "hello");
        assert_eq!(widget.cursor_position, 5);
        assert_eq!(widget.prompt, " > ");
        assert!(widget.focused);
    }

    #[test]
    fn test_prompt_builder() {
        let widget = InputBoxWidget::new("test", 0).prompt("$ ");
        assert_eq!(widget.prompt, "$ ");
    }

    #[test]
    fn test_focused_builder() {
        let widget = InputBoxWidget::new("test", 0).focused(false);
        assert!(!widget.focused);
    }

    #[test]
    fn test_focusable() {
        let widget = InputBoxWidget::new("", 0);
        assert!(widget.focusable());
    }

    #[test]
    fn test_empty_input_renders() {
        let widget = InputBoxWidget::new("", 0);

        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                f.render_widget(widget, area);
            })
            .unwrap();

        // Should render without panicking
    }

    #[test]
    fn test_text_input_renders() {
        let widget = InputBoxWidget::new("hello world", 5);

        let backend = TestBackend::new(80, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                f.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // Should contain the input text and prompt
        assert!(content.contains("hello world"));
        assert!(content.contains(">"));
    }

    // =============================================================================
    // Snapshot Tests
    // =============================================================================

    mod snapshot_tests {
        use super::*;
        use insta::assert_snapshot;

        const TEST_WIDTH: u16 = 80;
        const TEST_HEIGHT: u16 = 3;

        fn test_terminal() -> Terminal<TestBackend> {
            Terminal::new(TestBackend::new(TEST_WIDTH, TEST_HEIGHT)).unwrap()
        }

        fn render_widget(buffer: &str, cursor_pos: usize, focused: bool) -> Terminal<TestBackend> {
            let mut terminal = test_terminal();
            terminal
                .draw(|f| {
                    let widget = InputBoxWidget::new(buffer, cursor_pos).focused(focused);
                    f.render_widget(widget, f.area());
                })
                .unwrap();
            terminal
        }

        #[test]
        fn empty_input() {
            let terminal = render_widget("", 0, true);
            assert_snapshot!("input_box_empty", terminal.backend());
        }

        #[test]
        fn empty_input_unfocused() {
            let terminal = render_widget("", 0, false);
            assert_snapshot!("input_box_empty_unfocused", terminal.backend());
        }

        #[test]
        fn text_cursor_at_start() {
            let terminal = render_widget("hello world", 0, true);
            assert_snapshot!("input_box_cursor_start", terminal.backend());
        }

        #[test]
        fn text_cursor_at_middle() {
            let terminal = render_widget("hello world", 5, true);
            assert_snapshot!("input_box_cursor_middle", terminal.backend());
        }

        #[test]
        fn text_cursor_at_end() {
            let terminal = render_widget("hello world", 11, true);
            assert_snapshot!("input_box_cursor_end", terminal.backend());
        }

        #[test]
        fn long_text_wrapping() {
            let long_text = "This is a very long input that exceeds the typical width and should handle wrapping gracefully without breaking the layout";
            let terminal = render_widget(long_text, 50, true);
            assert_snapshot!("input_box_long_wrap", terminal.backend());
        }

        #[test]
        fn custom_prompt() {
            let mut terminal = test_terminal();
            terminal
                .draw(|f| {
                    let widget = InputBoxWidget::new("test", 4).prompt("$ ");
                    f.render_widget(widget, f.area());
                })
                .unwrap();
            assert_snapshot!("input_box_custom_prompt", terminal.backend());
        }

        #[test]
        fn shell_passthrough_prefix() {
            // ! prefix should use red-tinted style
            let terminal = render_widget("!ls -la", 7, true);
            assert_snapshot!("input_box_shell_prefix", terminal.backend());
        }

        #[test]
        fn repl_command_prefix() {
            // : prefix should use yellow-tinted style
            let terminal = render_widget(":quit", 5, true);
            assert_snapshot!("input_box_repl_prefix", terminal.backend());
        }

        #[test]
        fn slash_command_no_special_style() {
            // / prefix should use default style (not special)
            let terminal = render_widget("/search test", 12, true);
            assert_snapshot!("input_box_slash_prefix", terminal.backend());
        }
    }

    // =============================================================================
    // Multiline Tests
    // =============================================================================

    mod multiline_tests {
        use super::*;
        use insta::assert_snapshot;

        /// Helper to create a terminal for multiline tests
        fn multiline_terminal(height: u16) -> Terminal<TestBackend> {
            Terminal::new(TestBackend::new(80, height)).unwrap()
        }

        /// Render multiline widget with specified parameters
        fn render_multiline(buffer: &str, cursor_pos: usize, height: u16) -> Terminal<TestBackend> {
            let mut terminal = multiline_terminal(height);
            terminal
                .draw(|f| {
                    let widget = InputBoxWidget::new(buffer, cursor_pos);
                    f.render_widget(widget, f.area());
                })
                .unwrap();
            terminal
        }

        use test_case::test_case;

        #[test_case("", 1 ; "empty_is_one_line")]
        #[test_case("hello world", 1 ; "single_line")]
        #[test_case("line one\nline two", 2 ; "two_lines")]
        #[test_case("line one\n", 2 ; "trailing_newline")]
        #[test_case("one\ntwo\nthree\nfour\nfive", 5 ; "five_lines")]
        fn line_count(content: &str, expected: usize) {
            let widget = InputBoxWidget::new(content, 0);
            assert_eq!(widget.line_count(), expected);
        }

        #[test_case("hello\nworld", 3, 0, 3 ; "first_line_middle")]
        #[test_case("hello\nworld", 8, 1, 2 ; "second_line_after_wo")]
        #[test_case("hello\nworld", 6, 1, 0 ; "right_after_newline")]
        #[test_case("hello\nworld", 11, 1, 5 ; "end_of_buffer")]
        fn cursor_to_line_col(
            content: &str,
            cursor: usize,
            expected_line: usize,
            expected_col: usize,
        ) {
            let widget = InputBoxWidget::new(content, cursor);
            let (line, col) = widget.cursor_to_line_col();
            assert_eq!(line, expected_line);
            assert_eq!(col, expected_col);
        }

        #[test]
        fn required_height_single_line() {
            let widget = InputBoxWidget::new("hello", 0);
            assert_eq!(widget.required_height(), 3);
        }

        #[test]
        fn required_height_three_lines() {
            let widget = InputBoxWidget::new("one\ntwo\nthree", 0);
            assert_eq!(widget.required_height(), 5); // 3 lines + 2 padding
        }

        #[test]
        fn required_height_capped_at_max() {
            // 15 lines should be capped at DEFAULT_MAX_INPUT_LINES (10)
            let content = (0..15)
                .map(|i| format!("line {}", i))
                .collect::<Vec<_>>()
                .join("\n");
            let widget = InputBoxWidget::new(&content, 0);
            assert_eq!(widget.required_height(), DEFAULT_MAX_INPUT_LINES + 2);
        }

        #[test]
        fn multiline_two_lines() {
            let terminal = render_multiline("line one\nline two", 0, 5);
            assert_snapshot!("input_box_multiline_two_lines", terminal.backend());
        }

        #[test]
        fn multiline_three_lines() {
            let terminal = render_multiline("one\ntwo\nthree", 0, 6);
            assert_snapshot!("input_box_multiline_three_lines", terminal.backend());
        }

        #[test]
        fn multiline_cursor_on_second_line() {
            // Cursor on "two" (position 4+4=8, "one\ntwo" -> "one\nt" is 5 chars, +2 more = 7)
            let terminal = render_multiline("one\ntwo\nthree", 6, 6);
            assert_snapshot!("input_box_multiline_cursor_second", terminal.backend());
        }

        #[test]
        fn multiline_cursor_at_end() {
            // Cursor at the very end
            let terminal = render_multiline("one\ntwo\nthree", 13, 6);
            assert_snapshot!("input_box_multiline_cursor_end", terminal.backend());
        }

        #[test]
        fn multiline_with_trailing_newline() {
            let terminal = render_multiline("one\ntwo\n", 8, 6);
            assert_snapshot!("input_box_multiline_trailing_newline", terminal.backend());
        }
    }
}
