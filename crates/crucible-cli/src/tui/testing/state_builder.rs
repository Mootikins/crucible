//! Test utilities for TUI snapshot testing
//!
//! Provides helpers for creating test terminals and building TUI state
//! for snapshot tests with insta.

use crate::tui::render::render;
use crate::tui::state::TuiState;
use crate::tui::streaming::StreamingBuffer;
use ratatui::{backend::TestBackend, Terminal};

/// Standard test terminal width
pub const TEST_WIDTH: u16 = 80;

/// Standard test terminal height
pub const TEST_HEIGHT: u16 = 24;

/// Create a test terminal with standard dimensions (80x24)
pub fn test_terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(TEST_WIDTH, TEST_HEIGHT)).unwrap()
}

/// Create a test terminal with custom dimensions
pub fn test_terminal_sized(width: u16, height: u16) -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(width, height)).unwrap()
}

/// Render state to a terminal and return it for snapshot testing
pub fn render_to_terminal(state: &TuiState) -> Terminal<TestBackend> {
    let mut terminal = test_terminal();
    terminal.draw(|f| render(f, state)).unwrap();
    terminal
}

/// Fluent builder for constructing TuiState in tests
///
/// # Example
///
/// ```ignore
/// let state = TestStateBuilder::new("plan")
///     .with_input("/help")
///     .build();
/// ```
pub struct TestStateBuilder {
    mode_id: String,
    input_buffer: String,
    cursor_position: usize,
    streaming_content: Option<String>,
    status_error: Option<String>,
}

impl TestStateBuilder {
    /// Create a new builder with the specified mode
    pub fn new(mode: &str) -> Self {
        Self {
            mode_id: mode.to_string(),
            input_buffer: String::new(),
            cursor_position: 0,
            streaming_content: None,
            status_error: None,
        }
    }

    /// Set the input buffer content (cursor moves to end)
    pub fn with_input(mut self, text: &str) -> Self {
        self.input_buffer = text.to_string();
        self.cursor_position = text.len();
        self
    }

    /// Set the input buffer with explicit cursor position
    pub fn with_input_and_cursor(mut self, text: &str, cursor: usize) -> Self {
        self.input_buffer = text.to_string();
        self.cursor_position = cursor.min(text.len());
        self
    }

    /// Set streaming content (simulates active streaming)
    pub fn with_streaming(mut self, content: &str) -> Self {
        self.streaming_content = Some(content.to_string());
        self
    }

    /// Set a status error message
    pub fn with_error(mut self, error: &str) -> Self {
        self.status_error = Some(error.to_string());
        self
    }

    /// Build the TuiState
    pub fn build(self) -> TuiState {
        let mut state = TuiState::new(&self.mode_id);
        *state.input_mut() = self.input_buffer;
        state.set_cursor(self.cursor_position);
        state.status_error = self.status_error;

        if let Some(content) = self.streaming_content {
            let mut buf = StreamingBuffer::new();
            // Append content to the buffer
            buf.append(&content);
            state.streaming = Some(buf);
        }

        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_basic() {
        let state = TestStateBuilder::new("plan").build();
        assert_eq!(state.mode_id(), "plan");
        assert_eq!(state.input(), "");
    }

    #[test]
    fn test_builder_with_input() {
        let state = TestStateBuilder::new("act").with_input("/help").build();
        assert_eq!(state.input(), "/help");
        assert_eq!(state.cursor(), 5);
    }

    #[test]
    fn test_builder_with_streaming() {
        let state = TestStateBuilder::new("plan")
            .with_streaming("Hello, I am streaming...")
            .build();

        assert!(state.streaming.is_some());
    }

    #[test]
    fn test_terminal_creation() {
        let terminal = test_terminal();
        let size = terminal.size().unwrap();
        assert_eq!(size.width, TEST_WIDTH);
        assert_eq!(size.height, TEST_HEIGHT);
    }
}
