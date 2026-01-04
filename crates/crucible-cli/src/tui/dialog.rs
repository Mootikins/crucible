//! Modal dialog system for TUI
//!
//! Provides reusable dialog types (confirm, select, info) with:
//! - Focus trapping: dialogs capture all keyboard input
//! - Keyboard navigation: Tab, Arrow keys, Enter, Escape
//! - Shortcuts: y/n for confirm, j/k for select
//! - Centered rendering with dimmed background

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

// =============================================================================
// Dialog Types
// =============================================================================

/// Result from dialog interaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResult {
    /// User confirmed/selected (with value)
    Confirm(String),
    /// User cancelled
    Cancel,
    /// Dialog still active
    Pending,
}

/// Dialog state - each variant contains all state it needs
///
/// This is a proper enum-with-data pattern: no separate discriminant field,
/// and each variant carries only the fields relevant to that dialog type.
#[derive(Debug, Clone)]
pub enum DialogState {
    /// Yes/No confirmation dialog
    Confirm {
        title: String,
        message: String,
        confirm_label: String,
        cancel_label: String,
        /// 0 = confirm button focused, 1 = cancel button focused
        focused_button: usize,
    },
    /// Select from list dialog
    Select {
        title: String,
        items: Vec<String>,
        /// Currently selected item index
        selected: usize,
    },
    /// Information display dialog
    Info { title: String, content: String },
    /// Text input dialog for written answers
    Input {
        title: String,
        /// Optional placeholder/hint text
        placeholder: String,
        /// Current text buffer
        buffer: String,
        /// Cursor position in buffer (byte offset)
        cursor: usize,
    },
}

impl DialogState {
    /// Create a confirmation dialog
    pub fn confirm(title: impl Into<String>, message: impl Into<String>) -> Self {
        DialogState::Confirm {
            title: title.into(),
            message: message.into(),
            confirm_label: "Yes".into(),
            cancel_label: "No".into(),
            focused_button: 0,
        }
    }

    /// Create a selection dialog
    pub fn select(title: impl Into<String>, items: Vec<String>) -> Self {
        DialogState::Select {
            title: title.into(),
            items,
            selected: 0,
        }
    }

    /// Create an info dialog
    pub fn info(title: impl Into<String>, content: impl Into<String>) -> Self {
        DialogState::Info {
            title: title.into(),
            content: content.into(),
        }
    }

    /// Create a text input dialog
    pub fn input(title: impl Into<String>, placeholder: impl Into<String>) -> Self {
        DialogState::Input {
            title: title.into(),
            placeholder: placeholder.into(),
            buffer: String::new(),
            cursor: 0,
        }
    }

    /// Handle key input, returning result
    pub fn handle_key(&mut self, key: KeyEvent) -> DialogResult {
        match self {
            DialogState::Confirm {
                confirm_label,
                focused_button,
                ..
            } => match key.code {
                KeyCode::Left | KeyCode::Char('h') => {
                    *focused_button = 0;
                    DialogResult::Pending
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    *focused_button = 1;
                    DialogResult::Pending
                }
                KeyCode::Tab => {
                    *focused_button = (*focused_button + 1) % 2;
                    DialogResult::Pending
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if *focused_button == 0 {
                        DialogResult::Confirm(confirm_label.clone())
                    } else {
                        DialogResult::Cancel
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => DialogResult::Cancel,
                KeyCode::Char('y') => DialogResult::Confirm(confirm_label.clone()),
                KeyCode::Char('n') => DialogResult::Cancel,
                _ => DialogResult::Pending,
            },
            DialogState::Select {
                items, selected, ..
            } => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    *selected = selected.saturating_sub(1);
                    DialogResult::Pending
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    *selected = (*selected + 1).min(items.len().saturating_sub(1));
                    DialogResult::Pending
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if let Some(item) = items.get(*selected) {
                        DialogResult::Confirm(item.clone())
                    } else {
                        DialogResult::Cancel
                    }
                }
                KeyCode::Esc | KeyCode::Char('q') => DialogResult::Cancel,
                _ => DialogResult::Pending,
            },
            DialogState::Info { .. } => match key.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') | KeyCode::Char('q') => {
                    DialogResult::Confirm("ok".into())
                }
                _ => DialogResult::Pending,
            },
            DialogState::Input {
                buffer, cursor, ..
            } => match key.code {
                KeyCode::Enter => {
                    if buffer.is_empty() {
                        DialogResult::Cancel
                    } else {
                        DialogResult::Confirm(buffer.clone())
                    }
                }
                KeyCode::Esc => DialogResult::Cancel,
                KeyCode::Backspace => {
                    if *cursor > 0 {
                        // Find the previous character boundary
                        let prev_cursor = buffer[..*cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        buffer.drain(prev_cursor..*cursor);
                        *cursor = prev_cursor;
                    }
                    DialogResult::Pending
                }
                KeyCode::Delete => {
                    if *cursor < buffer.len() {
                        // Find the next character boundary
                        let next_cursor = buffer[*cursor..]
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| *cursor + i)
                            .unwrap_or(buffer.len());
                        buffer.drain(*cursor..next_cursor);
                    }
                    DialogResult::Pending
                }
                KeyCode::Left => {
                    if *cursor > 0 {
                        *cursor = buffer[..*cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                    }
                    DialogResult::Pending
                }
                KeyCode::Right => {
                    if *cursor < buffer.len() {
                        *cursor = buffer[*cursor..]
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| *cursor + i)
                            .unwrap_or(buffer.len());
                    }
                    DialogResult::Pending
                }
                KeyCode::Home => {
                    *cursor = 0;
                    DialogResult::Pending
                }
                KeyCode::End => {
                    *cursor = buffer.len();
                    DialogResult::Pending
                }
                KeyCode::Char(c) => {
                    buffer.insert(*cursor, c);
                    *cursor += c.len_utf8();
                    DialogResult::Pending
                }
                _ => DialogResult::Pending,
            },
        }
    }
}

// =============================================================================
// Dialog Widget
// =============================================================================

/// Widget for rendering dialogs
pub struct DialogWidget<'a> {
    state: &'a DialogState,
}

impl<'a> DialogWidget<'a> {
    pub fn new(state: &'a DialogState) -> Self {
        Self { state }
    }

    /// Calculate centered dialog area
    fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(area);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}

impl Widget for DialogWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Dim background
        let dim_style = Style::default().bg(Color::Black);
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(dim_style);
                }
            }
        }

        // Calculate dialog size based on content
        let dialog_area = match self.state {
            DialogState::Confirm { .. } => Self::centered_rect(50, 30, area),
            DialogState::Select { items, .. } => {
                let height = (items.len() + 4).min(20) as u16;
                let height_percent = (height * 100 / area.height).clamp(30, 80);
                Self::centered_rect(50, height_percent, area)
            }
            DialogState::Info { .. } => Self::centered_rect(60, 40, area),
            DialogState::Input { .. } => Self::centered_rect(60, 25, area),
        };

        // Clear dialog area
        Clear.render(dialog_area, buf);

        // Render based on dialog type
        match self.state {
            DialogState::Confirm {
                title,
                message,
                confirm_label,
                cancel_label,
                focused_button,
            } => {
                self.render_confirm(
                    dialog_area,
                    buf,
                    title,
                    message,
                    confirm_label,
                    cancel_label,
                    *focused_button,
                );
            }
            DialogState::Select {
                title,
                items,
                selected,
            } => {
                self.render_select(dialog_area, buf, title, items, *selected);
            }
            DialogState::Info { title, content } => {
                self.render_info(dialog_area, buf, title, content);
            }
            DialogState::Input {
                title,
                placeholder,
                buffer,
                cursor,
            } => {
                self.render_input(dialog_area, buf, title, placeholder, buffer, *cursor);
            }
        }
    }
}

impl DialogWidget<'_> {
    #[allow(clippy::too_many_arguments)]
    fn render_confirm(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &str,
        message: &str,
        confirm_label: &str,
        cancel_label: &str,
        focused_button: usize,
    ) {
        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        block.render(area, buf);

        // Message
        let message_para = Paragraph::new(message).alignment(Alignment::Center);
        let msg_area = Rect {
            x: inner.x,
            y: inner.y + 1,
            width: inner.width,
            height: 2,
        };
        message_para.render(msg_area, buf);

        // Buttons
        let button_y = inner.y + inner.height - 2;
        let btn_width = 10u16;
        let gap = 4u16;
        let total_width = btn_width * 2 + gap;
        let start_x = inner.x + (inner.width.saturating_sub(total_width)) / 2;

        // Confirm button
        let confirm_style = if focused_button == 0 {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };
        let confirm_btn = format!("[{}]", confirm_label);
        buf.set_string(start_x, button_y, &confirm_btn, confirm_style);

        // Cancel button
        let cancel_style = if focused_button == 1 {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        let cancel_btn = format!("[{}]", cancel_label);
        buf.set_string(
            start_x + btn_width + gap,
            button_y,
            &cancel_btn,
            cancel_style,
        );
    }

    fn render_select(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &str,
        items: &[String],
        selected: usize,
    ) {
        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        block.render(area, buf);

        // Render items
        for (i, item) in items.iter().enumerate() {
            if i >= inner.height as usize {
                break;
            }
            let style = if i == selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if i == selected { "> " } else { "  " };
            let line = format!("{}{}", prefix, item);
            buf.set_string(inner.x, inner.y + i as u16, &line, style);
        }
    }

    fn render_info(&self, area: Rect, buf: &mut Buffer, title: &str, content: &str) {
        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        block.render(area, buf);

        let para = Paragraph::new(content).wrap(ratatui::widgets::Wrap { trim: false });
        para.render(inner, buf);

        // Hint at bottom
        let hint = "[Press Enter or Esc to close]";
        let hint_x = inner.x + (inner.width.saturating_sub(hint.len() as u16)) / 2;
        if inner.height > 0 {
            buf.set_string(
                hint_x,
                inner.y + inner.height - 1,
                hint,
                Style::default().fg(Color::DarkGray),
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_input(
        &self,
        area: Rect,
        buf: &mut Buffer,
        title: &str,
        placeholder: &str,
        buffer: &str,
        cursor: usize,
    ) {
        let block = Block::default()
            .title(format!(" {} ", title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        block.render(area, buf);

        // Show placeholder if buffer is empty
        let display_text = if buffer.is_empty() {
            placeholder
        } else {
            buffer
        };

        // Calculate visible portion of text
        let text_style = if buffer.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default()
        };

        // Input field background - show it centered vertically
        let input_y = inner.y + (inner.height.saturating_sub(3)) / 2;
        let input_area = Rect {
            x: inner.x + 1,
            y: input_y,
            width: inner.width.saturating_sub(2),
            height: 1,
        };

        // Draw input background
        let input_bg = Style::default().bg(Color::DarkGray);
        for x in input_area.x..input_area.x + input_area.width {
            if let Some(cell) = buf.cell_mut((x, input_area.y)) {
                cell.set_char(' ');
                cell.set_style(input_bg);
            }
        }

        // Draw the text
        let visible_width = input_area.width as usize;
        let (display_start, cursor_screen_pos) = if cursor > visible_width.saturating_sub(5) {
            // Scroll the view if cursor is near the right edge
            let start = cursor.saturating_sub(visible_width.saturating_sub(5));
            (start, cursor - start)
        } else {
            (0, cursor)
        };

        let visible_text: String = display_text
            .chars()
            .skip(display_start)
            .take(visible_width)
            .collect();

        buf.set_string(input_area.x, input_area.y, &visible_text, text_style);

        // Draw cursor if buffer is not empty (or always show it)
        if !buffer.is_empty() {
            let cursor_x = input_area.x + cursor_screen_pos as u16;
            if cursor_x < input_area.x + input_area.width {
                if let Some(cell) = buf.cell_mut((cursor_x, input_area.y)) {
                    cell.set_style(
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::White)
                            .add_modifier(Modifier::SLOW_BLINK),
                    );
                }
            }
        }

        // Hint at bottom
        let hint = "[Enter to submit, Esc to cancel]";
        let hint_x = inner.x + (inner.width.saturating_sub(hint.len() as u16)) / 2;
        if inner.height > 2 {
            buf.set_string(
                hint_x,
                inner.y + inner.height - 1,
                hint,
                Style::default().fg(Color::DarkGray),
            );
        }
    }
}

// =============================================================================
// Dialog Stack
// =============================================================================

/// Stack-based dialog manager for nested dialogs
#[derive(Debug, Default)]
pub struct DialogStack {
    dialogs: Vec<DialogState>,
}

impl DialogStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, dialog: DialogState) {
        self.dialogs.push(dialog);
    }

    pub fn pop(&mut self) -> Option<DialogState> {
        self.dialogs.pop()
    }

    pub fn current(&self) -> Option<&DialogState> {
        self.dialogs.last()
    }

    pub fn current_mut(&mut self) -> Option<&mut DialogState> {
        self.dialogs.last_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.dialogs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.dialogs.len()
    }

    /// Handle key event for current dialog
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<DialogResult> {
        if let Some(dialog) = self.current_mut() {
            let result = dialog.handle_key(key);
            match &result {
                DialogResult::Confirm(_) | DialogResult::Cancel => {
                    self.pop();
                }
                DialogResult::Pending => {}
            }
            Some(result)
        } else {
            None
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_confirm_dialog_yes() {
        let mut dialog = DialogState::confirm("Delete?", "Are you sure?");
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("Yes".into())
        );
    }

    #[test]
    fn test_confirm_dialog_no() {
        let mut dialog = DialogState::confirm("Delete?", "Are you sure?");
        dialog.handle_key(key(KeyCode::Right)); // Move to No
        assert_eq!(dialog.handle_key(key(KeyCode::Enter)), DialogResult::Cancel);
    }

    #[test]
    fn test_confirm_dialog_escape() {
        let mut dialog = DialogState::confirm("Delete?", "Are you sure?");
        assert_eq!(dialog.handle_key(key(KeyCode::Esc)), DialogResult::Cancel);
    }

    #[test]
    fn test_confirm_dialog_shortcut_y() {
        let mut dialog = DialogState::confirm("Delete?", "Are you sure?");
        assert_eq!(
            dialog.handle_key(key(KeyCode::Char('y'))),
            DialogResult::Confirm("Yes".into())
        );
    }

    #[test]
    fn test_confirm_dialog_shortcut_n() {
        let mut dialog = DialogState::confirm("Delete?", "Are you sure?");
        assert_eq!(
            dialog.handle_key(key(KeyCode::Char('n'))),
            DialogResult::Cancel
        );
    }

    #[test]
    fn test_select_dialog_navigation() {
        let mut dialog = DialogState::select("Choose", vec!["A".into(), "B".into(), "C".into()]);
        dialog.handle_key(key(KeyCode::Down));
        dialog.handle_key(key(KeyCode::Down));
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("C".into())
        );
    }

    #[test]
    fn test_select_dialog_vim_navigation() {
        let mut dialog = DialogState::select("Choose", vec!["A".into(), "B".into(), "C".into()]);
        dialog.handle_key(key(KeyCode::Char('j')));
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("B".into())
        );
    }

    #[test]
    fn test_info_dialog_dismiss() {
        let mut dialog = DialogState::info("Help", "Press ? for help");
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("ok".into())
        );
    }

    #[test]
    fn test_dialog_stack() {
        let mut stack = DialogStack::new();
        assert!(stack.is_empty());

        stack.push(DialogState::confirm("First", "Message"));
        stack.push(DialogState::info("Second", "Content"));
        assert_eq!(stack.len(), 2);

        // Handle key dismisses top dialog
        stack.handle_key(key(KeyCode::Enter));
        assert_eq!(stack.len(), 1);

        // Handle key dismisses remaining dialog
        stack.handle_key(key(KeyCode::Char('y')));
        assert!(stack.is_empty());
    }

    #[test]
    fn test_dialog_stack_cancel() {
        let mut stack = DialogStack::new();
        stack.push(DialogState::confirm("Confirm", "Sure?"));

        let result = stack.handle_key(key(KeyCode::Esc));
        assert_eq!(result, Some(DialogResult::Cancel));
        assert!(stack.is_empty());
    }

    #[test]
    fn test_select_dialog_boundary_navigation() {
        let mut dialog = DialogState::select("Choose", vec!["A".into(), "B".into()]);

        // At top, pressing up should not go negative
        dialog.handle_key(key(KeyCode::Up));
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("A".into())
        );

        // At bottom, pressing down should not overflow
        let mut dialog = DialogState::select("Choose", vec!["A".into(), "B".into()]);
        dialog.handle_key(key(KeyCode::Down));
        dialog.handle_key(key(KeyCode::Down));
        dialog.handle_key(key(KeyCode::Down)); // Should stay at B
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("B".into())
        );
    }

    // =========================================================================
    // Input dialog tests
    // =========================================================================

    #[test]
    fn test_input_dialog_typing() {
        let mut dialog = DialogState::input("Enter name", "Type here...");
        dialog.handle_key(key(KeyCode::Char('H')));
        dialog.handle_key(key(KeyCode::Char('e')));
        dialog.handle_key(key(KeyCode::Char('l')));
        dialog.handle_key(key(KeyCode::Char('l')));
        dialog.handle_key(key(KeyCode::Char('o')));

        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("Hello".into())
        );
    }

    #[test]
    fn test_input_dialog_backspace() {
        let mut dialog = DialogState::input("Enter", "");
        dialog.handle_key(key(KeyCode::Char('a')));
        dialog.handle_key(key(KeyCode::Char('b')));
        dialog.handle_key(key(KeyCode::Char('c')));
        dialog.handle_key(key(KeyCode::Backspace));

        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("ab".into())
        );
    }

    #[test]
    fn test_input_dialog_escape_cancels() {
        let mut dialog = DialogState::input("Enter", "");
        dialog.handle_key(key(KeyCode::Char('t')));
        dialog.handle_key(key(KeyCode::Char('e')));

        assert_eq!(dialog.handle_key(key(KeyCode::Esc)), DialogResult::Cancel);
    }

    #[test]
    fn test_input_dialog_empty_cancels() {
        let mut dialog = DialogState::input("Enter", "");
        // Enter on empty buffer should cancel
        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Cancel
        );
    }

    #[test]
    fn test_input_dialog_cursor_navigation() {
        let mut dialog = DialogState::input("Edit", "");
        dialog.handle_key(key(KeyCode::Char('a')));
        dialog.handle_key(key(KeyCode::Char('c')));
        // Move left
        dialog.handle_key(key(KeyCode::Left));
        // Insert 'b' between 'a' and 'c'
        dialog.handle_key(key(KeyCode::Char('b')));

        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("abc".into())
        );
    }

    #[test]
    fn test_input_dialog_home_end() {
        let mut dialog = DialogState::input("Edit", "");
        dialog.handle_key(key(KeyCode::Char('b')));
        dialog.handle_key(key(KeyCode::Char('c')));
        // Move to start
        dialog.handle_key(key(KeyCode::Home));
        // Insert 'a' at start
        dialog.handle_key(key(KeyCode::Char('a')));

        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("abc".into())
        );
    }

    #[test]
    fn test_input_dialog_delete() {
        let mut dialog = DialogState::input("Edit", "");
        dialog.handle_key(key(KeyCode::Char('a')));
        dialog.handle_key(key(KeyCode::Char('X')));
        dialog.handle_key(key(KeyCode::Char('b')));
        // Move left twice
        dialog.handle_key(key(KeyCode::Left));
        dialog.handle_key(key(KeyCode::Left));
        // Delete the 'X'
        dialog.handle_key(key(KeyCode::Delete));

        assert_eq!(
            dialog.handle_key(key(KeyCode::Enter)),
            DialogResult::Confirm("ab".into())
        );
    }
}
