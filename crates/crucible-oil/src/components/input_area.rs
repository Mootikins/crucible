use crate::focus::FocusContext;
use crate::node::{col, focusable_auto, row, styled, InputNode, Node};
use crate::style::{Color, Style};

pub const INPUT_MAX_CONTENT_LINES: usize = 3;
const FOCUS_INPUT: &str = "input";

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

/// Generic input area component that works with any InputStyle implementation
#[derive(Debug, Clone)]
pub struct InputArea {
    pub content: String,
    pub cursor: usize,
    pub width: usize,
    pub focused: bool,
    pub show_popup: bool,
}

impl InputArea {
    pub fn new(content: impl Into<String>, cursor: usize, width: usize) -> Self {
        Self {
            content: content.into(),
            cursor,
            width,
            focused: true,
            show_popup: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn with_popup(mut self, show_popup: bool) -> Self {
        self.show_popup = show_popup;
        self
    }

    /// Render the input area with the given style
    pub fn view<S: InputStyle>(&self, style: &S, focus: &FocusContext) -> Node {
        let prompt = style.prompt();
        let bg = style.bg_color();

        let top_edge = styled("▄".repeat(self.width), Style::new().fg(bg));
        let bottom_edge = styled("▀".repeat(self.width), Style::new().fg(bg));

        let display_content = style.display_content(&self.content);
        let display_cursor = style.display_cursor(self.cursor);

        let content_width = self.width.saturating_sub(prompt.len() + 1);
        let all_lines = wrap_content(display_content, content_width);

        let (cursor_line, cursor_col) = if content_width > 0 && !all_lines.is_empty() {
            let line_idx = display_cursor / content_width;
            let col_in_line = display_cursor % content_width;
            (line_idx.min(all_lines.len() - 1), col_in_line)
        } else {
            (0, display_cursor)
        };

        let (visible_lines, visible_cursor_line) =
            clamp_input_lines(&all_lines, cursor_line, INPUT_MAX_CONTENT_LINES);

        let is_focused = self.focused && (!self.show_popup || focus.is_focused(FOCUS_INPUT));

        let mut rows: Vec<Node> = Vec::with_capacity(INPUT_MAX_CONTENT_LINES + 2);
        rows.push(top_edge);

        for (i, line) in visible_lines.iter().enumerate() {
            let line_len = line.chars().count();
            let line_padding = " ".repeat(content_width.saturating_sub(line_len) + 1);
            let is_first_visible = i == 0 && visible_lines.len() == all_lines.len();
            let line_prefix = if is_first_visible { prompt } else { "   " };

            if i == visible_cursor_line && is_focused {
                rows.push(row([
                    styled(line_prefix, Style::new().bg(bg)),
                    Node::Input(InputNode {
                        value: line.to_string(),
                        cursor: cursor_col.min(line_len),
                        placeholder: None,
                        style: Style::new().bg(bg),
                        focused: true,
                    }),
                    styled(line_padding, Style::new().bg(bg)),
                ]));
            } else {
                rows.push(styled(
                    format!("{}{}{}", line_prefix, line, line_padding),
                    Style::new().bg(bg),
                ));
            }
        }

        rows.push(bottom_edge);

        let input_node = col(rows);
        focusable_auto(FOCUS_INPUT, input_node)
    }
}

/// Wrap content to fit within max_width, breaking at character boundaries
pub fn wrap_content(content: &str, max_width: usize) -> Vec<String> {
    if content.is_empty() || max_width == 0 {
        return vec![String::new()];
    }

    let chars: Vec<char> = content.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let end = (start + max_width).min(chars.len());
        lines.push(chars[start..end].iter().collect());
        start = end;
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Clamp visible lines to max_lines, keeping cursor line visible
pub fn clamp_input_lines(
    lines: &[String],
    cursor_line: usize,
    max_lines: usize,
) -> (Vec<String>, usize) {
    if lines.len() <= max_lines {
        return (lines.to_vec(), cursor_line);
    }

    let half = max_lines / 2;
    let start = if cursor_line <= half {
        0
    } else if cursor_line >= lines.len() - half {
        lines.len() - max_lines
    } else {
        cursor_line - half
    };

    let end = (start + max_lines).min(lines.len());
    let visible = lines[start..end].to_vec();
    let adjusted_cursor = cursor_line - start;

    (visible, adjusted_cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestStyle {
        bg: Color,
        prompt_text: &'static str,
    }

    impl InputStyle for TestStyle {
        fn bg_color(&self) -> Color {
            self.bg
        }

        fn prompt(&self) -> &'static str {
            self.prompt_text
        }
    }

    #[test]
    fn wrap_content_handles_empty() {
        assert_eq!(wrap_content("", 10), vec![""]);
    }

    #[test]
    fn wrap_content_splits_at_width() {
        let result = wrap_content("abcdefghij", 5);
        assert_eq!(result, vec!["abcde", "fghij"]);
    }

    #[test]
    fn clamp_lines_returns_all_when_fits() {
        let lines: Vec<String> = vec!["a".into(), "b".into()];
        let (result, cursor) = clamp_input_lines(&lines, 0, 3);
        assert_eq!(result, lines);
        assert_eq!(cursor, 0);
    }

    #[test]
    fn clamp_lines_follows_cursor() {
        let lines: Vec<String> = (0..10).map(|i| format!("line {}", i)).collect();
        let (result, cursor) = clamp_input_lines(&lines, 5, 3);
        assert_eq!(result.len(), 3);
        assert!(cursor < 3);
    }

    #[test]
    fn input_area_renders_with_style() {
        let input = InputArea::new("hello", 5, 80);
        let style = TestStyle {
            bg: Color::Blue,
            prompt_text: " > ",
        };
        let focus = FocusContext::new();
        let _node = input.view(&style, &focus);
        // Just verify it doesn't panic
    }
}
