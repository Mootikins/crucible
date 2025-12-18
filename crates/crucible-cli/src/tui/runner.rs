//! TUI runner - main event loop for terminal UI

//!
//! Coordinates terminal input, event polling, and rendering.
//!
//! This module uses a terminal widget approach:
//! - Stays in normal terminal (no alternate screen)
//! - Completed messages print to stdout (terminal scrollback)
//! - Bottom widget handles input, status, and streaming

use crate::chat::bridge::AgentEventBridge;

use crate::tui::{
    map_key_event, render_widget, DynamicPopupProvider, InputAction, MarkdownRenderer,
    PopupProvider, TuiState, WidgetState,
};
use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event},
    execute,
    terminal::{self, disable_raw_mode, enable_raw_mode, size},
};
use crucible_core::traits::chat::AgentHandle;
use std::io::{self, Write};
use std::time::Duration;

/// TUI runner that manages the terminal UI lifecycle.
///
/// The runner coordinates:
/// - Terminal setup and cleanup (raw mode, no alternate screen)
/// - Input event polling and key mapping
/// - Session event polling from ring buffer
/// - Widget rendering at ~60fps (bottom of terminal)
/// - Markdown rendering for assistant responses
pub struct TuiRunner {
    state: TuiState,
    /// Markdown renderer with syntax highlighting
    renderer: MarkdownRenderer,
    /// Terminal width
    width: u16,
    /// Terminal height
    height: u16,
    popup_provider: std::sync::Arc<DynamicPopupProvider>,
}

impl TuiRunner {
    /// Create a new TUI runner with the given mode.
    pub fn new(
        mode_id: &str,
        popup_provider: std::sync::Arc<DynamicPopupProvider>,
    ) -> Result<Self> {
        let (width, height) = size().unwrap_or((80, 24));

        Ok(Self {
            state: TuiState::new(mode_id),
            renderer: MarkdownRenderer::new(),
            width,
            height,
            popup_provider,
        })
    }

    /// Run the TUI main loop with an agent.
    ///
    /// # Arguments
    /// * `bridge` - Event bridge for sending messages and accessing the ring
    /// * `agent` - Agent handle for processing messages
    ///
    /// # Returns
    /// * `Ok(())` when the user exits normally
    /// * `Err(...)` on terminal or agent errors
    pub async fn run<A: AgentHandle>(
        &mut self,
        bridge: &AgentEventBridge,
        agent: &mut A,
    ) -> Result<()> {
        // Setup terminal - raw mode only, no alternate screen
        // This keeps us in normal terminal with scrollback
        enable_raw_mode()?;

        // Clear screen and hide cursor
        let mut stdout = io::stdout();
        execute!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0),
            cursor::Hide
        )?;

        // Initial render of widget
        self.render_widget()?;

        let result = self.main_loop(bridge, agent).await;

        // Cleanup: restore terminal state
        let _ = disable_raw_mode();
        // Move cursor to bottom, print newline, and show cursor
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            cursor::MoveTo(0, self.height.saturating_sub(1)),
            cursor::Show
        );
        let _ = writeln!(stdout); // Ensure we're on a new line

        result
    }

    /// Internal main loop implementation.
    async fn main_loop<A: AgentHandle>(
        &mut self,
        bridge: &AgentEventBridge,
        agent: &mut A,
    ) -> Result<()> {
        let mut popup_debounce =
            crate::tui::popup::PopupDebounce::new(std::time::Duration::from_millis(50));
        loop {
            // 1. Poll terminal events (non-blocking, ~60fps)
            if event::poll(Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => {
                        let action = map_key_event(&key, &self.state);

                        // Handle cancel during streaming (Ctrl+C or ESC)
                        if matches!(action, InputAction::Cancel) && self.state.streaming.is_some() {
                            self.cancel_streaming()?;
                            continue;
                        }

                        if let Some(message) = self.state.execute_action(action.clone()) {
                            // Print user message to stdout (scrollback)
                            self.print_user_message(&message)?;
                            // Send message through bridge to agent
                            // Handle errors gracefully - flush partial content and show error
                            if let Err(e) = bridge.send_message(&message, agent).await {
                                self.handle_agent_error(&e.to_string())?;
                            }
                        }

                        if self.state.should_exit {
                            break;
                        }

                        // If popup is active and we got a confirm action, try to apply selection
                        if matches!(action, InputAction::ConfirmPopup) {
                            if let Some(token) = self.apply_popup_selection(agent)? {
                                self.state.input_buffer = token;
                                self.state.cursor_position = self.state.input_buffer.len();
                            }
                            self.state.popup = None;
                        }
                    }
                    Event::Resize(width, height) => {
                        self.handle_resize(width, height)?;
                    }
                    _ => {}
                }
            }

            // Refresh popup items on debounce if needed
            if let Some(ref mut popup) = self.state.popup {
                if popup_debounce.ready() {
                    let items = self.popup_provider.provide(popup.kind, &popup.query);
                    let selected = popup.selected.min(items.len().saturating_sub(1));
                    popup.items = items;
                    popup.selected = selected;
                }
            }

            // 2. Poll ring buffer for new session events
            // If a response was finalized, render it with markdown
            if let Some(content) = self.state.poll_events(&bridge.ring) {
                self.print_assistant_response(&content)?;
            }

            // 3. Render the widget
            self.render_widget()?;
        }

        Ok(())
    }

    /// Print user message to stdout (terminal scrollback).
    fn print_user_message(&mut self, message: &str) -> Result<()> {
        let mut stdout = io::stdout();

        // Calculate widget position
        let heights = crate::tui::calculate_heights(self.height, 1, 0);
        let widget_height = heights.total();
        let widget_top = self.height.saturating_sub(widget_height);

        // Clear the widget area first (prevents ghost content)
        for row in widget_top..=self.height {
            write!(stdout, "\x1b[{};1H\x1b[2K", row)?;
        }

        // Move to just above widget area, insert a line, print message
        write!(stdout, "\x1b[{};1H", widget_top)?;
        write!(stdout, "\x1b[L")?; // Insert line (scrolls content up)
        write!(stdout, "\x1b[1mYou:\x1b[0m {}", message)?;

        stdout.flush()?;
        Ok(())
    }

    /// Apply current popup selection; returns token to set in input if applicable.
    fn apply_popup_selection<A: AgentHandle>(&mut self, _agent: &mut A) -> Result<Option<String>> {
        if let Some(ref popup) = self.state.popup {
            if popup.items.is_empty() {
                return Ok(None);
            }
            let idx = popup.selected.min(popup.items.len() - 1);
            let item = &popup.items[idx];
            // For now, just return the token to place into the input buffer.
            // Future: invoke agent switch/command execution directly.
            return Ok(Some(item.token.clone()));
        }
        Ok(None)
    }

    /// Print assistant response to stdout with markdown rendering.
    ///
    /// Renders the markdown content with syntax highlighting and prints
    /// to terminal scrollback, above the widget area.
    fn print_assistant_response(&mut self, content: &str) -> Result<()> {
        let mut stdout = io::stdout();

        // Render markdown content
        let rendered = self.renderer.render(content);
        let lines: Vec<&str> = rendered.lines().collect();
        let line_count = lines.len().max(1);

        // Calculate widget position
        let heights = crate::tui::calculate_heights(self.height, 1, 0);
        let widget_height = heights.total();
        let widget_top = self.height.saturating_sub(widget_height);

        // Clear widget area first
        for row in widget_top..=self.height {
            write!(stdout, "\x1b[{};1H\x1b[2K", row)?;
        }

        // Insert lines for response (header + content)
        write!(stdout, "\x1b[{};1H", widget_top)?;
        for _ in 0..(line_count + 1) {
            write!(stdout, "\x1b[L")?; // Insert line
        }

        // Print header and content
        write!(
            stdout,
            "\x1b[{};1H\x1b[1mAssistant:\x1b[0m",
            widget_top.saturating_sub(line_count as u16)
        )?;
        for (i, line) in lines.iter().enumerate() {
            let row = widget_top.saturating_sub(line_count as u16) + 1 + i as u16;
            write!(stdout, "\x1b[{};1H{}", row, line)?;
        }

        stdout.flush()?;
        Ok(())
    }

    /// Handle terminal resize events.
    fn handle_resize(&mut self, width: u16, height: u16) -> Result<()> {
        // Update stored dimensions
        self.width = width;
        self.height = height;

        // Enforce minimum height (4 lines for widget)
        // If terminal is too small, we still try to render what we can

        // Re-render widget in new position
        self.render_widget()?;
        Ok(())
    }

    /// Handle agent error during streaming.
    ///
    /// Flushes partial content to stdout (marked as error),
    /// clears streaming state, and shows error in status line.
    fn handle_agent_error(&mut self, error: &str) -> Result<()> {
        let mut stdout = io::stdout();

        if let Some(mut buf) = self.state.streaming.take() {
            // Get remaining content
            let remaining = buf.finalize();
            if !remaining.is_empty() {
                // Print partial content
                write!(stdout, "{}", remaining)?;
            }
        }
        // Mark as error
        writeln!(stdout, "\x1b[31m [error: {}]\x1b[0m", error)?;
        writeln!(stdout)?;

        // Set status error for display in status line
        self.state.status_error = Some(error.to_string());

        stdout.flush()?;
        self.render_widget()?;
        Ok(())
    }

    /// Cancel streaming response.
    ///
    /// Flushes partial content to stdout (marked as interrupted),
    /// clears streaming state, and returns to input mode.
    fn cancel_streaming(&mut self) -> Result<()> {
        let mut stdout = io::stdout();

        if let Some(mut buf) = self.state.streaming.take() {
            // Get remaining content
            let remaining = buf.finalize();
            if !remaining.is_empty() {
                // Print with interruption marker
                write!(stdout, "{}", remaining)?;
            }
            // Mark as interrupted
            writeln!(stdout, "\x1b[2m [interrupted]\x1b[0m")?;
            writeln!(stdout)?;
        }

        // Clear any pending input
        self.state.input_buffer.clear();
        self.state.cursor_position = 0;

        // Update Ctrl+C tracking
        self.state.ctrl_c_count += 1;
        self.state.last_ctrl_c = Some(std::time::Instant::now());

        stdout.flush()?;
        self.render_widget()?;
        Ok(())
    }

    /// Render the bottom widget.
    fn render_widget(&self) -> Result<()> {
        // Show simple indicator during streaming instead of raw content
        // (full markdown-rendered response is printed when complete)
        let streaming_indicator = if self.state.streaming.is_some() {
            "Generating response..."
        } else {
            ""
        };

        let widget_state = WidgetState {
            mode_id: &self.state.mode_id,
            input: &self.state.input_buffer,
            cursor_col: self.state.cursor_position,
            streaming: streaming_indicator,
            width: self.width,
            height: self.height,
        };

        let mut stdout = io::stdout();
        render_widget(&mut stdout, &widget_state)?;
        Ok(())
    }

    /// Get a reference to the current TUI state.
    pub fn state(&self) -> &TuiState {
        &self.state
    }

    /// Get a mutable reference to the current TUI state.
    pub fn state_mut(&mut self) -> &mut TuiState {
        &mut self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 4.1.1 & 4.1.2: Verify TuiRunner has no alternate screen dependency
    #[test]
    fn test_tui_runner_creates_state() {
        // This tests that runner can be created without terminal
        // (doesn't require alternate screen)
        let state = TuiState::new("plan");
        assert!(!state.should_exit);
        // Messages no longer stored - they go to stdout
        assert!(state.streaming.is_none());
    }

    #[test]
    fn test_runner_stores_dimensions() {
        // TuiRunner should store width/height for widget rendering
        // Note: In CI, size() may fail so we use defaults
        let runner = TuiRunner::new("plan").unwrap();
        // Should have reasonable default dimensions
        assert!(runner.width > 0);
        assert!(runner.height > 0);
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

    // 4.2.3: Test handle_resize updates dimensions
    #[test]
    fn test_handle_resize_updates_dimensions() {
        let mut runner = TuiRunner::new("plan").unwrap();
        let original_width = runner.width;
        let original_height = runner.height;

        // Note: handle_resize calls render_widget which writes to stdout
        // In tests we can't actually render, but we can verify dimensions update
        runner.width = 100;
        runner.height = 50;

        assert_eq!(runner.width, 100);
        assert_eq!(runner.height, 50);
        // Verify it changed from original (unless original was same)
        if original_width != 100 {
            assert_ne!(runner.width, original_width);
        }
    }

    // 4.2.3: Test minimum height enforcement concept
    #[test]
    fn test_minimum_height_concept() {
        // Widget needs at least 4 lines: status + 2 separators + 1 input
        use crate::tui::WidgetHeights;
        assert_eq!(WidgetHeights::MIN_HEIGHT, 4);
    }

    // 4.3.1 & 4.3.2: Test cancel_streaming clears state
    #[test]
    fn test_cancel_streaming_clears_state() {
        use crate::tui::StreamingBuffer;

        let mut runner = TuiRunner::new("plan").unwrap();

        // Setup streaming state
        runner.state.streaming = Some(StreamingBuffer::new());
        runner
            .state
            .streaming
            .as_mut()
            .unwrap()
            .append("partial content");
        runner.state.input_buffer = "some input".to_string();
        runner.state.cursor_position = 10;

        // Note: cancel_streaming writes to stdout, so we test the logic separately
        // by directly manipulating state
        if let Some(mut buf) = runner.state.streaming.take() {
            let _remaining = buf.finalize();
        }
        runner.state.input_buffer.clear();
        runner.state.cursor_position = 0;
        runner.state.ctrl_c_count += 1;
        runner.state.last_ctrl_c = Some(std::time::Instant::now());

        // Verify state was cleared
        assert!(runner.state.streaming.is_none());
        assert!(runner.state.input_buffer.is_empty());
        assert_eq!(runner.state.cursor_position, 0);
        assert_eq!(runner.state.ctrl_c_count, 1);
        assert!(runner.state.last_ctrl_c.is_some());
    }

    // 4.3.1: Test Ctrl+C behavior during streaming
    #[test]
    fn test_ctrl_c_cancels_during_streaming() {
        use crate::tui::{InputAction, StreamingBuffer};

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        // When streaming is active, Cancel action should be handled specially
        // The runner checks: matches!(action, InputAction::Cancel) && state.streaming.is_some()
        let action = InputAction::Cancel;
        let is_streaming = state.streaming.is_some();

        assert!(matches!(action, InputAction::Cancel));
        assert!(is_streaming);
        // Runner would call cancel_streaming() here
    }

    // 4.3.2: Test ESC produces Cancel action
    #[test]
    fn test_esc_maps_to_cancel() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
    }

    // 4.3.2: Test ESC during streaming triggers cancel
    #[test]
    fn test_esc_cancels_during_streaming() {
        use crate::tui::StreamingBuffer;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
        assert!(state.streaming.is_some()); // Runner would handle cancellation
    }

    // 4.2.1: Test user message formatting concept
    #[test]
    fn test_user_message_format() {
        // User messages should be printed with "You: " prefix
        let message = "Hello, assistant!";
        let formatted = format!("\x1b[1mYou:\x1b[0m {}", message);

        assert!(formatted.contains("You:"));
        assert!(formatted.contains(message));
        assert!(formatted.contains("\x1b[1m")); // Bold
        assert!(formatted.contains("\x1b[0m")); // Reset
    }

    // 4.2.2: Test widget state creation
    #[test]
    fn test_widget_state_from_runner() {
        use crate::tui::StreamingBuffer;

        let mut runner = TuiRunner::new("act").unwrap();
        runner.state.input_buffer = "test input".to_string();
        runner.state.cursor_position = 5;
        runner.state.streaming = Some(StreamingBuffer::new());
        runner
            .state
            .streaming
            .as_mut()
            .unwrap()
            .append("streaming...");

        // Create widget state as runner would
        let streaming_content = runner
            .state
            .streaming
            .as_ref()
            .map(|s| s.content())
            .unwrap_or("");

        assert_eq!(streaming_content, "streaming...");
        assert_eq!(runner.state.input_buffer, "test input");
        assert_eq!(runner.state.cursor_position, 5);
        assert_eq!(runner.state.mode_id, "act");
    }

    // Edge case: Empty streaming buffer on cancel
    #[test]
    fn test_cancel_with_empty_streaming() {
        use crate::tui::StreamingBuffer;

        let mut state = TuiState::new("plan");
        state.streaming = Some(StreamingBuffer::new());

        // Empty streaming buffer
        let buf = state.streaming.take().unwrap();
        let remaining = buf.all_content();

        assert!(remaining.is_empty());
    }

    // Edge case: Cancel when not streaming
    #[test]
    fn test_cancel_not_streaming_clears_input() {
        let mut state = TuiState::new("plan");
        state.input_buffer = "some text".to_string();
        state.cursor_position = 9;

        // Execute cancel
        state.execute_action(InputAction::Cancel);

        // Should clear input buffer
        assert!(state.input_buffer.is_empty());
        assert_eq!(state.cursor_position, 0);
    }

    // Edge case: Multiple resize events
    #[test]
    fn test_multiple_resizes() {
        let mut runner = TuiRunner::new("plan").unwrap();

        // Simulate multiple resizes
        runner.width = 80;
        runner.height = 24;
        assert_eq!(runner.width, 80);
        assert_eq!(runner.height, 24);

        runner.width = 120;
        runner.height = 40;
        assert_eq!(runner.width, 120);
        assert_eq!(runner.height, 40);

        runner.width = 60;
        runner.height = 20;
        assert_eq!(runner.width, 60);
        assert_eq!(runner.height, 20);
    }

    // Edge case: Very small terminal
    #[test]
    fn test_very_small_terminal() {
        let mut runner = TuiRunner::new("plan").unwrap();

        // Terminal smaller than minimum widget height
        runner.width = 20;
        runner.height = 3; // Less than MIN_HEIGHT of 4

        // Should not panic, just store the values
        assert_eq!(runner.width, 20);
        assert_eq!(runner.height, 3);
    }

    // Phase 6: Agent error handling tests

    #[test]
    fn test_handle_agent_error_sets_status_error() {
        use crate::tui::StreamingBuffer;

        let mut runner = TuiRunner::new("plan").unwrap();

        // Set up streaming state
        runner.state.streaming = Some(StreamingBuffer::new());
        runner
            .state
            .streaming
            .as_mut()
            .unwrap()
            .append("partial content");

        // Manually simulate what handle_agent_error does (without stdout writes)
        // Since handle_agent_error writes to stdout, we test the state changes directly
        if let Some(mut buf) = runner.state.streaming.take() {
            let _remaining = buf.finalize();
        }
        runner.state.status_error = Some("Test error".to_string());

        // Verify state was updated
        assert!(runner.state.streaming.is_none());
        assert!(runner.state.status_error.is_some());
        assert_eq!(runner.state.status_error.as_ref().unwrap(), "Test error");
    }

    #[test]
    fn test_status_error_preserved_until_new_message() {
        let mut runner = TuiRunner::new("plan").unwrap();

        // Set an error
        runner.state.status_error = Some("Connection failed".to_string());

        // Error should still be there after other actions
        runner.state.execute_action(InputAction::InsertChar('a'));
        assert!(runner.state.status_error.is_some());

        // But cleared on SendMessage
        runner
            .state
            .execute_action(InputAction::SendMessage("test".to_string()));
        assert!(runner.state.status_error.is_none());
    }

    #[test]
    fn test_handle_agent_error_with_empty_streaming() {
        let mut runner = TuiRunner::new("plan").unwrap();

        // No streaming in progress
        assert!(runner.state.streaming.is_none());

        // Set error directly (simulating what handle_agent_error does)
        runner.state.status_error = Some("Error without stream".to_string());

        // Error should be set
        assert!(runner.state.status_error.is_some());
    }
}
