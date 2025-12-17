//! Input handling for TUI

//!
//! Maps crossterm key events to TUI actions.

use crate::tui::state::TuiState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

/// Actions that can be performed from keyboard input
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    SendMessage(String),
    InsertNewline,
    InsertChar(char),
    DeleteChar,
    MoveCursorLeft,
    MoveCursorRight,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    CycleMode,
    Cancel,
    Exit,
    None,
}

/// Time window for double Ctrl+C to trigger exit
const DOUBLE_CTRL_C_WINDOW: Duration = Duration::from_millis(500);

/// Map a crossterm key event to a TUI action
pub fn map_key_event(event: &KeyEvent, state: &TuiState) -> InputAction {
    match (event.code, event.modifiers) {
        // Ctrl+C handling - single cancels, double exits
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            if let Some(last) = state.last_ctrl_c {
                if last.elapsed() < DOUBLE_CTRL_C_WINDOW && state.ctrl_c_count >= 1 {
                    return InputAction::Exit;
                }
            }
            InputAction::Cancel
        }

        // ESC key also cancels (same behavior as single Ctrl+C)
        (KeyCode::Esc, _) => InputAction::Cancel,

        // Ctrl+J inserts newline
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => InputAction::InsertNewline,

        // Enter sends message if buffer non-empty
        (KeyCode::Enter, KeyModifiers::NONE) => {
            if state.input_buffer.trim().is_empty() {
                InputAction::None
            } else {
                InputAction::SendMessage(state.input_buffer.clone())
            }
        }

        // Shift+Tab cycles mode
        (KeyCode::BackTab, _) => InputAction::CycleMode,

        // Navigation
        (KeyCode::Up, KeyModifiers::NONE) => InputAction::ScrollUp,
        (KeyCode::Down, KeyModifiers::NONE) => InputAction::ScrollDown,
        (KeyCode::PageUp, _) => InputAction::PageUp,
        (KeyCode::PageDown, _) => InputAction::PageDown,
        (KeyCode::Left, KeyModifiers::NONE) => InputAction::MoveCursorLeft,
        (KeyCode::Right, KeyModifiers::NONE) => InputAction::MoveCursorRight,

        // Editing
        (KeyCode::Backspace, _) => InputAction::DeleteChar,
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => InputAction::InsertChar(c),

        _ => InputAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    #[test]
    fn test_map_key_enter_sends_message() {
        let mut state = TuiState::new("plan");
        state.input_buffer = "Hello".into();

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert!(matches!(action, InputAction::SendMessage(msg) if msg == "Hello"));
    }

    #[test]
    fn test_map_key_enter_empty_does_nothing() {
        let state = TuiState::new("plan");

        let event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::None);
    }

    #[test]
    fn test_map_key_ctrl_j_inserts_newline() {
        let state = TuiState::new("plan");

        let event = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::InsertNewline);
    }

    #[test]
    fn test_map_key_ctrl_c_once_cancels() {
        let state = TuiState::new("plan");

        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
    }

    #[test]
    fn test_map_key_ctrl_c_twice_exits() {
        let mut state = TuiState::new("plan");
        state.ctrl_c_count = 1;
        state.last_ctrl_c = Some(Instant::now());

        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Exit);
    }

    #[test]
    fn test_map_key_shift_tab_cycles_mode() {
        let state = TuiState::new("plan");

        let event = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::CycleMode);
    }

    #[test]
    fn test_map_key_esc_cancels() {
        let state = TuiState::new("plan");

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
    }

    #[test]
    fn test_map_key_esc_with_modifiers() {
        // ESC should cancel regardless of modifiers
        let state = TuiState::new("plan");

        let event = KeyEvent::new(KeyCode::Esc, KeyModifiers::SHIFT);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::Cancel);
    }

    #[test]
    fn test_map_key_char_inserts() {
        let state = TuiState::new("plan");

        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::InsertChar('a'));
    }
}
