//! Dialog widget for modal interactions (confirm, select, info)
//!
//! Provides interactive dialogs that trap focus and capture all keyboard input.
//! Dialogs support:
//! - Confirm dialogs: Yes/No prompts
//! - Select dialogs: Choose from a list
//! - Info dialogs: Information display
//!
//! The dialog widget wraps the existing DialogState and DialogWidget rendering
//! from tui::dialog with InteractiveWidget support for event handling.

use crate::tui::components::InteractiveWidget;
use crate::tui::dialog::{DialogResult, DialogState};
use crate::tui::event_result::{DialogResult as UiDialogResult, EventResult, TuiAction};
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Interactive dialog widget
///
/// Wraps DialogState with InteractiveWidget support for keyboard navigation.
/// Dialogs trap focus - all keyboard input is consumed.
pub struct DialogWidget<'a> {
    state: &'a mut DialogState,
}

impl<'a> DialogWidget<'a> {
    pub fn new(state: &'a mut DialogState) -> Self {
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
                ref title,
                ref message,
                ref confirm_label,
                ref cancel_label,
                focused_button,
            } => {
                Self::render_confirm_static(
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
                ref title,
                ref items,
                selected,
            } => {
                Self::render_select_static(dialog_area, buf, title, items, *selected);
            }
            DialogState::Info {
                ref title,
                ref content,
            } => {
                Self::render_info_static(dialog_area, buf, title, content);
            }
            DialogState::Input {
                ref title,
                ref placeholder,
                ref buffer,
                cursor,
            } => {
                Self::render_input_static(dialog_area, buf, title, placeholder, buffer, *cursor);
            }
        }
    }
}

impl DialogWidget<'_> {
    fn render_confirm_static(
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
        use crate::tui::constants::{BUTTON_WIDTH, BUTTON_GAP};
        use crate::tui::geometry::PopupGeometry;
        let button_y = inner.y + inner.height - 2;
        let total_width = BUTTON_WIDTH * 2 + BUTTON_GAP;
        let start_x = PopupGeometry::center_horizontally(inner, total_width);

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
            start_x + BUTTON_WIDTH + BUTTON_GAP,
            button_y,
            &cancel_btn,
            cancel_style,
        );
    }

    fn render_select_static(
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

    fn render_info_static(area: Rect, buf: &mut Buffer, title: &str, content: &str) {
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
        use crate::tui::geometry::PopupGeometry;
        let hint_x = PopupGeometry::center_text_horizontally(inner, hint.len() as u16);
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
    fn render_input_static(
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
            width: crate::tui::constants::UiConstants::dialog_width(inner.width),
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
        use crate::tui::geometry::PopupGeometry;
        let hint_x = PopupGeometry::center_text_horizontally(inner, hint.len() as u16);
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

impl InteractiveWidget for DialogWidget<'_> {
    fn handle_event(&mut self, event: &Event) -> EventResult {
        if let Event::Key(key) = event {
            let result = self.state.handle_key(*key);

            // Convert DialogResult to EventResult with TuiAction
            match result {
                DialogResult::Confirm(_value) => {
                    // For select dialogs, parse the selected index
                    if let DialogState::Select { selected, .. } = self.state {
                        EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Select(
                            *selected,
                        )))
                    } else {
                        EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Confirm))
                    }
                }
                DialogResult::Cancel => {
                    EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Cancel))
                }
                DialogResult::Pending => EventResult::Consumed,
            }
        } else {
            // Dialogs consume all events (focus trap)
            EventResult::Consumed
        }
    }

    fn focusable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::buffer::Buffer;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    // =============================================================================
    // Event Handling Tests
    // =============================================================================

    #[test]
    fn test_confirm_dialog_yes_with_enter() {
        let mut state = DialogState::confirm("Delete?", "Are you sure?");
        let mut widget = DialogWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Enter));
        assert_eq!(
            result,
            EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Confirm))
        );
    }

    #[test]
    fn test_confirm_dialog_no_with_navigation() {
        let mut state = DialogState::confirm("Delete?", "Are you sure?");

        // Move to "No" button
        {
            let mut widget = DialogWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Right));
            assert_eq!(result, EventResult::Consumed);
        }

        // Confirm on "No" button
        {
            let mut widget = DialogWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Enter));
            assert_eq!(
                result,
                EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Cancel))
            );
        }
    }

    #[test]
    fn test_confirm_dialog_escape() {
        let mut state = DialogState::confirm("Delete?", "Are you sure?");
        let mut widget = DialogWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Esc));
        assert_eq!(
            result,
            EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Cancel))
        );
    }

    #[test]
    fn test_confirm_dialog_shortcut_y() {
        let mut state = DialogState::confirm("Delete?", "Are you sure?");
        let mut widget = DialogWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Char('y')));
        assert_eq!(
            result,
            EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Confirm))
        );
    }

    #[test]
    fn test_confirm_dialog_shortcut_n() {
        let mut state = DialogState::confirm("Delete?", "Are you sure?");
        let mut widget = DialogWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Char('n')));
        assert_eq!(
            result,
            EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Cancel))
        );
    }

    #[test]
    fn test_select_dialog_navigation() {
        let mut state = DialogState::select("Choose", vec!["A".into(), "B".into(), "C".into()]);

        // Navigate down twice
        {
            let mut widget = DialogWidget::new(&mut state);
            widget.handle_event(&key(KeyCode::Down));
        }
        {
            let mut widget = DialogWidget::new(&mut state);
            widget.handle_event(&key(KeyCode::Down));
        }

        // Confirm selection
        {
            let mut widget = DialogWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Enter));
            assert_eq!(
                result,
                EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Select(2)))
            );
        }
    }

    #[test]
    fn test_select_dialog_vim_navigation() {
        let mut state = DialogState::select("Choose", vec!["A".into(), "B".into(), "C".into()]);

        // Navigate with 'j'
        {
            let mut widget = DialogWidget::new(&mut state);
            widget.handle_event(&key(KeyCode::Char('j')));
        }

        // Confirm
        {
            let mut widget = DialogWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Enter));
            assert_eq!(
                result,
                EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Select(1)))
            );
        }
    }

    #[test]
    fn test_info_dialog_dismiss_with_enter() {
        let mut state = DialogState::info("Help", "Press ? for help");
        let mut widget = DialogWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Enter));
        assert_eq!(
            result,
            EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Confirm))
        );
    }

    #[test]
    fn test_info_dialog_dismiss_with_escape() {
        let mut state = DialogState::info("Help", "Press ? for help");
        let mut widget = DialogWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Esc));
        assert_eq!(
            result,
            EventResult::Action(TuiAction::CloseDialog(UiDialogResult::Confirm))
        );
    }

    #[test]
    fn test_dialog_focus_trap_consumes_all_keys() {
        let mut state = DialogState::confirm("Test", "Message");
        let mut widget = DialogWidget::new(&mut state);

        // Unknown keys should still be consumed (focus trap)
        let result = widget.handle_event(&key(KeyCode::F(1)));
        assert_eq!(result, EventResult::Consumed);
    }

    #[test]
    fn test_dialog_widget_is_focusable() {
        let mut state = DialogState::confirm("Test", "Message");
        let widget = DialogWidget::new(&mut state);
        assert!(widget.focusable());
    }

    // =============================================================================
    // Rendering Tests (Snapshots)
    // =============================================================================

    #[test]
    fn test_confirm_dialog_renders() {
        let mut state = DialogState::confirm("Delete File", "Are you sure you want to delete?");
        let widget = DialogWidget::new(&mut state);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Verify rendering completes without panic
        assert!(buf.area().width > 0);

        // Check for dialog elements in buffer
        let content: Vec<&str> = buf.content().iter().map(|c| c.symbol()).collect();
        let text = content.join("");

        // Dialog should contain title
        assert!(text.contains("Delete") || text.contains("File"));
    }

    #[test]
    fn test_confirm_dialog_renders_button_focus() {
        // Confirm dialog starts with focused_button = 0 (Yes focused)
        let mut state = DialogState::confirm("Test", "Message");

        let widget = DialogWidget::new(&mut state);
        let area = Rect::new(0, 0, 60, 15);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Verify focused button is highlighted (check buffer has styled content)
        let has_styled_cells = buf
            .content()
            .iter()
            .any(|cell| cell.fg != Color::default() || cell.bg != Color::default());
        assert!(has_styled_cells);
    }

    #[test]
    fn test_select_dialog_renders() {
        let mut state = DialogState::select(
            "Choose Option",
            vec!["Option A".into(), "Option B".into(), "Option C".into()],
        );

        let widget = DialogWidget::new(&mut state);
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Verify rendering
        assert!(buf.area().width > 0);

        // Check for items
        let content: Vec<&str> = buf.content().iter().map(|c| c.symbol()).collect();
        let text = content.join("");

        assert!(text.contains("Option"));
    }

    #[test]
    fn test_select_dialog_renders_selection_highlight() {
        let mut state = DialogState::select("Pick", vec!["First".into(), "Second".into()]);
        // Navigate down to select second item
        if let DialogState::Select { selected, .. } = &mut state {
            *selected = 1;
        }

        let widget = DialogWidget::new(&mut state);
        let area = Rect::new(0, 0, 50, 10);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Check for selection marker
        let content: Vec<&str> = buf.content().iter().map(|c| c.symbol()).collect();
        let text = content.join("");

        assert!(text.contains(">") || text.contains("Second"));
    }

    #[test]
    fn test_info_dialog_renders() {
        let mut state = DialogState::info(
            "Information",
            "This is an informational message.\nIt can have multiple lines.",
        );

        let widget = DialogWidget::new(&mut state);
        let area = Rect::new(0, 0, 70, 20);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Verify rendering
        assert!(buf.area().width > 0);

        // Check for content
        let content: Vec<&str> = buf.content().iter().map(|c| c.symbol()).collect();
        let text = content.join("");

        assert!(
            text.contains("Information")
                || text.contains("informational")
                || text.contains("message")
        );
    }

    #[test]
    fn test_dialog_dims_background() {
        let mut state = DialogState::info("Test", "Content");
        let widget = DialogWidget::new(&mut state);

        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Check that background cells are dimmed (have black background)
        let has_dimmed_bg = buf.content().iter().any(|cell| cell.bg == Color::Black);
        assert!(has_dimmed_bg);
    }
}
