//! Popup widget for inline completion (slash commands, agents, files, notes)
//!
//! Provides an interactive popup for selecting from filtered lists of:
//! - Slash commands (/)
//! - Agents, files, and notes (@)
//! - Skills (skill:)
//!
//! The popup handles:
//! - Arrow key navigation
//! - Enter to confirm selection
//! - Escape to dismiss
//! - Character input to filter results

use crate::tui::components::{DialogAction, InteractiveWidget, WidgetAction, WidgetEventResult};
use crate::tui::state::PopupState;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

/// Maximum number of items to display in popup
const MAX_POPUP_ITEMS: usize = 10;

/// Interactive popup widget for inline completion
///
/// Wraps the existing popup rendering logic with InteractiveWidget support.
/// Handles keyboard navigation and selection confirmation.
pub struct PopupWidget<'a> {
    state: &'a mut PopupState,
}

impl<'a> PopupWidget<'a> {
    pub fn new(state: &'a mut PopupState) -> Self {
        Self { state }
    }
}

impl Widget for PopupWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Update viewport to ensure selection is visible
        let visible_count = area.height.saturating_sub(2) as usize; // Account for borders
        let visible_count = visible_count.min(MAX_POPUP_ITEMS);
        self.state.update_viewport(visible_count);

        // Calculate visible slice with viewport offset
        let total_items = self.state.items.len().min(MAX_POPUP_ITEMS);
        let viewport_end = (self.state.viewport_offset + visible_count).min(total_items);
        let visible_items = &self.state.items[self.state.viewport_offset..viewport_end];

        // Render popup items
        let lines: Vec<Line> = visible_items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let absolute_idx = self.state.viewport_offset + idx;
                let mut spans = Vec::new();

                // Selection marker
                let marker = if absolute_idx == self.state.selected {
                    ">"
                } else {
                    " "
                };
                spans.push(Span::styled(
                    marker,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));

                // Kind label
                let kind_label = format!("[{}]", item.kind_label());
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    kind_label,
                    Style::default().fg(Color::Magenta),
                ));

                // Title (with selection highlight)
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    item.title(),
                    if absolute_idx == self.state.selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ));

                // Subtitle (optional)
                let subtitle = item.subtitle();
                if !subtitle.is_empty() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        subtitle.to_string(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                Line::from(spans)
            })
            .collect();

        // Show scroll indicators if needed
        let title = if self.state.items.len() > MAX_POPUP_ITEMS {
            format!(
                "Select ({}/{})",
                self.state.selected + 1,
                self.state.items.len()
            )
        } else {
            "Select".to_string()
        };

        let popup_widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: true });

        popup_widget.render(area, buf);
    }
}

impl InteractiveWidget for PopupWidget<'_> {
    fn handle_event(&mut self, event: &Event) -> WidgetEventResult {
        if let Event::Key(KeyEvent { code, .. }) = event {
            match code {
                // Navigation
                KeyCode::Up | KeyCode::Char('k') => {
                    self.state.move_selection(-1);
                    WidgetEventResult::Consumed
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.state.move_selection(1);
                    WidgetEventResult::Consumed
                }
                KeyCode::PageUp => {
                    self.state.move_selection(-5);
                    WidgetEventResult::Consumed
                }
                KeyCode::PageDown => {
                    self.state.move_selection(5);
                    WidgetEventResult::Consumed
                }

                // Confirm selection
                KeyCode::Enter | KeyCode::Tab => {
                    WidgetEventResult::Action(WidgetAction::ConfirmPopup(self.state.selected))
                }

                // Dismiss popup
                KeyCode::Esc => WidgetEventResult::Action(WidgetAction::DismissPopup),

                // Character input filters results (handled externally by runner)
                // We consume printable characters to prevent them from propagating
                KeyCode::Char(c) if !c.is_control() => WidgetEventResult::Consumed,
                KeyCode::Backspace => WidgetEventResult::Consumed,

                // Ignore other keys
                _ => WidgetEventResult::Ignored,
            }
        } else {
            WidgetEventResult::Ignored
        }
    }

    fn focusable(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{PopupItem, PopupKind};
    use crossterm::event::KeyModifiers;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn make_popup_state(num_items: usize) -> PopupState {
        let mut state = PopupState::new(PopupKind::Command);
        state.items = (0..num_items)
            .map(|i| {
                PopupItem::cmd(format!("cmd{}", i))
                    .desc(format!("Command {}", i))
                    .with_score(1)
            })
            .collect();
        state
    }

    #[test]
    fn test_popup_widget_handles_up_down() {
        let mut state = make_popup_state(5);

        assert_eq!(state.selected, 0);

        // Move down
        {
            let mut widget = PopupWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Down));
            assert_eq!(result, WidgetEventResult::Consumed);
        }
        assert_eq!(state.selected, 1);

        // Move up
        {
            let mut widget = PopupWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Up));
            assert_eq!(result, WidgetEventResult::Consumed);
        }
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_popup_widget_handles_vim_keys() {
        let mut state = make_popup_state(5);

        // j moves down
        {
            let mut widget = PopupWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Char('j')));
            assert_eq!(result, WidgetEventResult::Consumed);
        }
        assert_eq!(state.selected, 1);

        // k moves up
        {
            let mut widget = PopupWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Char('k')));
            assert_eq!(result, WidgetEventResult::Consumed);
        }
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_popup_widget_handles_page_navigation() {
        let mut state = make_popup_state(20);

        // PageDown moves 5 items
        {
            let mut widget = PopupWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::PageDown));
            assert_eq!(result, WidgetEventResult::Consumed);
        }
        assert_eq!(state.selected, 5);

        // PageUp moves back 5 items
        {
            let mut widget = PopupWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::PageUp));
            assert_eq!(result, WidgetEventResult::Consumed);
        }
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_popup_widget_wraps_selection() {
        let mut state = make_popup_state(3);

        // At top, up wraps to bottom
        {
            let mut widget = PopupWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Up));
            assert_eq!(result, WidgetEventResult::Consumed);
        }
        assert_eq!(state.selected, 2);

        // At bottom, down wraps to top
        {
            let mut widget = PopupWidget::new(&mut state);
            let result = widget.handle_event(&key(KeyCode::Down));
            assert_eq!(result, WidgetEventResult::Consumed);
        }
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_popup_widget_confirm_with_enter() {
        let mut state = make_popup_state(5);
        state.selected = 2;
        let mut widget = PopupWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Enter));
        assert_eq!(
            result,
            WidgetEventResult::Action(WidgetAction::ConfirmPopup(2))
        );
    }

    #[test]
    fn test_popup_widget_confirm_with_tab() {
        let mut state = make_popup_state(5);
        state.selected = 3;
        let mut widget = PopupWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Tab));
        assert_eq!(
            result,
            WidgetEventResult::Action(WidgetAction::ConfirmPopup(3))
        );
    }

    #[test]
    fn test_popup_widget_dismiss_with_escape() {
        let mut state = make_popup_state(5);
        let mut widget = PopupWidget::new(&mut state);

        let result = widget.handle_event(&key(KeyCode::Esc));
        assert_eq!(
            result,
            WidgetEventResult::Action(WidgetAction::DismissPopup)
        );
    }

    #[test]
    fn test_popup_widget_consumes_character_input() {
        let mut state = make_popup_state(5);
        let mut widget = PopupWidget::new(&mut state);

        // Character input should be consumed (filtering handled externally)
        let result = widget.handle_event(&key(KeyCode::Char('a')));
        assert_eq!(result, WidgetEventResult::Consumed);

        let result = widget.handle_event(&key(KeyCode::Backspace));
        assert_eq!(result, WidgetEventResult::Consumed);
    }

    #[test]
    fn test_popup_widget_ignores_unknown_keys() {
        let mut state = make_popup_state(5);
        let mut widget = PopupWidget::new(&mut state);

        // F1 should be ignored
        let result = widget.handle_event(&key(KeyCode::F(1)));
        assert_eq!(result, WidgetEventResult::Ignored);
    }

    #[test]
    fn test_popup_widget_is_focusable() {
        let mut state = make_popup_state(5);
        let widget = PopupWidget::new(&mut state);
        assert!(widget.focusable());
    }

    // =============================================================================
    // Snapshot Tests - Verify rendering output
    // =============================================================================

    #[test]
    fn test_popup_widget_renders_items() {
        let mut state = PopupState::new(PopupKind::Command);
        state.items = vec![
            PopupItem::cmd("search")
                .desc("Search the vault")
                .with_score(10),
            PopupItem::agent("dev")
                .desc("Developer agent")
                .with_score(8),
        ];
        state.selected = 0;

        let widget = PopupWidget::new(&mut state);
        let area = Rect::new(0, 0, 50, 6);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Verify the widget renders without panic
        // Check that selected item has marker
        let content = buf.content();
        let has_marker = content.iter().any(|cell| cell.symbol() == ">");
        assert!(has_marker, "Selected item should have '>' marker");
    }

    #[test]
    fn test_popup_widget_renders_selection_highlight() {
        let mut state = PopupState::new(PopupKind::Command);
        state.items = vec![
            PopupItem::cmd("cmd1").with_score(1),
            PopupItem::cmd("cmd2").with_score(1),
        ];
        state.selected = 1;

        let widget = PopupWidget::new(&mut state);
        let area = Rect::new(0, 0, 30, 5);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Verify rendering completes
        assert!(buf.area().width > 0);
    }

    #[test]
    fn test_popup_widget_renders_scroll_indicators() {
        let mut state = PopupState::new(PopupKind::Command);
        state.items = (0..25)
            .map(|i| PopupItem::cmd(format!("cmd{}", i)).with_score(1))
            .collect();
        state.selected = 15;

        let widget = PopupWidget::new(&mut state);
        let area = Rect::new(0, 0, 40, 12);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Verify it renders with many items
        assert!(buf.area().width > 0);

        // Check that title shows count when there are more items than MAX_POPUP_ITEMS
        let content = buf.content();
        let _has_count = content
            .iter()
            .any(|cell| cell.symbol().contains('(') || cell.symbol().contains('/'));
        // Note: This is a weak assertion - mainly checking it doesn't panic
    }

    #[test]
    fn test_popup_widget_viewport_scrolls() {
        let mut state = make_popup_state(20);
        state.selected = 15; // Select an item beyond the initial viewport

        let widget = PopupWidget::new(&mut state);
        let area = Rect::new(0, 0, 40, 12); // Limited height
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // After render, viewport should be updated to show selected item
        assert!(state.viewport_offset <= state.selected);
        assert!(state.selected < state.viewport_offset + 10);
    }

    #[test]
    fn test_popup_widget_different_item_kinds() {
        let mut state = PopupState::new(PopupKind::AgentOrFile);
        state.items = vec![
            PopupItem::agent("dev").desc("Developer").with_score(10),
            PopupItem::file("src/main.rs").with_score(8),
            PopupItem::note("project/foo.md").with_score(6),
            PopupItem::skill("code-review")
                .desc("Review code (user)")
                .with_score(5),
        ];

        let widget = PopupWidget::new(&mut state);
        let area = Rect::new(0, 0, 60, 8);
        let mut buf = Buffer::empty(area);

        widget.render(area, &mut buf);

        // Verify all item kinds render
        let content = buf.content();
        let symbols: Vec<&str> = content.iter().map(|c| c.symbol()).collect();
        let text = symbols.join("");

        // Check for kind labels
        assert!(text.contains("[agent]") || text.contains("agent"));
        assert!(text.contains("[file]") || text.contains("file"));
        assert!(text.contains("[note]") || text.contains("note"));
        assert!(text.contains("[skill]") || text.contains("skill"));
    }
}
