//! ChatInput - tui-textarea wrapper for chat input
//!
//! Wraps tui-textarea to provide chat-specific input handling including
//! submit behavior and completion triggers.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};
use tui_textarea::{Input, TextArea};

/// Actions that can result from input handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatAction {
    /// Send the current message
    Send(String),
    /// Trigger command completion
    TriggerCommandCompletion,
    /// Trigger file/agent completion
    TriggerFileCompletion,
    /// No action needed
    None,
}

/// Chat input widget wrapping tui-textarea
pub struct ChatInput {
    textarea: TextArea<'static>,
}

impl ChatInput {
    /// Create a new chat input
    pub fn new() -> Self {
        let mut textarea = TextArea::default();

        // Configure for chat input - minimal styling
        textarea.set_cursor_line_style(ratatui::style::Style::default());
        textarea
            .set_block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::NONE));

        Self { textarea }
    }

    /// Handle a key event, returning any resulting action
    pub fn handle_key(&mut self, key: KeyEvent) -> ChatAction {
        match key.code {
            // Send on Enter (no modifiers)
            KeyCode::Enter if key.modifiers.is_empty() => {
                let content = self.content();
                if !content.trim().is_empty() {
                    self.clear();
                    ChatAction::Send(content)
                } else {
                    ChatAction::None
                }
            }

            // Newline on Ctrl+Enter or Shift+Enter
            KeyCode::Enter
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    || key.modifiers.contains(KeyModifiers::SHIFT) =>
            {
                self.textarea.insert_newline();
                ChatAction::None
            }

            // Completion triggers
            KeyCode::Char('/') if self.at_word_start() => {
                self.textarea.input(Input::from(key));
                ChatAction::TriggerCommandCompletion
            }
            KeyCode::Char('@') if self.at_word_start() => {
                self.textarea.input(Input::from(key));
                ChatAction::TriggerFileCompletion
            }

            // Default handling
            _ => {
                self.textarea.input(Input::from(key));
                ChatAction::None
            }
        }
    }

    /// Get the current input content
    pub fn content(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.textarea = TextArea::default();
        self.textarea
            .set_cursor_line_style(ratatui::style::Style::default());
        self.textarea
            .set_block(ratatui::widgets::Block::default().borders(ratatui::widgets::Borders::NONE));
    }

    /// Calculate the height needed for the input
    pub fn height(&self, _width: u16) -> u16 {
        let lines = self.textarea.lines().len() as u16;
        lines.clamp(1, 5) // 1-5 lines
    }

    /// Check if cursor is at the start of a word (for completion triggers)
    fn at_word_start(&self) -> bool {
        let (row, col) = self.textarea.cursor();
        if col == 0 {
            return true;
        }

        // Check character before cursor
        if let Some(line) = self.textarea.lines().get(row) {
            if let Some(ch) = line.chars().nth(col - 1) {
                return ch.is_whitespace();
            }
        }

        true
    }

    /// Render the input widget
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(&self.textarea, area);
    }

    /// Get mutable access to the underlying textarea
    pub fn textarea_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.textarea
    }

    /// Insert text at cursor position
    pub fn insert_str(&mut self, s: &str) {
        self.textarea.insert_str(s);
    }
}

impl Default for ChatInput {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_editing() {
        let mut input = ChatInput::new();

        // Type some text
        input.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));

        assert_eq!(input.content(), "hello");

        // Test cursor movement - move to start with Home
        input.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));

        // Insert at beginning
        input.handle_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE));
        assert_eq!(input.content(), "Xhello");

        // Test cursor movement - move to end with End
        input.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
        assert_eq!(input.content(), "Xhello!");

        // Test arrow keys - move left
        input.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('_'), KeyModifiers::NONE));
        assert_eq!(input.content(), "Xhell_o!");

        // Test arrow keys - move right
        input.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::NONE));
        assert_eq!(input.content(), "Xhell_oO!");

        // Test backspace - delete character before cursor
        input.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(input.content(), "Xhell_o!");

        // Test delete - delete character at cursor
        input.handle_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));
        assert_eq!(input.content(), "Xhell_o");

        // Test multi-line editing with Ctrl+Enter
        input.clear();
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(input.content(), "line1\nline2");

        // Test Up arrow to move to previous line
        input.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::NONE));
        assert_eq!(input.content(), "line1X\nline2");

        // Test Down arrow to move to next line
        input.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('Y'), KeyModifiers::NONE));
        assert_eq!(input.content(), "line1X\nline2Y");

        // Test word-based movement - Ctrl+Left to move back by word
        input.clear();
        input.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(input.content(), "hello world");

        // Move back one word (Ctrl+Left)
        input.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('_'), KeyModifiers::NONE));
        assert_eq!(input.content(), "hello _world");

        // Move forward one word (Ctrl+Right)
        input.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE));
        assert_eq!(input.content(), "hello _world!");
    }

    #[test]
    fn test_submit_behavior() {
        let mut input = ChatInput::new();

        // Type a message
        input.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));

        // Press Enter to submit
        let action = input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(action, ChatAction::Send("hi".to_string()));
        assert_eq!(input.content(), ""); // Cleared after send
    }

    #[test]
    fn test_empty_submit_does_nothing() {
        let mut input = ChatInput::new();

        let action = input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, ChatAction::None);
    }

    #[test]
    fn test_ctrl_enter_newline() {
        let mut input = ChatInput::new();

        input.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));

        assert_eq!(input.content(), "a\nb");
    }

    #[test]
    fn test_command_completion_trigger() {
        let mut input = ChatInput::new();

        let action = input.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(action, ChatAction::TriggerCommandCompletion);
        assert_eq!(input.content(), "/");
    }

    #[test]
    fn test_file_completion_trigger() {
        let mut input = ChatInput::new();

        let action = input.handle_key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));
        assert_eq!(action, ChatAction::TriggerFileCompletion);
        assert_eq!(input.content(), "@");
    }

    #[test]
    fn test_no_trigger_mid_word() {
        let mut input = ChatInput::new();

        // Type some text first
        input.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));

        // @ mid-word should not trigger completion
        let action = input.handle_key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));
        assert_eq!(action, ChatAction::None);
        assert_eq!(input.content(), "h@");
    }

    #[test]
    fn test_input_height() {
        let mut input = ChatInput::new();

        // Single line should return height 1
        assert_eq!(input.height(80), 1);

        // Add one line of text - still height 1
        input.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(input.height(80), 1);

        // Add second line - height should be 2
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE));
        assert_eq!(input.height(80), 2);

        // Add third line - height should be 3
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE));
        assert_eq!(input.height(80), 3);

        // Add fourth line - height should be 4
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE));
        assert_eq!(input.height(80), 4);

        // Add fifth line - height should be 5
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('5'), KeyModifiers::NONE));
        assert_eq!(input.height(80), 5);

        // Add sixth line - height should be clamped to 5 (max)
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('6'), KeyModifiers::NONE));
        assert_eq!(input.height(80), 5);

        // Add seventh line - still clamped at 5
        input.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        input.handle_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE));
        input.handle_key(KeyEvent::new(KeyCode::Char('7'), KeyModifiers::NONE));
        assert_eq!(input.height(80), 5);

        // Clear and verify height goes back to 1 (minimum)
        input.clear();
        assert_eq!(input.height(80), 1);
    }
}
