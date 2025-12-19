//! Test utilities for TUI snapshot testing
//!
//! Provides helpers for creating test terminals and building TUI state
//! for snapshot tests with insta.

use crate::tui::render::render;
use crate::tui::state::{PopupItem, PopupItemKind, PopupKind, PopupState, TuiState};
use crate::tui::streaming::StreamingBuffer;
use ratatui::{backend::TestBackend, Terminal};
use std::time::Instant;

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
///     .with_popup_items(PopupKind::Command, vec![
///         PopupItem::command("help", "Show help"),
///     ])
///     .build();
/// ```
pub struct TestStateBuilder {
    mode_id: String,
    input_buffer: String,
    cursor_position: usize,
    popup: Option<PopupState>,
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
            popup: None,
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

    /// Add popup with items
    pub fn with_popup_items(mut self, kind: PopupKind, items: Vec<PopupItem>) -> Self {
        self.popup = Some(PopupState {
            kind,
            query: String::new(),
            items,
            selected: 0,
            last_update: Instant::now(),
        });
        self
    }

    /// Set which popup item is selected (0-indexed)
    pub fn with_popup_selected(mut self, index: usize) -> Self {
        if let Some(ref mut popup) = self.popup {
            popup.selected = index.min(popup.items.len().saturating_sub(1));
        }
        self
    }

    /// Set the popup query string
    pub fn with_popup_query(mut self, query: &str) -> Self {
        if let Some(ref mut popup) = self.popup {
            popup.query = query.to_string();
        }
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
        state.input_buffer = self.input_buffer;
        state.cursor_position = self.cursor_position;
        state.popup = self.popup;
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

// Convenience constructors for PopupItem
impl PopupItem {
    /// Create a command popup item
    pub fn command(name: &str, desc: &str) -> Self {
        Self {
            kind: PopupItemKind::Command,
            title: format!("/{}", name),
            subtitle: desc.to_string(),
            token: format!("/{} ", name),
            score: 0,
            available: true,
        }
    }

    /// Create an agent popup item
    pub fn agent(id: &str, desc: &str) -> Self {
        Self {
            kind: PopupItemKind::Agent,
            title: format!("@{}", id),
            subtitle: desc.to_string(),
            token: format!("@{}", id),
            score: 0,
            available: true,
        }
    }

    /// Create a file popup item
    pub fn file(path: &str, subtitle: &str) -> Self {
        Self {
            kind: PopupItemKind::File,
            title: path.to_string(),
            subtitle: subtitle.to_string(),
            token: path.to_string(),
            score: 0,
            available: true,
        }
    }

    /// Create a note popup item
    pub fn note(path: &str, subtitle: &str) -> Self {
        Self {
            kind: PopupItemKind::Note,
            title: path.to_string(),
            subtitle: subtitle.to_string(),
            token: path.to_string(),
            score: 0,
            available: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_basic() {
        let state = TestStateBuilder::new("plan").build();
        assert_eq!(state.mode_id, "plan");
        assert_eq!(state.input_buffer, "");
        assert!(state.popup.is_none());
    }

    #[test]
    fn test_builder_with_input() {
        let state = TestStateBuilder::new("act")
            .with_input("/help")
            .build();
        assert_eq!(state.input_buffer, "/help");
        assert_eq!(state.cursor_position, 5);
    }

    #[test]
    fn test_builder_with_popup() {
        let state = TestStateBuilder::new("plan")
            .with_popup_items(PopupKind::Command, vec![
                PopupItem::command("help", "Show help"),
                PopupItem::command("exit", "Exit"),
            ])
            .with_popup_selected(1)
            .build();

        let popup = state.popup.unwrap();
        assert_eq!(popup.items.len(), 2);
        assert_eq!(popup.selected, 1);
    }

    #[test]
    fn test_builder_with_streaming() {
        let state = TestStateBuilder::new("plan")
            .with_streaming("Hello, I am streaming...")
            .build();

        assert!(state.streaming.is_some());
    }

    #[test]
    fn test_popup_item_constructors() {
        let cmd = PopupItem::command("search", "Search files");
        assert_eq!(cmd.title, "/search");
        assert_eq!(cmd.kind, PopupItemKind::Command);

        let agent = PopupItem::agent("dev-helper", "Developer assistant");
        assert_eq!(agent.title, "@dev-helper");
        assert_eq!(agent.kind, PopupItemKind::Agent);

        let file = PopupItem::file("src/main.rs", "workspace");
        assert_eq!(file.title, "src/main.rs");
        assert_eq!(file.kind, PopupItemKind::File);

        let note = PopupItem::note("note:project/todo.md", "note");
        assert_eq!(note.title, "note:project/todo.md");
        assert_eq!(note.kind, PopupItemKind::Note);
    }

    #[test]
    fn test_terminal_creation() {
        let terminal = test_terminal();
        let size = terminal.size().unwrap();
        assert_eq!(size.width, TEST_WIDTH);
        assert_eq!(size.height, TEST_HEIGHT);
    }
}
