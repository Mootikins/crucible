//! Multi-line text input widget with proper paste handling.
//!
//! Used for the "Other" free-text option in ask dialogs.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// State for a multi-line text input.
#[derive(Debug, Clone, Default)]
pub struct MultiLineInputState {
    /// The text content (may contain newlines).
    pub buffer: String,
    /// Cursor position (byte offset into buffer).
    pub cursor: usize,
    /// Scroll offset (first visible line).
    pub scroll: usize,
}

impl MultiLineInputState {
    /// Create a new empty input state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with initial text.
    pub fn with_text(text: impl Into<String>) -> Self {
        let buffer = text.into();
        let cursor = buffer.len();
        Self {
            buffer,
            cursor,
            scroll: 0,
        }
    }

    /// Get the current text.
    pub fn text(&self) -> &str {
        &self.buffer
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Handle a key event, returns true if the event was consumed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char(c) => {
                // Ctrl+Enter or Shift+Enter for newline
                if c == '\n'
                    || (key.modifiers.contains(KeyModifiers::CONTROL) && c == 'j')
                    || (key.modifiers.contains(KeyModifiers::SHIFT) && c == '\n')
                {
                    self.buffer.insert(self.cursor, '\n');
                    self.cursor += 1;
                } else if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'a' => self.cursor = 0,
                        'e' => self.cursor = self.buffer.len(),
                        'u' => {
                            // Clear to start of line
                            let line_start = self.buffer[..self.cursor]
                                .rfind('\n')
                                .map(|i| i + 1)
                                .unwrap_or(0);
                            self.buffer.drain(line_start..self.cursor);
                            self.cursor = line_start;
                        }
                        'k' => {
                            // Clear to end of line
                            let line_end = self.buffer[self.cursor..]
                                .find('\n')
                                .map(|i| self.cursor + i)
                                .unwrap_or(self.buffer.len());
                            self.buffer.drain(self.cursor..line_end);
                        }
                        'w' => {
                            // Delete word backward
                            let word_start = self.buffer[..self.cursor]
                                .trim_end()
                                .rfind(|c: char| c.is_whitespace())
                                .map(|i| i + 1)
                                .unwrap_or(0);
                            self.buffer.drain(word_start..self.cursor);
                            self.cursor = word_start;
                        }
                        _ => return false,
                    }
                } else {
                    self.buffer.insert(self.cursor, c);
                    self.cursor += c.len_utf8();
                }
                true
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let prev = self.buffer[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.buffer.drain(prev..self.cursor);
                    self.cursor = prev;
                }
                true
            }
            KeyCode::Delete => {
                if self.cursor < self.buffer.len() {
                    let next = self.buffer[self.cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor + i)
                        .unwrap_or(self.buffer.len());
                    self.buffer.drain(self.cursor..next);
                }
                true
            }
            KeyCode::Left => {
                if self.cursor > 0 {
                    self.cursor = self.buffer[..self.cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                }
                true
            }
            KeyCode::Right => {
                if self.cursor < self.buffer.len() {
                    self.cursor = self.buffer[self.cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor + i)
                        .unwrap_or(self.buffer.len());
                }
                true
            }
            KeyCode::Up => {
                // Move to same column on previous line
                self.move_vertical(-1);
                true
            }
            KeyCode::Down => {
                // Move to same column on next line
                self.move_vertical(1);
                true
            }
            KeyCode::Home => {
                // Move to start of current line
                self.cursor = self.buffer[..self.cursor]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                true
            }
            KeyCode::End => {
                // Move to end of current line
                self.cursor = self.buffer[self.cursor..]
                    .find('\n')
                    .map(|i| self.cursor + i)
                    .unwrap_or(self.buffer.len());
                true
            }
            KeyCode::Enter => {
                // Regular Enter inserts newline
                self.buffer.insert(self.cursor, '\n');
                self.cursor += 1;
                true
            }
            _ => false,
        }
    }

    /// Move cursor vertically by the given number of lines.
    fn move_vertical(&mut self, delta: i32) {
        let lines: Vec<&str> = self.buffer.split('\n').collect();
        if lines.is_empty() {
            return;
        }

        // Find current line and column
        let mut current_line = 0;
        let mut col = self.cursor;
        let mut offset = 0;
        for (i, line) in lines.iter().enumerate() {
            let line_len = line.len();
            if offset + line_len >= self.cursor
                && (i == lines.len() - 1 || offset + line_len >= self.cursor)
            {
                current_line = i;
                col = self.cursor - offset;
                break;
            }
            offset += line_len + 1; // +1 for newline
        }

        // Calculate target line
        let target_line = if delta < 0 {
            current_line.saturating_sub((-delta) as usize)
        } else {
            (current_line + delta as usize).min(lines.len().saturating_sub(1))
        };

        if target_line == current_line {
            return;
        }

        // Calculate new cursor position
        let mut new_cursor = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == target_line {
                let target_col = col.min(line.len());
                new_cursor += target_col;
                break;
            }
            new_cursor += line.len() + 1;
        }
        self.cursor = new_cursor.min(self.buffer.len());
    }

    /// Insert text at cursor (for paste operations).
    pub fn insert_text(&mut self, text: &str) {
        self.buffer.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    /// Clear all text.
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.scroll = 0;
    }
}

/// Widget for rendering a multi-line input.
pub struct MultiLineInputWidget<'a> {
    state: &'a MultiLineInputState,
    placeholder: &'a str,
    focused: bool,
}

impl<'a> MultiLineInputWidget<'a> {
    /// Create a new widget.
    pub fn new(state: &'a MultiLineInputState) -> Self {
        Self {
            state,
            placeholder: "",
            focused: true,
        }
    }

    /// Set placeholder text shown when empty.
    pub fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = text;
        self
    }

    /// Set whether the widget is focused.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Calculate cursor line and column from byte offset.
    fn cursor_position(&self) -> (usize, usize) {
        let before_cursor = &self.state.buffer[..self.state.cursor];
        let line = before_cursor.matches('\n').count();
        let col = before_cursor
            .rfind('\n')
            .map(|i| self.state.cursor - i - 1)
            .unwrap_or(self.state.cursor);
        (line, col)
    }
}

impl Widget for MultiLineInputWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = if self.state.buffer.is_empty() {
            self.placeholder
        } else {
            &self.state.buffer
        };

        let text_style = if self.state.buffer.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        // Render lines
        for (i, line) in text.lines().enumerate() {
            if i >= area.height as usize {
                break;
            }
            let y = area.y + i as u16;
            let display_line: String = line.chars().take(area.width as usize).collect();
            buf.set_string(area.x, y, &display_line, text_style);
        }

        // Render cursor if focused and not showing placeholder
        if self.focused && !self.state.buffer.is_empty() {
            let (cursor_line, cursor_col) = self.cursor_position();
            if cursor_line < area.height as usize {
                let cursor_x = area.x + (cursor_col as u16).min(area.width.saturating_sub(1));
                let cursor_y = area.y + cursor_line as u16;
                if let Some(cell) = buf.cell_mut((cursor_x, cursor_y)) {
                    cell.set_style(
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::White)
                            .add_modifier(Modifier::SLOW_BLINK),
                    );
                }
            }
        } else if self.focused && self.state.buffer.is_empty() {
            // Show cursor at start when empty and focused
            if let Some(cell) = buf.cell_mut((area.x, area.y)) {
                cell.set_style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::SLOW_BLINK),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn basic_typing() {
        let mut state = MultiLineInputState::new();
        state.handle_key(key(KeyCode::Char('H')));
        state.handle_key(key(KeyCode::Char('i')));
        assert_eq!(state.text(), "Hi");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn backspace() {
        let mut state = MultiLineInputState::with_text("abc");
        state.handle_key(key(KeyCode::Backspace));
        assert_eq!(state.text(), "ab");
    }

    #[test]
    fn ctrl_a_moves_to_start() {
        let mut state = MultiLineInputState::with_text("hello");
        state.handle_key(ctrl('a'));
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn ctrl_e_moves_to_end() {
        let mut state = MultiLineInputState::with_text("hello");
        state.cursor = 0;
        state.handle_key(ctrl('e'));
        assert_eq!(state.cursor, 5);
    }

    #[test]
    fn insert_text_for_paste() {
        let mut state = MultiLineInputState::with_text("ac");
        state.cursor = 1;
        state.insert_text("b");
        assert_eq!(state.text(), "abc");
    }

    #[test]
    fn multiline_text() {
        let mut state = MultiLineInputState::new();
        state.insert_text("line1\nline2\nline3");
        assert_eq!(state.buffer.lines().count(), 3);
    }

    #[test]
    fn enter_inserts_newline() {
        let mut state = MultiLineInputState::with_text("hello");
        state.cursor = 5;
        state.handle_key(key(KeyCode::Enter));
        assert_eq!(state.text(), "hello\n");
    }

    #[test]
    fn left_right_navigation() {
        let mut state = MultiLineInputState::with_text("abc");
        assert_eq!(state.cursor, 3);
        state.handle_key(key(KeyCode::Left));
        assert_eq!(state.cursor, 2);
        state.handle_key(key(KeyCode::Left));
        assert_eq!(state.cursor, 1);
        state.handle_key(key(KeyCode::Right));
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn home_end_navigation() {
        let mut state = MultiLineInputState::with_text("hello world");
        state.handle_key(key(KeyCode::Home));
        assert_eq!(state.cursor, 0);
        state.handle_key(key(KeyCode::End));
        assert_eq!(state.cursor, 11);
    }

    #[test]
    fn ctrl_w_deletes_word() {
        let mut state = MultiLineInputState::with_text("one two three");
        state.handle_key(ctrl('w'));
        assert_eq!(state.text(), "one two ");
    }

    #[test]
    fn ctrl_u_clears_to_line_start() {
        let mut state = MultiLineInputState::with_text("hello world");
        state.cursor = 6; // after "hello "
        state.handle_key(ctrl('u'));
        assert_eq!(state.text(), "world");
    }

    #[test]
    fn delete_key() {
        let mut state = MultiLineInputState::with_text("abc");
        state.cursor = 0;
        state.handle_key(key(KeyCode::Delete));
        assert_eq!(state.text(), "bc");
    }

    #[test]
    fn clear() {
        let mut state = MultiLineInputState::with_text("hello");
        state.clear();
        assert!(state.is_empty());
        assert_eq!(state.cursor, 0);
    }
}
