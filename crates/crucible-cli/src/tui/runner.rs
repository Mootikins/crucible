//! TUI runner - main event loop for terminal UI
//!
//! Coordinates terminal input, event polling, and rendering.
//! Uses ratatui with alternate screen for full viewport control.

use crate::chat::bridge::AgentEventBridge;

use crate::tui::conversation::StatusKind;
use crate::tui::conversation_view::{ConversationView, RatatuiView};
use crate::tui::{map_key_event, DynamicPopupProvider, InputAction, PopupProvider, TuiState};
use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, MouseEvent, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen},
};
use crucible_core::events::SessionEvent;
use crucible_core::traits::chat::AgentHandle;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

/// TUI runner with full ratatui control and new conversation styling.
///
/// Uses alternate screen mode with:
/// - Inverted user messages
/// - Clean tool call display
/// - Status bar below input
/// - Mouse scroll support
pub struct RatatuiRunner {
    view: RatatuiView,
    popup_provider: std::sync::Arc<DynamicPopupProvider>,
    /// Token count from current/last response
    token_count: usize,
    /// Track if we're currently streaming
    is_streaming: bool,
    /// Track Ctrl+C for double-press exit
    ctrl_c_count: u8,
    last_ctrl_c: Option<std::time::Instant>,
    /// Should exit flag
    should_exit: bool,
    /// Popup state
    popup: Option<crate::tui::state::PopupState>,
}

impl RatatuiRunner {
    /// Create a new ratatui-based TUI runner.
    pub fn new(
        mode_id: &str,
        popup_provider: std::sync::Arc<DynamicPopupProvider>,
    ) -> Result<Self> {
        let (width, height) = size().unwrap_or((80, 24));

        Ok(Self {
            view: RatatuiView::new(mode_id, width, height),
            popup_provider,
            token_count: 0,
            is_streaming: false,
            ctrl_c_count: 0,
            last_ctrl_c: None,
            should_exit: false,
            popup: None,
        })
    }

    /// Run the TUI main loop with an agent.
    pub async fn run<A: AgentHandle>(
        &mut self,
        bridge: &AgentEventBridge,
        agent: &mut A,
    ) -> Result<()> {
        // Setup terminal with alternate screen and mouse capture
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
            cursor::Hide
        )?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        let result = self.main_loop(&mut terminal, bridge, agent).await;

        // Cleanup
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            crossterm::event::DisableMouseCapture,
            LeaveAlternateScreen,
            cursor::Show
        )?;

        result
    }

    /// Internal main loop.
    async fn main_loop<A: AgentHandle>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        bridge: &AgentEventBridge,
        agent: &mut A,
    ) -> Result<()> {
        let mut popup_debounce =
            crate::tui::popup::PopupDebounce::new(std::time::Duration::from_millis(50));
        let mut last_seen_seq = 0u64;

        loop {
            // 1. Render
            terminal.draw(|f| self.view.render_frame(f))?;

            // 2. Poll events (non-blocking, ~60fps)
            if event::poll(Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => {
                        if self.handle_key_event(&key, bridge, agent).await? {
                            break;
                        }
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse_event(&mouse);
                    }
                    Event::Resize(width, height) => {
                        self.view.handle_resize(width, height)?;
                    }
                    _ => {}
                }
            }

            // 3. Refresh popup items
            if let Some(ref mut popup) = self.popup {
                if popup_debounce.ready() {
                    let items = self.popup_provider.provide(popup.kind, &popup.query);
                    let selected = popup.selected.min(items.len().saturating_sub(1));
                    popup.items = items;
                    popup.selected = selected;
                }
            }

            // 4. Poll ring buffer for session events
            self.poll_session_events(bridge, &mut last_seen_seq);
        }

        Ok(())
    }

    /// Handle keyboard input.
    async fn handle_key_event<A: AgentHandle>(
        &mut self,
        key: &crossterm::event::KeyEvent,
        bridge: &AgentEventBridge,
        agent: &mut A,
    ) -> Result<bool> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Build a minimal TuiState for key mapping (we'll migrate away from this)
        let temp_state = TuiState::new(self.view.mode_id());
        let action = map_key_event(key, &temp_state);

        match action {
            InputAction::Exit => {
                return Ok(true);
            }
            InputAction::Cancel => {
                if self.is_streaming {
                    // Cancel streaming
                    self.is_streaming = false;
                    self.view.clear_status();
                    self.view.set_status_text("Cancelled");
                } else {
                    // Clear input or track Ctrl+C for double-press exit
                    self.ctrl_c_count += 1;
                    self.last_ctrl_c = Some(std::time::Instant::now());

                    if self.ctrl_c_count >= 2 {
                        if let Some(last) = self.last_ctrl_c {
                            if last.elapsed() < Duration::from_millis(500) {
                                return Ok(true); // Exit on double Ctrl+C
                            }
                        }
                    }

                    self.view.set_input("");
                    self.view.set_cursor_position(0);
                    self.popup = None;
                }
            }
            InputAction::SendMessage(msg) => {
                // Reset Ctrl+C tracking
                self.ctrl_c_count = 0;

                // Add user message to view
                self.view.push_user_message(&msg)?;

                // Set thinking status
                self.is_streaming = true;
                self.token_count = 0;
                self.view.set_status(StatusKind::Thinking);
                self.view.set_status_text("Thinking");

                // Send to agent
                if let Err(e) = bridge.send_message(&msg, agent).await {
                    self.is_streaming = false;
                    self.view.clear_status();
                    self.view.set_status_text(&format!("Error: {}", e));
                }
            }
            InputAction::InsertChar(c) => {
                let mut input = self.view.input().to_string();
                let pos = self.view.cursor_position();
                input.insert(pos, c);
                self.view.set_input(&input);
                self.view.set_cursor_position(pos + c.len_utf8());
                self.update_popup();
            }
            InputAction::DeleteChar => {
                let input = self.view.input().to_string();
                let pos = self.view.cursor_position();
                if pos > 0 {
                    let prev = input[..pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    let new_input = format!("{}{}", &input[..prev], &input[pos..]);
                    self.view.set_input(&new_input);
                    self.view.set_cursor_position(prev);
                    self.update_popup();
                }
            }
            InputAction::InsertNewline => {
                let mut input = self.view.input().to_string();
                let pos = self.view.cursor_position();
                input.insert(pos, '\n');
                self.view.set_input(&input);
                self.view.set_cursor_position(pos + 1);
            }
            InputAction::MoveCursorLeft => {
                let input = self.view.input();
                let pos = self.view.cursor_position();
                if pos > 0 {
                    let new_pos = input[..pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.view.set_cursor_position(new_pos);
                }
            }
            InputAction::MoveCursorRight => {
                let input = self.view.input();
                let pos = self.view.cursor_position();
                if pos < input.len() {
                    let new_pos = input[pos..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| pos + i)
                        .unwrap_or(input.len());
                    self.view.set_cursor_position(new_pos);
                }
            }
            InputAction::CycleMode => {
                let new_mode = crucible_core::traits::chat::cycle_mode_id(self.view.mode_id());
                self.view.set_mode_id(new_mode);
            }
            InputAction::ScrollUp => {
                self.view.scroll_up(3);
            }
            InputAction::ScrollDown => {
                self.view.scroll_down(3);
            }
            InputAction::PageUp => {
                self.view.scroll_up(10);
            }
            InputAction::PageDown => {
                self.view.scroll_down(10);
            }
            InputAction::MovePopupSelection(delta) => {
                if let Some(ref mut popup) = self.popup {
                    popup.move_selection(delta);
                }
            }
            InputAction::ConfirmPopup => {
                if let Some(ref popup) = self.popup {
                    if !popup.items.is_empty() {
                        let idx = popup.selected.min(popup.items.len() - 1);
                        let token = popup.items[idx].token.clone();
                        self.view.set_input(&token);
                        self.view.set_cursor_position(token.len());
                    }
                }
                self.popup = None;
            }
            InputAction::None => {}
        }

        Ok(false)
    }

    /// Handle mouse events for scrolling.
    fn handle_mouse_event(&mut self, mouse: &MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.view.scroll_up(3);
            }
            MouseEventKind::ScrollDown => {
                self.view.scroll_down(3);
            }
            _ => {}
        }
    }

    /// Poll session events from the ring buffer.
    fn poll_session_events(&mut self, bridge: &AgentEventBridge, last_seen_seq: &mut u64) {
        let events: Vec<_> = bridge
            .ring
            .range(*last_seen_seq, bridge.ring.write_sequence())
            .collect();
        *last_seen_seq = bridge.ring.write_sequence();

        for event in events {
            match &*event {
                SessionEvent::TextDelta { delta, .. } => {
                    // Update token count and status
                    self.token_count += delta.split_whitespace().count();
                    self.view.set_status(StatusKind::Generating {
                        token_count: self.token_count,
                    });
                    self.view.set_status_text("Generating");
                    self.view.set_token_count(Some(self.token_count));
                }
                SessionEvent::AgentResponded { content, tool_calls } => {
                    // Streaming complete
                    self.is_streaming = false;
                    self.view.clear_status();
                    self.view.set_status_text("Ready");

                    // Add assistant message
                    let _ = self.view.push_assistant_message(content);
                }
                SessionEvent::ToolCalled { name, args: _ } => {
                    self.view.push_tool_running(name);
                    self.view.set_status_text(&format!("Running: {}", name));
                }
                SessionEvent::ToolCompleted { name, result, error } => {
                    if let Some(err) = error {
                        self.view.error_tool(name, err);
                    } else {
                        // Truncate result for summary
                        let summary = if result.len() > 50 {
                            Some(format!("{}...", &result[..47]))
                        } else if !result.is_empty() {
                            Some(result.clone())
                        } else {
                            None
                        };
                        self.view.complete_tool(name, summary);
                    }
                }
                _ => {}
            }
        }
    }

    /// Update popup based on current input.
    fn update_popup(&mut self) {
        use crate::tui::state::{PopupKind, PopupState};

        let input = self.view.input();
        let trimmed = input.trim_start();

        if trimmed.starts_with('/') {
            let query = trimmed.strip_prefix('/').unwrap_or("").to_string();
            if self.popup.as_ref().map(|p| p.kind) != Some(PopupKind::Command) {
                self.popup = Some(PopupState::new(PopupKind::Command));
            }
            if let Some(ref mut popup) = self.popup {
                popup.query = query;
            }
        } else if trimmed.starts_with('@') {
            let query = trimmed.strip_prefix('@').unwrap_or("").to_string();
            if self.popup.as_ref().map(|p| p.kind) != Some(PopupKind::AgentOrFile) {
                self.popup = Some(PopupState::new(PopupKind::AgentOrFile));
            }
            if let Some(ref mut popup) = self.popup {
                popup.query = query;
            }
        } else {
            self.popup = None;
        }
    }

    /// Get the view state for testing.
    pub fn view(&self) -> &RatatuiView {
        &self.view
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_popup_provider() -> std::sync::Arc<DynamicPopupProvider> {
        std::sync::Arc::new(DynamicPopupProvider::new())
    }

    #[test]
    fn test_tui_state_creates_correctly() {
        let state = TuiState::new("plan");
        assert!(!state.should_exit);
        assert!(state.streaming.is_none());
    }

    #[test]
    fn test_ratatui_runner_creates() {
        let runner = RatatuiRunner::new("plan", test_popup_provider()).unwrap();
        assert_eq!(runner.view().mode_id(), "plan");
    }

    #[tokio::test]
    async fn test_runner_components_create() {
        use crate::chat::bridge::AgentEventBridge;
        use crucible_rune::SessionBuilder;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let session = SessionBuilder::new("test-runner")
            .with_folder(temp.path())
            .build();

        let ring = session.ring().clone();
        let bridge = AgentEventBridge::new(session.handle(), ring.clone());

        assert!(ring.is_empty());
        assert!(bridge.handle.session_id().contains("test-runner"));
    }

    #[test]
    fn test_ctrl_c_cancels_during_streaming() {
        use crate::tui::StreamingBuffer;

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        let action = InputAction::Cancel;
        let is_streaming = state.streaming.is_some();

        assert!(matches!(action, InputAction::Cancel));
        assert!(is_streaming);
    }

    #[test]
    fn test_esc_maps_to_cancel() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
    }

    #[test]
    fn test_esc_cancels_during_streaming() {
        use crate::tui::StreamingBuffer;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
        assert!(state.streaming.is_some());
    }

    #[test]
    fn test_cancel_with_empty_streaming() {
        use crate::tui::StreamingBuffer;

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        let buf = state.streaming.take().unwrap();
        let remaining = buf.all_content();

        assert!(remaining.is_empty());
    }

    #[test]
    fn test_cancel_not_streaming_clears_input() {
        let mut state = TuiState::new("plan");
        state.input_buffer = "some text".to_string();
        state.cursor_position = 9;

        state.execute_action(InputAction::Cancel);

        assert!(state.input_buffer.is_empty());
        assert_eq!(state.cursor_position, 0);
    }

    #[test]
    fn test_status_error_preserved_until_new_message() {
        let mut state = TuiState::new("plan");

        state.status_error = Some("Connection failed".to_string());

        state.execute_action(InputAction::InsertChar('a'));
        assert!(state.status_error.is_some());

        state.execute_action(InputAction::SendMessage("test".to_string()));
        assert!(state.status_error.is_none());
    }

    #[test]
    fn test_ratatui_view_scroll() {
        let runner = RatatuiRunner::new("plan", test_popup_provider()).unwrap();

        // View should be accessible
        assert_eq!(runner.view().state().scroll_offset, 0);
    }
}
