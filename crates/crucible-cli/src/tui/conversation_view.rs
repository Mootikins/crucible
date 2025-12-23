//! Conversation view abstraction
//!
//! Provides a trait for rendering conversation history with full ratatui control.

use crate::tui::conversation::{
    ConversationState, ConversationWidget, InputBoxWidget, StatusBarWidget, StatusKind,
};
use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout},
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

    /// Push a tool call (running state)
    fn push_tool_running(&mut self, name: &str);

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
    pub width: u16,
    pub height: u16,
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
            width,
            height,
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

    /// Render to a ratatui frame
    pub fn render_frame(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),    // Conversation area
                Constraint::Length(3), // Input box
                Constraint::Length(1), // Status bar
            ])
            .split(frame.area());

        // Conversation
        let conv_widget =
            ConversationWidget::new(&self.state.conversation).scroll_offset(self.state.scroll_offset);
        frame.render_widget(conv_widget, chunks[0]);

        // Input box
        let input_widget =
            InputBoxWidget::new(&self.state.input_buffer, self.state.cursor_position);
        frame.render_widget(input_widget, chunks[1]);

        // Status bar
        let mut status_widget = StatusBarWidget::new(&self.state.mode_id, &self.state.status_text);
        if let Some(count) = self.state.token_count {
            status_widget = status_widget.token_count(count);
        }
        frame.render_widget(status_widget, chunks[2]);
    }

    /// Get inner state reference
    pub fn state(&self) -> &ViewState {
        &self.state
    }

    /// Get mutable inner state reference
    pub fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Calculate total content height for scroll bounds
    fn content_height(&self) -> usize {
        // Rough estimate: count items * average lines per item
        self.state.conversation.items().len() * 3
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
        self.scroll_to_bottom();
        Ok(())
    }

    fn set_status(&mut self, status: StatusKind) {
        self.state.conversation.set_status(status);
    }

    fn clear_status(&mut self) {
        self.state.conversation.clear_status();
    }

    fn push_tool_running(&mut self, name: &str) {
        self.state.conversation.push_tool_running(name);
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
    }

    fn scroll_down(&mut self, lines: usize) {
        self.state.scroll_offset = self.state.scroll_offset.saturating_sub(lines);
    }

    fn scroll_to_bottom(&mut self) {
        self.state.scroll_offset = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Add some content
        for i in 0..10 {
            view.push_user_message(&format!("Message {}", i)).unwrap();
        }

        // Should be at bottom
        assert_eq!(view.state().scroll_offset, 0);

        // Scroll up
        view.scroll_up(5);
        assert!(view.state().scroll_offset > 0);

        // Scroll back down
        view.scroll_to_bottom();
        assert_eq!(view.state().scroll_offset, 0);
    }
}
