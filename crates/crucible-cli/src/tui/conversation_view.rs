//! Conversation view abstraction
//!
//! Provides a trait for rendering conversation history with full ratatui control.

use crate::tui::components::{
    GenericPopupState, InputBoxWidget, SessionHistoryWidget, StatusBarWidget,
};
use crate::tui::conversation::{render_item_to_lines, ConversationState, StatusKind};
use crate::tui::dialog::{DialogResult, DialogStack, DialogWidget};
use crate::tui::notification::NotificationState;
use crate::tui::state::{PopupItem, PopupState};
use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

// =============================================================================
// View Trait
// =============================================================================

/// Abstraction for conversation rendering
pub trait ConversationView {
    /// Push a user message to the view
    fn push_user_message(&mut self, content: &str) -> Result<()>;

    /// Push an assistant message to the view
    fn push_assistant_message(&mut self, content: &str) -> Result<()>;

    /// Set the current status (thinking, generating, etc.)
    fn set_status(&mut self, status: StatusKind);

    /// Clear the status indicator
    fn clear_status(&mut self);

    /// Push a tool call (running state) with arguments
    fn push_tool_running(&mut self, name: &str, args: serde_json::Value);

    /// Update tool output (streaming)
    fn update_tool_output(&mut self, name: &str, output: &str);

    /// Mark a tool as complete
    fn complete_tool(&mut self, name: &str, summary: Option<String>);

    /// Mark a tool as errored
    fn error_tool(&mut self, name: &str, message: &str);

    /// Render the view (implementation-specific)
    fn render(&mut self) -> Result<()>;

    /// Handle terminal resize
    fn handle_resize(&mut self, width: u16, height: u16) -> Result<()>;

    /// Get/set input state (for the input box)
    fn input(&self) -> &str;
    fn set_input(&mut self, input: &str);
    fn cursor_position(&self) -> usize;
    fn set_cursor_position(&mut self, pos: usize);

    /// Mode and status for status bar
    fn mode_id(&self) -> &str;
    fn set_mode_id(&mut self, mode: &str);
    fn token_count(&self) -> Option<usize>;
    fn set_token_count(&mut self, count: Option<usize>);
    fn status_text(&self) -> &str;
    fn set_status_text(&mut self, status: &str);

    /// Scroll control
    fn scroll_up(&mut self, lines: usize);
    fn scroll_down(&mut self, lines: usize);
    fn scroll_to_top(&mut self);
    fn scroll_to_bottom(&mut self);
}

// =============================================================================
// View State
// =============================================================================

/// State for the ratatui view
#[derive(Debug)]
pub struct ViewState {
    pub conversation: ConversationState,
    pub input_buffer: String,
    pub cursor_position: usize,
    pub mode_id: String,
    pub token_count: Option<usize>,
    pub status_text: String,
    pub scroll_offset: usize,
    /// True if user is at bottom (auto-scroll enabled)
    pub at_bottom: bool,
    pub width: u16,
    pub height: u16,
    /// Popup state for slash commands / agents / files (legacy)
    pub popup: Option<PopupState>,
    /// Generic popup state using new popup system
    pub generic_popup: Option<GenericPopupState>,
    /// Dialog stack for modal dialogs
    pub dialog_stack: DialogStack,
    /// Notification state for file watch events
    pub notifications: NotificationState,
}

impl ViewState {
    pub fn new(mode_id: &str, width: u16, height: u16) -> Self {
        Self {
            conversation: ConversationState::new(),
            input_buffer: String::new(),
            cursor_position: 0,
            mode_id: mode_id.to_string(),
            token_count: None,
            status_text: "Ready".to_string(),
            scroll_offset: 0,
            at_bottom: true,
            width,
            height,
            popup: None,
            generic_popup: None,
            dialog_stack: DialogStack::new(),
            notifications: NotificationState::new(),
        }
    }

    /// Returns the display offset for cursor positioning.
    ///
    /// When the input starts with `:` or `!`, the prefix is shown as the prompt
    /// and stripped from the content display. This returns 1 in that case so
    /// the cursor position can be adjusted.
    pub fn input_display_offset(&self) -> usize {
        let trimmed = self.input_buffer.trim_start();
        if trimmed.starts_with(':') || trimmed.starts_with('!') {
            1
        } else {
            0
        }
    }
}

// =============================================================================
// Ratatui Implementation
// =============================================================================

/// Full ratatui-controlled view
///
/// Uses alternate screen with complete viewport control.
/// Manages its own scrollback buffer.
pub struct RatatuiView {
    state: ViewState,
}

impl RatatuiView {
    pub fn new(mode_id: &str, width: u16, height: u16) -> Self {
        Self {
            state: ViewState::new(mode_id, width, height),
        }
    }

    /// Maximum popup items to display
    const MAX_POPUP_ITEMS: usize = 5;

    /// Render to a ratatui frame
    pub fn render_frame(&self, frame: &mut Frame) {
        // Calculate popup height if needed
        let popup_height = self
            .state
            .popup
            .as_ref()
            .filter(|p| !p.items.is_empty())
            .map(|p| (p.items.len().min(Self::MAX_POPUP_ITEMS) + 2) as u16)
            .unwrap_or(0);

        let constraints = if popup_height > 0 {
            vec![
                Constraint::Min(3),               // Conversation area
                Constraint::Length(1),            // Spacer above input (visual separation)
                Constraint::Length(popup_height), // Popup
                Constraint::Length(3),            // Input box
                Constraint::Length(1),            // Status bar
            ]
        } else {
            vec![
                Constraint::Min(3),    // Conversation area
                Constraint::Length(1), // Spacer above input (visual separation)
                Constraint::Length(3), // Input box
                Constraint::Length(1), // Status bar
            ]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(frame.area());

        let mut idx = 0;

        // Conversation (using SessionHistoryWidget)
        let conv_area = chunks[idx];
        let conv_widget = SessionHistoryWidget::new(&self.state.conversation)
            .scroll_offset(self.state.scroll_offset)
            .viewport_height(conv_area.height);
        frame.render_widget(conv_widget, conv_area);
        idx += 1;

        // Spacer (visual separation before input - just skip it, it remains empty)
        idx += 1;

        // Popup (if active)
        if popup_height > 0 {
            self.render_popup(frame, chunks[idx]);
            idx += 1;
        }

        // Input box
        let input_area = chunks[idx];
        let input_widget =
            InputBoxWidget::new(&self.state.input_buffer, self.state.cursor_position);
        frame.render_widget(input_widget, input_area);
        idx += 1;

        // Status bar with notification support
        let notification = self.state.notifications.current();
        let mut status_widget = StatusBarWidget::new(&self.state.mode_id, &self.state.status_text);
        if let Some(count) = self.state.token_count {
            status_widget = status_widget.token_count(count);
        }
        status_widget = status_widget.notification(notification);
        frame.render_widget(status_widget, chunks[idx]);

        // Render dialog on top if present (overlays everything)
        if let Some(dialog) = self.state.dialog_stack.current() {
            let widget = DialogWidget::new(dialog);
            frame.render_widget(widget, frame.area());
            // Hide cursor when dialog is active
        } else {
            // Position cursor in input box (accounting for border and prompt)
            // When prefix (: or !) is shown as prompt, adjust cursor position
            let offset = self.state.input_display_offset();
            let display_cursor = self.state.cursor_position.saturating_sub(offset);
            let cursor_x = input_area.x + 1 + 2 + display_cursor as u16;
            let cursor_y = input_area.y + 1;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }

    /// Render popup overlay
    fn render_popup(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        let Some(ref popup) = self.state.popup else {
            return;
        };

        let lines: Vec<Line> = popup
            .items
            .iter()
            .enumerate()
            .skip(popup.viewport_offset)
            .take(Self::MAX_POPUP_ITEMS)
            .map(|(idx, item)| {
                let mut spans = Vec::new();
                // Fixed-width marker: always 2 chars for consistent alignment
                let marker = if idx == popup.selected { "> " } else { "  " };
                spans.push(Span::styled(
                    marker,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));

                // Kind labels removed - trigger char already indicates type
                spans.push(Span::styled(
                    item.title(),
                    if idx == popup.selected {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ));
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

        let popup_widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Select"))
            .wrap(Wrap { trim: true });

        frame.render_widget(popup_widget, area);
    }

    /// Get inner state reference
    pub fn state(&self) -> &ViewState {
        &self.state
    }

    /// Get mutable inner state reference
    pub fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Set popup state
    pub fn set_popup(&mut self, popup: Option<PopupState>) {
        self.state.popup = popup;
    }

    /// Get popup state reference
    pub fn popup(&self) -> Option<&PopupState> {
        self.state.popup.as_ref()
    }

    /// Get mutable popup state reference
    pub fn popup_mut(&mut self) -> Option<&mut PopupState> {
        self.state.popup.as_mut()
    }

    /// Set generic popup state
    pub fn set_generic_popup(&mut self, popup: Option<GenericPopupState>) {
        self.state.generic_popup = popup;
    }

    /// Get generic popup state reference
    pub fn generic_popup(&self) -> Option<&GenericPopupState> {
        self.state.generic_popup.as_ref()
    }

    /// Get mutable generic popup state reference
    pub fn generic_popup_mut(&mut self) -> Option<&mut GenericPopupState> {
        self.state.generic_popup.as_mut()
    }

    /// Check if any popup (legacy or generic) is active
    pub fn has_popup(&self) -> bool {
        self.state.popup.is_some() || self.state.generic_popup.is_some()
    }

    /// Calculate total content height for scroll bounds
    fn content_height(&self) -> usize {
        // Content width minus prefix (" · " = 3 chars) and right margin (1 char)
        // Must match ConversationWidget::render
        let content_width = (self.state.width as usize).saturating_sub(4);
        self.state
            .conversation
            .items()
            .iter()
            .map(|item| render_item_to_lines(item, content_width).len())
            .sum()
    }

    /// Start streaming an assistant message (creates empty message with streaming indicator)
    pub fn start_assistant_streaming(&mut self) {
        self.state.conversation.start_assistant_streaming();
    }

    /// Append content blocks to the streaming assistant message
    pub fn append_streaming_blocks(&mut self, blocks: Vec<crate::tui::StreamBlock>) {
        self.state.conversation.append_streaming_blocks(blocks);
        // Only auto-scroll if user was at bottom (allows reading while streaming)
        if self.state.at_bottom {
            self.scroll_to_bottom();
        }
    }

    /// Mark the streaming assistant message as complete
    pub fn complete_assistant_streaming(&mut self) {
        self.state.conversation.complete_streaming();
    }

    /// Append content to the last block of the streaming message
    pub fn append_to_last_block(&mut self, content: &str) {
        self.state.conversation.append_to_last_block(content);
    }

    /// Mark the last block as complete
    pub fn complete_last_block(&mut self) {
        self.state.conversation.complete_last_block();
    }

    /// Append text to the last prose block, or create a new one if needed
    /// Used for streaming to consolidate continuous prose text
    pub fn append_or_create_prose(&mut self, text: &str) {
        self.state.conversation.append_or_create_prose(text);
        // Only auto-scroll if user was at bottom (allows reading while streaming)
        if self.state.at_bottom {
            self.scroll_to_bottom();
        }
    }

    /// Push a dialog onto the stack
    pub fn push_dialog(&mut self, dialog: crate::tui::dialog::DialogState) {
        self.state.dialog_stack.push(dialog);
    }

    /// Check if a dialog is currently active
    pub fn has_dialog(&self) -> bool {
        !self.state.dialog_stack.is_empty()
    }

    /// Handle key event for the current dialog
    pub fn handle_dialog_key(&mut self, key: crossterm::event::KeyEvent) -> Option<DialogResult> {
        self.state.dialog_stack.handle_key(key)
    }

    /// Example: Route events using LayerStack for composable event handling
    ///
    /// This demonstrates how LayerStack can be used to route events through
    /// the layer hierarchy (base → popup → modal) with proper focus management.
    ///
    /// Note: Currently unused, as the runner uses direct event handling.
    /// Kept as reference for the component architecture pattern.
    #[allow(dead_code)]
    fn route_event_via_layer_stack(
        &mut self,
        event: &crossterm::event::Event,
    ) -> crate::tui::components::WidgetEventResult {
        use crate::tui::components::{FocusTarget, LayerStack};

        // Determine current focus based on active layers
        let focus = if self.state.dialog_stack.current().is_some() {
            FocusTarget::Dialog
        } else if self.state.popup.is_some() {
            FocusTarget::Popup
        } else {
            FocusTarget::Input
        };

        // Create layer stack for event routing
        let mut stack = LayerStack::new(focus);

        // Add popup layer if active
        // Note: This is a demonstration pattern. Actual popup widgets would need
        // to implement InteractiveWidget trait.
        // if let Some(popup) = &mut self.state.popup {
        //     stack.set_popup(popup_widget);
        // }

        // Add modal layer if active
        // if let Some(dialog) = self.state.dialog_stack.current_mut() {
        //     stack.set_modal(dialog_widget);
        // }

        // Route the event through the stack
        stack.route_event(event)

        // In practice, the runner would then handle the WidgetEventResult:
        // - Consumed: event was handled, stop propagation
        // - Ignored: continue to base layer handlers
        // - Action(action): process the requested action
    }
}

impl ConversationView for RatatuiView {
    fn push_user_message(&mut self, content: &str) -> Result<()> {
        self.state.conversation.push_user_message(content);
        self.scroll_to_bottom();
        Ok(())
    }

    fn push_assistant_message(&mut self, content: &str) -> Result<()> {
        self.state.conversation.push_assistant_message(content);
        // Only auto-scroll if user was at bottom (allows reading while assistant responds)
        if self.state.at_bottom {
            self.scroll_to_bottom();
        }
        Ok(())
    }

    fn set_status(&mut self, status: StatusKind) {
        self.state.conversation.set_status(status);
    }

    fn clear_status(&mut self) {
        self.state.conversation.clear_status();
    }

    fn push_tool_running(&mut self, name: &str, args: serde_json::Value) {
        self.state.conversation.push_tool_running(name, args);
    }

    fn update_tool_output(&mut self, name: &str, output: &str) {
        self.state.conversation.update_tool_output(name, output);
    }

    fn complete_tool(&mut self, name: &str, summary: Option<String>) {
        self.state.conversation.complete_tool(name, summary);
    }

    fn error_tool(&mut self, name: &str, message: &str) {
        self.state.conversation.error_tool(name, message);
    }

    fn render(&mut self) -> Result<()> {
        // This is a no-op - actual rendering happens via render_frame()
        // which is called by the terminal.draw() in the runner
        Ok(())
    }

    fn handle_resize(&mut self, width: u16, height: u16) -> Result<()> {
        self.state.width = width;
        self.state.height = height;
        Ok(())
    }

    fn input(&self) -> &str {
        &self.state.input_buffer
    }

    fn set_input(&mut self, input: &str) {
        self.state.input_buffer = input.to_string();
    }

    fn cursor_position(&self) -> usize {
        self.state.cursor_position
    }

    fn set_cursor_position(&mut self, pos: usize) {
        self.state.cursor_position = pos;
    }

    fn mode_id(&self) -> &str {
        &self.state.mode_id
    }

    fn set_mode_id(&mut self, mode: &str) {
        self.state.mode_id = mode.to_string();
    }

    fn token_count(&self) -> Option<usize> {
        self.state.token_count
    }

    fn set_token_count(&mut self, count: Option<usize>) {
        self.state.token_count = count;
    }

    fn status_text(&self) -> &str {
        &self.state.status_text
    }

    fn set_status_text(&mut self, status: &str) {
        self.state.status_text = status.to_string();
    }

    fn scroll_up(&mut self, lines: usize) {
        self.state.scroll_offset = self.state.scroll_offset.saturating_add(lines);
        // Clamp to content bounds
        let max_scroll = self
            .content_height()
            .saturating_sub(self.state.height as usize);
        self.state.scroll_offset = self.state.scroll_offset.min(max_scroll);
        self.state.at_bottom = false; // User scrolled up
    }

    fn scroll_down(&mut self, lines: usize) {
        self.state.scroll_offset = self.state.scroll_offset.saturating_sub(lines);
        if self.state.scroll_offset == 0 {
            self.state.at_bottom = true; // Back at bottom
        }
    }

    fn scroll_to_top(&mut self) {
        let max_scroll = self
            .content_height()
            .saturating_sub(self.state.height as usize);
        self.state.scroll_offset = max_scroll;
        self.state.at_bottom = false;
    }

    fn scroll_to_bottom(&mut self) {
        self.state.scroll_offset = 0;
        self.state.at_bottom = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{PopupItemKind, PopupKind};
    use ratatui::{backend::TestBackend, Terminal};
    use std::time::Instant;

    #[test]
    fn test_view_state_new() {
        let state = ViewState::new("plan", 80, 24);
        assert_eq!(state.mode_id, "plan");
        assert_eq!(state.width, 80);
        assert_eq!(state.height, 24);
        assert!(state.input_buffer.is_empty());
    }

    #[test]
    fn test_ratatui_view_push_messages() {
        let mut view = RatatuiView::new("plan", 80, 24);

        view.push_user_message("Hello").unwrap();
        view.push_assistant_message("Hi there!").unwrap();

        assert_eq!(view.state().conversation.items().len(), 2);
    }

    #[test]
    fn test_ratatui_view_scroll() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add enough content to exceed viewport (24 lines)
        // Each user message is 2 lines (blank + content), so need >12 messages
        for i in 0..15 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }

        // Should be at bottom
        assert_eq!(view.state().scroll_offset, 0);

        // Scroll up - should work since 15 * 2 = 30 lines > 24 viewport
        view.scroll_up(5);
        assert!(
            view.state().scroll_offset > 0,
            "scroll_offset should be > 0 after scrolling up with 30 lines of content"
        );

        // Scroll back down
        view.scroll_to_bottom();
        assert_eq!(view.state().scroll_offset, 0);
    }

    /// Test that popup is rendered in RatatuiView::render_frame
    /// This test would have FAILED before the fix because render_frame
    /// didn't render the popup at all.
    #[test]
    fn test_ratatui_view_renders_popup() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Set up popup with a command
        let popup = PopupState {
            kind: PopupKind::Command,
            query: String::new(),
            items: vec![PopupItem::cmd("help").desc("Show help")],
            selected: 0,
            viewport_offset: 0,
            last_update: Instant::now(),
        };
        view.set_popup(Some(popup));

        // Render to test backend
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render_frame(f)).unwrap();

        // Get the buffer content as string
        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // The popup should contain "/help" (kind labels removed for cleaner UI)
        assert!(
            content.contains("/help"),
            "Popup should render '/help' command. Buffer content: {}",
            content
        );
        // Kind labels are no longer shown - trigger char indicates type
        assert!(
            !content.contains("[cmd]"),
            "Popup should NOT render '[cmd]' label (removed). Buffer content: {}",
            content
        );
    }

    /// Test that skill items render (kind labels removed)
    #[test]
    fn test_ratatui_view_renders_skill_popup() {
        let mut view = RatatuiView::new("plan", 80, 24);

        let popup = PopupState {
            kind: PopupKind::Command,
            query: String::new(),
            items: vec![PopupItem::skill("git-commit").desc("Create commits (personal)")],
            selected: 0,
            viewport_offset: 0,
            last_update: Instant::now(),
        };
        view.set_popup(Some(popup));

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render_frame(f)).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // Kind labels are no longer shown - trigger char indicates type
        assert!(
            !content.contains("[skill]"),
            "Popup should NOT render '[skill]' label (removed). Buffer: {}",
            content
        );
        assert!(
            content.contains("skill:git-commit"),
            "Popup should render skill title. Buffer: {}",
            content
        );
    }

    // =============================================================================
    // Bottom-Anchored Rendering Tests
    // =============================================================================

    #[test]
    fn test_at_bottom_tracking() {
        let mut view = RatatuiView::new("plan", 80, 24);
        assert!(view.state().at_bottom);

        // Add content
        for i in 0..20 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }
        assert!(view.state().at_bottom);

        // Scroll up - no longer at bottom
        view.scroll_up(5);
        assert!(!view.state().at_bottom);

        // Scroll back to bottom
        view.scroll_to_bottom();
        assert!(view.state().at_bottom);
    }

    #[test]
    fn test_at_bottom_scroll_down_to_zero() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add content and scroll up
        for i in 0..20 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }
        view.scroll_up(10);
        assert!(!view.state().at_bottom);
        assert_eq!(view.state().scroll_offset, 10);

        // Scroll down to exactly 0
        view.scroll_down(10);
        assert_eq!(view.state().scroll_offset, 0);
        assert!(
            view.state().at_bottom,
            "Should be at_bottom when scroll_offset reaches 0"
        );
    }

    #[test]
    fn test_at_bottom_scroll_down_partial() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add content and scroll up
        for i in 0..20 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }
        view.scroll_up(10);
        assert!(!view.state().at_bottom);

        // Scroll down partially (not all the way to bottom)
        view.scroll_down(5);
        assert_eq!(view.state().scroll_offset, 5);
        assert!(
            !view.state().at_bottom,
            "Should NOT be at_bottom when scroll_offset > 0"
        );
    }

    /// Test that new content doesn't auto-scroll when user has scrolled up.
    ///
    /// Regression test for: auto-scroll ignoring at_bottom flag.
    /// Users should be able to read old messages while new content streams.
    #[test]
    fn test_no_auto_scroll_when_scrolled_up() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add initial content
        for i in 0..20 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }

        // User scrolls up to read older messages
        view.scroll_up(10);
        assert!(!view.state().at_bottom);
        let offset_before = view.state().scroll_offset;
        assert!(offset_before > 0);

        // New assistant message arrives (simulating streaming)
        view.push_assistant_message("New content from assistant")
            .unwrap();

        // Scroll position should NOT change - user should stay where they were
        assert_eq!(
            view.state().scroll_offset,
            offset_before,
            "Scroll offset should remain unchanged when user scrolled up. \
             Got {} but expected {} (the position before new content)",
            view.state().scroll_offset,
            offset_before
        );
        assert!(
            !view.state().at_bottom,
            "at_bottom should remain false when user has scrolled up"
        );
    }

    /// Test that new content DOES auto-scroll when user is at bottom.
    #[test]
    fn test_auto_scroll_when_at_bottom() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add initial content - user starts at bottom
        view.push_user_message("First message").unwrap();
        assert!(view.state().at_bottom);
        assert_eq!(view.state().scroll_offset, 0);

        // New assistant message arrives
        view.push_assistant_message("Response").unwrap();

        // Should remain at bottom
        assert!(view.state().at_bottom);
        assert_eq!(view.state().scroll_offset, 0);
    }

    /// Test that scrolling works correctly for messages with many lines.
    ///
    /// BUG: content_height() uses items.len() * 3, which severely underestimates
    /// actual content height for messages with code blocks or multiple paragraphs.
    /// This causes scroll_up to clamp scroll_offset too early, preventing users
    /// from scrolling back to see older content.
    #[test]
    fn test_scroll_with_multiline_messages() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Add 5 messages, each with 10+ lines (code block)
        // Total content should be ~50+ lines, not 15 (5 * 3)
        for i in 0..5 {
            let multiline_content = format!(
                "Message {} with code:\n```\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\n```",
                i
            );
            view.push_assistant_message(&multiline_content).unwrap();
        }

        // With viewport height 24 and actual content ~50+ lines,
        // max_scroll should be ~26+ lines
        // But buggy content_height() = 5 * 3 = 15
        // Buggy max_scroll = 15 - 24 = 0 (saturating_sub)

        // Try to scroll up significantly
        view.scroll_up(100); // Request more than possible

        // scroll_offset should be clamped to actual max_scroll (26+),
        // not the buggy estimate (0)
        // If content_height is calculated correctly, we should be able to scroll
        assert!(
            view.state().scroll_offset > 0,
            "Should be able to scroll up when content exceeds viewport. \
             scroll_offset={}, but expected > 0 (content should be ~50+ lines, viewport 24)",
            view.state().scroll_offset
        );

        // More specifically: with 5 messages each having 10+ lines = 50+ content lines,
        // and viewport of 24, max_scroll should be at least 26.
        // The buggy implementation gives max_scroll = max(0, 15-24) = 0
        assert!(
            view.state().scroll_offset >= 20,
            "scroll_offset should be at least 20 for 50+ lines of content with viewport 24. \
             Got scroll_offset={}, which suggests content_height is being underestimated.",
            view.state().scroll_offset
        );
    }

    // =============================================================================
    // Viewport Rendering Tests
    // =============================================================================

    /// Test that render_popup() uses viewport_offset to skip items
    /// This test should FAIL because render_popup() currently does:
    ///   popup.items.iter().take(MAX_POPUP_ITEMS)
    /// instead of:
    ///   popup.items.iter().skip(viewport_offset).take(MAX_POPUP_ITEMS)
    #[test]
    fn test_render_popup_uses_viewport_offset() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Create popup with 10 items, viewport_offset=3, selected=5
        let items = (0..10)
            .map(|i| PopupItem::cmd(format!("Item {}", i)).desc(format!("Subtitle {}", i)))
            .collect();

        let popup = PopupState {
            kind: PopupKind::Command,
            query: String::new(),
            items,
            selected: 5,
            viewport_offset: 3,
            last_update: Instant::now(),
        };
        view.set_popup(Some(popup));

        // Render to test backend
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render_frame(f)).unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // With viewport_offset=3, we should see items 3,4,5,6,7
        // NOT items 0,1,2,3,4
        assert!(
            content.contains("Item 3"),
            "Viewport should show Item 3 (first visible item). Buffer: {}",
            content
        );
        assert!(
            content.contains("Item 4"),
            "Viewport should show Item 4. Buffer: {}",
            content
        );
        assert!(
            content.contains("Item 5"),
            "Viewport should show Item 5 (selected). Buffer: {}",
            content
        );
        assert!(
            content.contains("Item 6"),
            "Viewport should show Item 6. Buffer: {}",
            content
        );
        assert!(
            content.contains("Item 7"),
            "Viewport should show Item 7 (last visible item). Buffer: {}",
            content
        );

        // Should NOT show items before viewport
        assert!(
            !content.contains("Item 0"),
            "Viewport should NOT show Item 0 (before viewport_offset). Buffer: {}",
            content
        );
        assert!(
            !content.contains("Item 1"),
            "Viewport should NOT show Item 1 (before viewport_offset). Buffer: {}",
            content
        );
        assert!(
            !content.contains("Item 2"),
            "Viewport should NOT show Item 2 (before viewport_offset). Buffer: {}",
            content
        );
    }

    /// Test that render_popup() highlights the selected item at correct visual position
    /// This test should FAIL because the current code uses enumerated index for highlighting,
    /// not the actual item index. With viewport_offset=3, selected=5, the selected item should
    /// be at visual position 2 (since we show items 3,4,5,6,7 and item 5 is at index 2).
    #[test]
    fn test_render_popup_selected_highlight_correct() {
        let mut view = RatatuiView::new("plan", 80, 24);

        // Create popup with 10 items, viewport_offset=3, selected=5
        let items = (0..10)
            .map(|i| PopupItem::cmd(format!("Item {}", i)).desc(format!("Subtitle {}", i)))
            .collect();

        let popup = PopupState {
            kind: PopupKind::Command,
            query: String::new(),
            items,
            selected: 5,
            viewport_offset: 3,
            last_update: Instant::now(),
        };
        view.set_popup(Some(popup));

        // Render to test backend
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| view.render_frame(f)).unwrap();

        let buffer = terminal.backend().buffer();

        // Find the line containing "Item 5" and check if it has the highlight marker ">"
        // The highlight marker should be at the start of the line
        let mut found_item_5 = false;
        let mut has_highlight_marker = false;

        for y in 0..buffer.area().height {
            let line: String = (0..buffer.area().width)
                .map(|x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect();

            if line.contains("Item 5") {
                found_item_5 = true;
                // Check if this line has the ">" marker at the start (after border)
                // The format is: "│> [cmd] Item 5 Subtitle 5..."
                has_highlight_marker = line.contains("│> ");
                break;
            }
        }

        assert!(found_item_5, "Should render Item 5 in viewport");
        assert!(
            has_highlight_marker,
            "Item 5 should have highlight marker '>' since it's selected"
        );

        // Additionally, verify that other visible items do NOT have the marker
        for y in 0..buffer.area().height {
            let line: String = (0..buffer.area().width)
                .map(|x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect();

            // Items 3, 4, 6, 7 should NOT have the highlight marker
            if line.contains("Item 3")
                || line.contains("Item 4")
                || line.contains("Item 6")
                || line.contains("Item 7")
            {
                let has_marker = line.contains("│> ");
                assert!(
                    !has_marker,
                    "Non-selected items should NOT have highlight marker '>'. Line: {}",
                    line
                );
            }
        }
    }

    // =============================================================================
    // Generic Popup Integration Tests
    // =============================================================================

    mod generic_popup_tests {
        use super::*;
        use crate::tui::components::{GenericPopupState, PopupItemProvider};
        use crate::tui::state::PopupKind;
        use std::sync::Arc;

        /// Mock provider for tests
        struct MockProvider;

        impl PopupItemProvider for MockProvider {
            fn provide(&self, _kind: PopupKind, _query: &str) -> Vec<PopupItem> {
                vec![
                    PopupItem::cmd("help").desc("Show help").with_score(100),
                    PopupItem::cmd("clear").desc("Clear history").with_score(90),
                ]
            }
        }

        fn mock_provider() -> Arc<dyn PopupItemProvider> {
            Arc::new(MockProvider)
        }

        #[test]
        fn test_view_state_has_generic_popup_field() {
            let state = ViewState::new("plan", 80, 24);
            // ViewState should have a generic_popup field that's None by default
            assert!(state.generic_popup.is_none());
        }

        #[test]
        fn test_ratatui_view_set_generic_popup() {
            let mut view = RatatuiView::new("plan", 80, 24);

            // Create a generic popup
            let popup = GenericPopupState::new(PopupKind::Command, mock_provider());

            // Set it on the view
            view.set_generic_popup(Some(popup));

            // Should be set
            assert!(view.generic_popup().is_some());
        }

        #[test]
        fn test_ratatui_view_generic_popup_mutable_access() {
            let mut view = RatatuiView::new("plan", 80, 24);

            let popup = GenericPopupState::new(PopupKind::Command, mock_provider());
            view.set_generic_popup(Some(popup));

            // Should be able to get mutable access
            let popup_mut = view.generic_popup_mut().unwrap();
            popup_mut.set_filter_query("hel");

            // Filter query should be updated
            assert_eq!(view.generic_popup().unwrap().filter_query(), "hel");
        }

        #[test]
        fn test_ratatui_view_has_popup_checks_both() {
            let mut view = RatatuiView::new("plan", 80, 24);

            // Neither popup set
            assert!(!view.has_popup());

            // Set legacy popup
            let legacy_popup = PopupState::new(PopupKind::Command);
            view.set_popup(Some(legacy_popup));
            assert!(view.has_popup());

            // Clear legacy, set generic
            view.set_popup(None);
            let generic_popup = GenericPopupState::new(PopupKind::Command, mock_provider());
            view.set_generic_popup(Some(generic_popup));
            assert!(view.has_popup());

            // Clear both
            view.set_generic_popup(None);
            assert!(!view.has_popup());
        }

        #[test]
        fn test_ratatui_view_clear_generic_popup() {
            let mut view = RatatuiView::new("plan", 80, 24);

            let popup = GenericPopupState::new(PopupKind::Command, mock_provider());
            view.set_generic_popup(Some(popup));
            assert!(view.generic_popup().is_some());

            // Clear it
            view.set_generic_popup(None);
            assert!(view.generic_popup().is_none());
        }
    }
}
