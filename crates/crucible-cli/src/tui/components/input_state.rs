//! Input state management with tui-textarea
//!
//! This module provides `InputState`, a wrapper around tui-textarea that:
//! - Handles text input with readline-style shortcuts
//! - Manages input history with navigation
//! - Returns `EventResult` from the new event system

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::{Input, Key, TextArea};

use crate::tui::event_result::{EventResult, TuiAction};

/// Input state wrapping tui-textarea with history support
#[derive(Debug)]
pub struct InputState<'a> {
    /// The underlying textarea widget
    textarea: TextArea<'a>,
    /// Command history (most recent last)
    history: Vec<String>,
    /// Current position in history (None = not browsing)
    history_index: Option<usize>,
    /// Saved input when browsing history
    saved_input: String,
}

impl<'a> Default for InputState<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> InputState<'a> {
    /// Create a new empty input state
    pub fn new() -> Self {
        let textarea = TextArea::default();
        // Note: tui-textarea has good defaults, we can customize later if needed

        Self {
            textarea,
            history: Vec::new(),
            history_index: None,
            saved_input: String::new(),
        }
    }

    /// Check if the input is empty
    pub fn is_empty(&self) -> bool {
        self.content().is_empty()
    }

    /// Get the current content as a single string
    pub fn content(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Get the content with leading/trailing whitespace trimmed
    pub fn content_trimmed(&self) -> String {
        self.content().trim().to_string()
    }

    /// Set the content, replacing everything
    pub fn set_content(&mut self, text: &str) {
        // Clear and insert new content
        self.textarea.select_all();
        self.textarea.cut();
        self.textarea.insert_str(text);
    }

    /// Clear all content
    pub fn clear(&mut self) {
        self.textarea.select_all();
        self.textarea.cut();
        self.history_index = None;
    }

    /// Add an entry to history (skips duplicates of the last entry)
    pub fn add_to_history(&mut self, entry: String) {
        if entry.is_empty() {
            return;
        }
        // Don't add duplicates of the most recent entry
        if self.history.last() != Some(&entry) {
            self.history.push(entry);
        }
        self.history_index = None;
    }

    /// Navigate to previous history entry (older)
    pub fn history_prev(&mut self) -> EventResult {
        if self.history.is_empty() {
            return EventResult::Handled;
        }

        match self.history_index {
            None => {
                // Save current input before browsing
                self.saved_input = self.content();
                self.history_index = Some(self.history.len() - 1);
            }
            Some(0) => {
                // Already at oldest entry
                return EventResult::Handled;
            }
            Some(idx) => {
                self.history_index = Some(idx - 1);
            }
        }

        if let Some(idx) = self.history_index {
            if let Some(entry) = self.history.get(idx).cloned() {
                self.set_content(&entry);
                return EventResult::NeedsRender;
            }
        }

        EventResult::Handled
    }

    /// Navigate to next history entry (newer)
    pub fn history_next(&mut self) -> EventResult {
        match self.history_index {
            None => EventResult::Handled,
            Some(idx) => {
                if idx + 1 >= self.history.len() {
                    // Past the end, restore saved input
                    self.history_index = None;
                    let saved = self.saved_input.clone();
                    self.set_content(&saved);
                } else {
                    self.history_index = Some(idx + 1);
                    if let Some(entry) = self.history.get(idx + 1).cloned() {
                        self.set_content(&entry);
                    }
                }
                EventResult::NeedsRender
            }
        }
    }

    /// Handle a key event
    ///
    /// Returns an `EventResult` indicating what happened:
    /// - `Action(SendMessage)` when Enter is pressed with content
    /// - `NeedsRender` for most edits
    /// - `Handled` for no-op operations
    /// - `Ignored` when the event should propagate
    pub fn handle_key(&mut self, key: &KeyEvent) -> EventResult {
        match (key.code, key.modifiers) {
            // Enter with content sends message
            (KeyCode::Enter, KeyModifiers::NONE) => {
                let content = self.content_trimmed();
                if content.is_empty() {
                    EventResult::Ignored
                } else {
                    EventResult::Action(TuiAction::SendMessage(content))
                }
            }

            // Ctrl+J or Ctrl+Enter inserts newline
            (KeyCode::Char('j'), KeyModifiers::CONTROL)
            | (KeyCode::Enter, KeyModifiers::CONTROL) => {
                self.textarea.insert_newline();
                EventResult::NeedsRender
            }

            // History navigation
            (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.history_prev()
            }
            (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.history_next()
            }

            // Let tui-textarea handle everything else
            _ => {
                // Convert to tui-textarea's Input type manually (avoids crossterm version mismatch)
                let input = key_event_to_input(key);
                if self.textarea.input(input) {
                    EventResult::NeedsRender
                } else {
                    EventResult::Handled
                }
            }
        }
    }

    /// Get a reference to the underlying TextArea for rendering
    pub fn textarea(&self) -> &TextArea<'a> {
        &self.textarea
    }

    /// Get a mutable reference to the underlying TextArea
    pub fn textarea_mut(&mut self) -> &mut TextArea<'a> {
        &mut self.textarea
    }
}

/// Convert crossterm KeyEvent to tui-textarea Input (avoids version mismatch)
fn key_event_to_input(key: &KeyEvent) -> Input {
    let textarea_key = match key.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Esc => Key::Esc,
        KeyCode::F(n) => Key::F(n),
        _ => Key::Null,
    };

    Input {
        key: textarea_key,
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // Basic creation and content tests
    // ==========================================================================

    #[test]
    fn test_input_state_new_is_empty() {
        let state = InputState::new();
        assert!(state.is_empty());
        assert_eq!(state.content(), "");
        assert_eq!(state.content_trimmed(), "");
    }

    #[test]
    fn test_input_state_set_content() {
        let mut state = InputState::new();
        state.set_content("hello world");
        assert_eq!(state.content(), "hello world");
        assert!(!state.is_empty());
    }

    #[test]
    fn test_input_state_content_trimmed() {
        let mut state = InputState::new();
        state.set_content("  hello world  ");
        assert_eq!(state.content_trimmed(), "hello world");
    }

    #[test]
    fn test_input_state_clear() {
        let mut state = InputState::new();
        state.set_content("hello");
        state.clear();
        assert!(state.is_empty());
    }

    // ==========================================================================
    // History navigation tests
    // ==========================================================================

    #[test]
    fn test_history_navigation() {
        let mut state = InputState::new();
        state.add_to_history("first".into());
        state.add_to_history("second".into());
        state.add_to_history("third".into());

        // Navigate back
        assert_eq!(state.history_prev(), EventResult::NeedsRender);
        assert_eq!(state.content(), "third");

        assert_eq!(state.history_prev(), EventResult::NeedsRender);
        assert_eq!(state.content(), "second");

        // Navigate forward
        assert_eq!(state.history_next(), EventResult::NeedsRender);
        assert_eq!(state.content(), "third");
    }

    #[test]
    fn test_history_preserves_current_input() {
        let mut state = InputState::new();
        state.add_to_history("old".into());
        state.set_content("current typing");

        state.history_prev();
        assert_eq!(state.content(), "old");

        state.history_next();
        assert_eq!(state.content(), "current typing");
    }

    #[test]
    fn test_history_no_duplicates() {
        let mut state = InputState::new();
        state.add_to_history("same".into());
        state.add_to_history("same".into());

        state.history_prev();
        assert_eq!(state.content(), "same");

        // Should be at start, no more history
        assert_eq!(state.history_prev(), EventResult::Handled);
    }

    #[test]
    fn test_empty_history_navigation() {
        let mut state = InputState::new();
        assert_eq!(state.history_prev(), EventResult::Handled);
        assert_eq!(state.history_next(), EventResult::Handled);
    }

    #[test]
    fn test_empty_string_not_added_to_history() {
        let mut state = InputState::new();
        state.add_to_history("".into());
        assert_eq!(state.history_prev(), EventResult::Handled);
    }

    // ==========================================================================
    // Key event handling tests
    // ==========================================================================

    #[test]
    fn test_enter_sends_message() {
        let mut state = InputState::new();
        state.set_content("hello");

        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = state.handle_key(&enter);

        assert!(matches!(
            result,
            EventResult::Action(TuiAction::SendMessage(s)) if s == "hello"
        ));
    }

    #[test]
    fn test_enter_on_empty_is_ignored() {
        let mut state = InputState::new();
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(state.handle_key(&enter), EventResult::Ignored);
    }

    #[test]
    fn test_enter_on_whitespace_only_is_ignored() {
        let mut state = InputState::new();
        state.set_content("   ");
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(state.handle_key(&enter), EventResult::Ignored);
    }

    #[test]
    fn test_ctrl_j_inserts_newline() {
        let mut state = InputState::new();
        state.set_content("line1");

        let ctrl_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
        let result = state.handle_key(&ctrl_j);

        assert_eq!(result, EventResult::NeedsRender);
        assert!(state.content().contains('\n'));
    }

    #[test]
    fn test_char_input_returns_needs_render() {
        let mut state = InputState::new();
        let a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);

        assert_eq!(state.handle_key(&a), EventResult::NeedsRender);
        assert_eq!(state.content(), "a");
    }

    #[test]
    fn test_typing_multiple_chars() {
        let mut state = InputState::new();

        state.handle_key(&KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        state.handle_key(&KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));

        assert_eq!(state.content(), "hi");
    }

    // ==========================================================================
    // Readline shortcut tests (delegated to tui-textarea)
    // ==========================================================================

    #[test]
    fn test_ctrl_w_deletes_word() {
        let mut state = InputState::new();
        state.set_content("hello world");
        // tui-textarea cursor is at end after set_content

        let ctrl_w = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
        state.handle_key(&ctrl_w);

        assert_eq!(state.content(), "hello ");
    }

    #[test]
    fn test_ctrl_a_moves_to_start() {
        let mut state = InputState::new();
        state.set_content("hello");

        let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        state.handle_key(&ctrl_a);

        // Verify by typing - should insert at start
        let x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        state.handle_key(&x);

        assert_eq!(state.content(), "xhello");
    }

    #[test]
    fn test_ctrl_e_moves_to_end() {
        let mut state = InputState::new();
        state.set_content("hello");

        // Move to start first
        let ctrl_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        state.handle_key(&ctrl_a);

        // Then move to end
        let ctrl_e = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
        state.handle_key(&ctrl_e);

        // Type - should append
        let x = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        state.handle_key(&x);

        assert_eq!(state.content(), "hellox");
    }

    #[test]
    fn test_backspace_deletes_char() {
        let mut state = InputState::new();
        state.set_content("hello");

        let backspace = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        state.handle_key(&backspace);

        assert_eq!(state.content(), "hell");
    }

    #[test]
    fn test_ctrl_u_clears_to_start() {
        let mut state = InputState::new();
        state.set_content("hello world");

        let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        state.handle_key(&ctrl_u);

        assert!(state.is_empty());
    }

    // ==========================================================================
    // History key binding tests
    // ==========================================================================

    #[test]
    fn test_up_arrow_navigates_history() {
        let mut state = InputState::new();
        state.add_to_history("previous".into());

        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        state.handle_key(&up);

        assert_eq!(state.content(), "previous");
    }

    #[test]
    fn test_down_arrow_navigates_history() {
        let mut state = InputState::new();
        state.add_to_history("first".into());
        state.add_to_history("second".into());

        // Go back twice
        state.handle_key(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        state.handle_key(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(state.content(), "first");

        // Go forward
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        state.handle_key(&down);
        assert_eq!(state.content(), "second");
    }

    #[test]
    fn test_ctrl_p_navigates_history() {
        let mut state = InputState::new();
        state.add_to_history("previous".into());

        let ctrl_p = KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL);
        state.handle_key(&ctrl_p);

        assert_eq!(state.content(), "previous");
    }

    #[test]
    fn test_ctrl_n_navigates_history() {
        let mut state = InputState::new();
        state.add_to_history("first".into());
        state.add_to_history("second".into());
        state.set_content("current");

        // Go back twice
        state.handle_key(&KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        state.handle_key(&KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));

        // Go forward with Ctrl+N
        let ctrl_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        state.handle_key(&ctrl_n);

        assert_eq!(state.content(), "second");
    }
}
