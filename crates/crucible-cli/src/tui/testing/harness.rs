//! Test harness for TUI testing
//!
//! Provides a simulated TUI environment for testing component behavior
//! without a real terminal.

use crate::tui::components::generic_popup::PopupState;
use crate::tui::conversation::{ConversationItem, ConversationState};
use crate::tui::conversation_view::{ConversationView, RatatuiView};
use crate::tui::popup::PopupProvider;
use crate::tui::state::{PopupItem, PopupKind, TuiState};
use crate::tui::streaming_channel::StreamingEvent;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::sync::Arc;

/// A static provider that returns a fixed list of items
struct StaticItemsProvider {
    kind: PopupKind,
    items: Vec<PopupItem>,
}

impl PopupProvider for StaticItemsProvider {
    fn provide(&self, kind: PopupKind, _query: &str) -> Vec<PopupItem> {
        if kind == self.kind {
            self.items.clone()
        } else {
            Vec::new()
        }
    }
}

/// An empty provider for testing (returns no items)
struct EmptyProvider;

impl PopupProvider for EmptyProvider {
    fn provide(&self, _kind: PopupKind, _query: &str) -> Vec<PopupItem> {
        Vec::new()
    }
}

/// Test harness for TUI components
///
/// Provides a simulated environment for testing TUI behavior:
/// - Simulates key presses and input
/// - Injects streaming events
/// - Captures rendered output for snapshot testing
///
/// # Example
///
/// ```ignore
/// let mut h = Harness::new(80, 24);
/// h.keys("hello");
/// assert_eq!(h.input_text(), "hello");
///
/// h.key(KeyCode::Char('/'));
/// assert!(h.popup().is_some());
/// ```
pub struct Harness {
    /// Main TUI state (input, popup, etc.)
    pub state: TuiState,
    /// Conversation state
    pub conversation: ConversationState,
    /// Ratatui view (combines conversation + state for rendering)
    pub view: RatatuiView,
    /// Viewport dimensions
    pub width: u16,
    pub height: u16,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new(80, 24)
    }
}

impl Harness {
    /// Create a new harness with given dimensions
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            state: TuiState::new("plan"),
            conversation: ConversationState::new(),
            view: RatatuiView::new("plan", width, height),
            width,
            height,
        }
    }

    /// Builder: set initial conversation
    ///
    /// Populates both the harness's conversation state and the view's internal
    /// conversation state so rendering works correctly.
    pub fn with_session(mut self, items: Vec<ConversationItem>) -> Self {
        for item in items.clone() {
            self.conversation.push(item);
        }
        // Also sync to view's internal conversation for rendering
        for item in items {
            self.view.state_mut().conversation.push(item);
        }
        self
    }

    /// Builder: set popup items
    ///
    /// Uses PopupState with PopupRenderer for proper rendering
    pub fn with_popup_items(mut self, kind: PopupKind, items: Vec<PopupItem>) -> Self {
        // Create a static provider with the given items
        let provider = Arc::new(StaticItemsProvider {
            kind,
            items: items.clone(),
        });

        // Create PopupState and populate it
        let mut popup = PopupState::new(kind, provider);
        popup.update_query(""); // Load items from provider

        // Set on the view
        self.view.set_popup(Some(popup));
        self.state.has_popup = true;

        self
    }

    /// Builder: enable gradient popup style (neovim-inspired raised effect)
    pub fn with_gradient_popup(mut self, enabled: bool) -> Self {
        self.view.state_mut().use_gradient_popup = enabled;
        self
    }

    // =========================================================================
    // Input simulation
    // =========================================================================

    /// Simulate a key press
    pub fn key(&mut self, code: KeyCode) {
        self.key_with_modifiers(code, KeyModifiers::NONE);
    }

    /// Simulate a key press with modifiers
    pub fn key_with_modifiers(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let event = KeyEvent::new(code, modifiers);
        self.handle_key_event(event);
    }

    /// Simulate Ctrl+key
    pub fn key_ctrl(&mut self, c: char) {
        self.key_with_modifiers(KeyCode::Char(c), KeyModifiers::CONTROL);
    }

    /// Simulate Alt+key
    pub fn key_alt(&mut self, c: char) {
        self.key_with_modifiers(KeyCode::Char(c), KeyModifiers::ALT);
        // Handle Alt+T for reasoning toggle
        if c == 't' {
            self.state.show_reasoning = !self.state.show_reasoning;
        }
    }

    /// Type a string (simulates key-by-key input)
    pub fn keys(&mut self, input: &str) {
        for c in input.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    /// Handle a key event (internal)
    fn handle_key_event(&mut self, event: KeyEvent) {
        // Handle Ctrl modifiers first
        if event.modifiers.contains(KeyModifiers::CONTROL) {
            match event.code {
                KeyCode::Char('a') => {
                    self.state.cursor_position = 0;
                    return;
                }
                KeyCode::Char('e') => {
                    self.state.cursor_position = self.state.input_buffer.len();
                    return;
                }
                KeyCode::Char('u') => {
                    self.state.input_buffer.drain(..self.state.cursor_position);
                    self.state.cursor_position = 0;
                    return;
                }
                KeyCode::Char('k') => {
                    self.state.input_buffer.truncate(self.state.cursor_position);
                    return;
                }
                KeyCode::Char('w') => {
                    let new_pos = crate::tui::state::find_word_start_backward(
                        &self.state.input_buffer[..self.state.cursor_position],
                    );
                    self.state
                        .input_buffer
                        .drain(new_pos..self.state.cursor_position);
                    self.state.cursor_position = new_pos;
                    return;
                }
                _ => {}
            }
        }

        // Handle popup input first if popup is open
        if let Some(popup) = self.view.popup_mut() {
            match event.code {
                KeyCode::Esc => {
                    self.view.set_popup(None);
                    self.state.has_popup = false;
                    return;
                }
                KeyCode::Up => {
                    popup.move_selection(-1);
                    return;
                }
                KeyCode::Down => {
                    popup.move_selection(1);
                    return;
                }
                KeyCode::Enter => {
                    if let Some(item) = popup.selected_item() {
                        let token = item.token();
                        self.view.set_popup(None);
                        self.state.has_popup = false;
                        self.state.input_buffer = token;
                        self.state.cursor_position = self.state.input_buffer.len();
                    }
                    return;
                }
                KeyCode::Char(c) => {
                    // Update query (append char and re-query provider)
                    let current_query = popup.query().to_string();
                    let new_query = format!("{}{}", current_query, c);
                    popup.update_query(&new_query);
                    return;
                }
                KeyCode::Backspace => {
                    let current_query = popup.query().to_string();
                    if current_query.is_empty() {
                        self.view.set_popup(None);
                        self.state.has_popup = false;
                    } else {
                        let new_query = current_query[..current_query.len() - 1].to_string();
                        popup.update_query(&new_query);
                    }
                    return;
                }
                _ => {}
            }
        }

        // Normal input handling
        match event.code {
            KeyCode::Char('/') if self.state.input_buffer.is_empty() => {
                // Trigger command popup
                let popup = PopupState::new(
                    PopupKind::Command,
                    Arc::new(EmptyProvider) as Arc<dyn PopupProvider>,
                );
                self.view.set_popup(Some(popup));
                self.state.has_popup = true;
                self.state.input_buffer.push('/');
                self.state.cursor_position = 1;
            }
            KeyCode::Char('@') if self.state.input_buffer.is_empty() => {
                // Trigger agent/file popup
                let popup = PopupState::new(
                    PopupKind::AgentOrFile,
                    Arc::new(EmptyProvider) as Arc<dyn PopupProvider>,
                );
                self.view.set_popup(Some(popup));
                self.state.has_popup = true;
                self.state.input_buffer.push('@');
                self.state.cursor_position = 1;
            }
            KeyCode::Char(c) => {
                self.state
                    .input_buffer
                    .insert(self.state.cursor_position, c);
                self.state.cursor_position += c.len_utf8();
            }
            KeyCode::Backspace => {
                if self.state.cursor_position > 0 {
                    let prev_char_boundary = self.state.input_buffer[..self.state.cursor_position]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.state.input_buffer.remove(prev_char_boundary);
                    self.state.cursor_position = prev_char_boundary;
                }
            }
            KeyCode::Left => {
                if self.state.cursor_position > 0 {
                    self.state.cursor_position = self.state.input_buffer
                        [..self.state.cursor_position]
                        .char_indices()
                        .last()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                }
            }
            KeyCode::Right => {
                if self.state.cursor_position < self.state.input_buffer.len() {
                    self.state.cursor_position = self.state.input_buffer
                        [self.state.cursor_position..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.state.cursor_position + i)
                        .unwrap_or(self.state.input_buffer.len());
                }
            }
            _ => {}
        }
    }

    // =========================================================================
    // Event injection
    // =========================================================================

    /// Inject a streaming event
    pub fn event(&mut self, event: StreamingEvent) {
        match event {
            StreamingEvent::Delta { text, seq } => {
                self.state.last_seen_seq = seq;
                self.view.append_or_create_prose(&text);
            }
            StreamingEvent::Done { full_response: _ } => {
                self.view.complete_assistant_streaming();
                // Clear reasoning buffer when response completes
                self.state.clear_reasoning();
            }
            StreamingEvent::Error { message } => {
                self.state.status_error = Some(message);
            }
            StreamingEvent::ToolCall { name, args, .. } => {
                // Track in state
                self.state
                    .pending_tools
                    .push(crate::tui::state::ToolCallInfo {
                        name: name.clone(),
                        args: args.clone(),
                        call_id: None,
                        completed: false,
                        result: None,
                        error: None,
                    });
                // Also push to view for rendering
                self.view.push_tool_running(&name, args);
            }
            StreamingEvent::ToolCompleted {
                name,
                result,
                error,
            } => {
                // Mark matching tool as completed in state
                if let Some(tool) = self
                    .state
                    .pending_tools
                    .iter_mut()
                    .find(|t| t.name == name && !t.completed)
                {
                    tool.completed = true;
                    tool.result = Some(result.clone());
                    tool.error = error.clone();
                }
                // Update view with completion
                if let Some(err) = &error {
                    self.view.error_tool(&name, err);
                } else {
                    let summary = if result.len() > 50 {
                        Some(format!("{}...", &result[..47]))
                    } else if !result.is_empty() {
                        Some(result)
                    } else {
                        None
                    };
                    self.view.complete_tool(&name, summary);
                }
            }
            StreamingEvent::Reasoning { text, seq } => {
                // Accumulate reasoning in state for display
                self.state.append_reasoning(&text);
                self.state.last_seen_seq = seq;
            }
        }
    }

    /// Inject multiple events in sequence
    pub fn events(&mut self, events: Vec<StreamingEvent>) {
        for event in events {
            self.event(event);
        }
    }

    // =========================================================================
    // State accessors
    // =========================================================================

    /// Get current input text
    pub fn input_text(&self) -> &str {
        &self.state.input_buffer
    }

    /// Get cursor position
    pub fn cursor_position(&self) -> usize {
        self.state.cursor_position
    }

    /// Get current popup state
    pub fn popup(&self) -> Option<&PopupState> {
        self.view.popup()
    }

    /// Check if popup is open
    pub fn has_popup(&self) -> bool {
        self.view.popup().is_some()
    }

    /// Get popup query
    pub fn popup_query(&self) -> Option<&str> {
        self.view.popup().map(|p| p.query())
    }

    /// Get popup selected index
    pub fn popup_selected(&self) -> Option<usize> {
        self.view.popup().map(|p| p.selected_index())
    }

    /// Get conversation items
    pub fn conversation_items(&self) -> &[ConversationItem] {
        self.conversation.items()
    }

    /// Get number of conversation items
    pub fn conversation_len(&self) -> usize {
        self.conversation.items().len()
    }

    /// Check if there's an error
    pub fn has_error(&self) -> bool {
        self.state.status_error.is_some()
    }

    /// Get error message
    pub fn error(&self) -> Option<&str> {
        self.state.status_error.as_deref()
    }

    /// Check if reasoning display is enabled
    pub fn show_reasoning(&self) -> bool {
        self.state.show_reasoning
    }

    /// Get accumulated reasoning text
    pub fn reasoning(&self) -> &str {
        &self.state.accumulated_reasoning
    }

    // =========================================================================
    // Rendering
    // =========================================================================

    /// Render to a test terminal and return it for snapshot testing
    pub fn render_terminal(&self) -> Terminal<TestBackend> {
        let mut terminal =
            Terminal::new(TestBackend::new(self.width, self.height)).expect("create terminal");
        terminal
            .draw(|frame| {
                self.view.render_frame(frame);
            })
            .expect("draw frame");
        terminal
    }

    /// Render to a string for snapshot testing
    pub fn render(&self) -> String {
        let terminal = self.render_terminal();
        let backend = terminal.backend();
        let buffer = backend.buffer();
        buffer_to_string(buffer)
    }

    /// Render just the input area as a string
    pub fn render_input(&self) -> String {
        let cursor_marker = if self.state.cursor_position == self.state.input_buffer.len() {
            format!("{}|", self.state.input_buffer)
        } else {
            let (before, after) = self.state.input_buffer.split_at(self.state.cursor_position);
            format!("{}|{}", before, after)
        };
        cursor_marker
    }
}

/// Convert a ratatui Buffer to a string for snapshot testing
fn buffer_to_string(buffer: &ratatui::buffer::Buffer) -> String {
    let mut output = String::new();
    let area = buffer.area;

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buffer.cell((x, y)) {
                output.push_str(cell.symbol());
            }
        }
        output.push('\n');
    }

    // Trim trailing empty lines but keep structure
    while output.ends_with("\n\n") {
        output.pop();
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::testing::fixtures::sessions;

    #[test]
    fn harness_default_is_empty() {
        let h = Harness::default();
        assert_eq!(h.input_text(), "");
        assert!(!h.has_popup());
        assert_eq!(h.conversation_len(), 0);
    }

    #[test]
    fn harness_accepts_input() {
        let mut h = Harness::new(80, 24);
        h.keys("hello");
        assert_eq!(h.input_text(), "hello");
    }

    #[test]
    fn harness_cursor_movement() {
        let mut h = Harness::new(80, 24);
        h.keys("hello");
        assert_eq!(h.cursor_position(), 5);

        h.key(KeyCode::Left);
        assert_eq!(h.cursor_position(), 4);

        h.key_ctrl('a');
        assert_eq!(h.cursor_position(), 0);

        h.key_ctrl('e');
        assert_eq!(h.cursor_position(), 5);
    }

    #[test]
    fn harness_slash_opens_popup() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));
        assert!(h.has_popup());
        assert_eq!(h.input_text(), "/");
    }

    #[test]
    fn harness_escape_closes_popup() {
        let mut h = Harness::new(80, 24);
        h.key(KeyCode::Char('/'));
        assert!(h.has_popup());

        h.key(KeyCode::Esc);
        assert!(!h.has_popup());
    }

    #[test]
    fn harness_with_session() {
        let h = Harness::new(80, 24).with_session(sessions::basic_exchange());
        assert_eq!(h.conversation_len(), 2);
    }

    #[test]
    fn harness_ctrl_w_deletes_word() {
        let mut h = Harness::new(80, 24);
        h.keys("hello world");
        h.key_ctrl('w');
        assert_eq!(h.input_text(), "hello ");
    }

    #[test]
    fn harness_ctrl_u_clears_to_start() {
        let mut h = Harness::new(80, 24);
        h.keys("hello world");
        h.key_ctrl('u');
        assert_eq!(h.input_text(), "");
    }

    #[test]
    fn harness_render_returns_string() {
        let h = Harness::new(40, 10);
        let output = h.render();
        assert!(!output.is_empty());
    }

    // =========================================================================
    // Reasoning Toggle Tests (TDD - RED PHASE)
    // =========================================================================

    #[test]
    fn harness_reasoning_default_hidden() {
        let h = Harness::new(80, 24);
        assert!(!h.show_reasoning());
        assert!(h.reasoning().is_empty());
    }

    #[test]
    fn harness_alt_t_toggles_reasoning() {
        let mut h = Harness::new(80, 24);
        assert!(!h.show_reasoning());

        h.key_alt('t');
        assert!(h.show_reasoning());

        h.key_alt('t');
        assert!(!h.show_reasoning());
    }

    #[test]
    fn harness_reasoning_event_accumulates() {
        let mut h = Harness::new(80, 24);
        h.event(StreamingEvent::Reasoning {
            text: "Thinking about ".to_string(),
            seq: 0,
        });
        h.event(StreamingEvent::Reasoning {
            text: "the problem".to_string(),
            seq: 1,
        });
        assert_eq!(h.reasoning(), "Thinking about the problem");
    }

    #[test]
    fn harness_reasoning_clears_on_done() {
        let mut h = Harness::new(80, 24);
        h.event(StreamingEvent::Reasoning {
            text: "Some thinking".to_string(),
            seq: 0,
        });
        assert!(!h.reasoning().is_empty());

        h.event(StreamingEvent::Done {
            full_response: "Done".to_string(),
        });
        assert!(h.reasoning().is_empty());
    }
}
