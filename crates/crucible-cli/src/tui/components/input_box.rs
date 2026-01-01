//! Input box widget for text entry
//!
//! This widget provides a simple text input field with cursor support,
//! suitable for command-line style input at the bottom of the TUI.

use crate::tui::{
    components::{InteractiveWidget, WidgetEventResult},
    styles::presets,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Widget that renders an input box with cursor support
///
/// # State
///
/// - `buffer`: The current input text
/// - `cursor_position`: Character offset of the cursor (0 = before first char)
/// - `prompt`: Text to display before the input (e.g., " > ")
/// - `focused`: Whether the widget has focus (affects styling)
///
/// # Rendering
///
/// The input is displayed with the prompt on the left and the text buffer on the right.
/// When the cursor is at the end, a space is added to show the cursor position.
/// The content is centered vertically within the allocated area.
pub struct InputBoxWidget<'a> {
    buffer: &'a str,
    cursor_position: usize,
    prompt: &'a str,
    focused: bool,
}

impl<'a> InputBoxWidget<'a> {
    /// Create a new input box widget
    ///
    /// # Arguments
    ///
    /// * `buffer` - The current input text
    /// * `cursor_position` - Character offset of cursor within buffer
    pub fn new(buffer: &'a str, cursor_position: usize) -> Self {
        Self {
            buffer,
            cursor_position,
            prompt: " > ",
            focused: true,
        }
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
            // REPL command: green tint, show ":" as prompt
            (presets::input_repl(), " : ", rest)
        } else {
            // Default style and prompt
            (presets::input_box(), self.prompt, self.buffer)
        };

        // Fill background
        buf.set_style(area, style);

        // Render content with cursor, centered vertically
        // Add space at end if cursor is at the end (shows cursor position)
        let content_with_cursor = if self.cursor_position >= self.buffer.len() {
            format!("{} ", content)
        } else {
            content.to_string()
        };

        let line = Line::from(vec![
            Span::raw(display_prompt),
            Span::raw(content_with_cursor),
        ]);

        // Center vertically in the area
        let middle_row = area.y + area.height / 2;
        let centered_area = Rect {
            x: area.x,
            y: middle_row,
            width: area.width,
            height: 1,
        };

        let paragraph = Paragraph::new(line).style(style);
        paragraph.render(centered_area, buf);
    }
}

impl InteractiveWidget for InputBoxWidget<'_> {
    fn handle_event(&mut self, _event: &Event) -> WidgetEventResult {
        // Input box is managed by the runner/view layer
        // This widget is display-only - actual editing happens at a higher level
        // The runner maintains the buffer and cursor state and passes them here
        WidgetEventResult::Ignored
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
            // : prefix should use green-tinted style
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
}
