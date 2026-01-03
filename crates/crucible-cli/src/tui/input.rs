//! Input handling for TUI

//!
//! Maps crossterm key events to TUI actions.

use crate::tui::popup::is_exact_slash_command;
use crate::tui::state::TuiState;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

/// Actions that can be performed from keyboard input
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    SendMessage(String),
    InsertNewline,
    InsertChar(char),
    DeleteChar,
    MoveCursorLeft,
    MoveCursorRight,
    MovePopupSelection(isize),
    ConfirmPopup,
    ExecuteSlashCommand(String),
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    ScrollToTop,
    ScrollToBottom,
    CycleMode,
    HistoryPrev,
    HistoryNext,
    Cancel,
    Exit,
    // Readline-style editing (emacs mode)
    DeleteWordBackward, // Ctrl+W
    DeleteToLineStart,  // Ctrl+U
    DeleteToLineEnd,    // Ctrl+K
    MoveCursorToStart,  // Ctrl+A
    MoveCursorToEnd,    // Ctrl+E
    MoveWordBackward,   // Alt+B
    MoveWordForward,    // Alt+F
    TransposeChars,     // Ctrl+T
    // Reasoning display toggle
    ToggleReasoning, // Alt+T
    // Mouse capture toggle (for text selection)
    ToggleMouseCapture, // Alt+M
    // Copy last message as markdown
    CopyMarkdown, // Alt+C
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

        // Ctrl+D exits immediately
        (KeyCode::Char('d'), KeyModifiers::CONTROL) => InputAction::Exit,

        // ESC key also cancels (same behavior as single Ctrl+C)
        (KeyCode::Esc, _) => InputAction::Cancel,

        // Ctrl+J inserts newline
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => InputAction::InsertNewline,

        // Readline-style editing (emacs mode)
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => InputAction::DeleteWordBackward,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => InputAction::DeleteToLineStart,
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => InputAction::DeleteToLineEnd,
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => InputAction::MoveCursorToStart,
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => InputAction::MoveCursorToEnd,
        (KeyCode::Char('b'), KeyModifiers::ALT) => InputAction::MoveWordBackward,
        (KeyCode::Char('f'), KeyModifiers::ALT) => InputAction::MoveWordForward,
        (KeyCode::Char('t'), KeyModifiers::CONTROL) => InputAction::TransposeChars,
        (KeyCode::Char('t'), KeyModifiers::ALT) => InputAction::ToggleReasoning,
        (KeyCode::Char('m'), KeyModifiers::ALT) => InputAction::ToggleMouseCapture,
        (KeyCode::Char('c'), KeyModifiers::ALT) => InputAction::CopyMarkdown,

        // Enter: confirm popup if active with / or @, otherwise send message
        (KeyCode::Enter, KeyModifiers::NONE) => {
            let trimmed = state.input_buffer.trim();
            if trimmed.is_empty() {
                InputAction::None
            } else if trimmed == "/exit" || trimmed == "/quit" || trimmed == "/q" {
                InputAction::Exit
            } else if trimmed.starts_with('/') {
                // Check if this is an exact command match
                if is_exact_slash_command(trimmed) {
                    InputAction::ExecuteSlashCommand(trimmed.to_string())
                } else if state.has_popup {
                    InputAction::ConfirmPopup
                } else {
                    // Unknown command, no popup - do nothing
                    InputAction::None
                }
            } else if trimmed.starts_with('@') && state.has_popup {
                InputAction::ConfirmPopup
            } else {
                InputAction::SendMessage(state.input_buffer.clone())
            }
        }

        // Shift+Tab cycles mode
        (KeyCode::BackTab, _) => InputAction::CycleMode,

        // Scrolling
        (KeyCode::Up, KeyModifiers::CONTROL) => InputAction::ScrollUp,
        (KeyCode::Down, KeyModifiers::CONTROL) => InputAction::ScrollDown,
        (KeyCode::Char('u'), mods) if mods == KeyModifiers::CONTROL.union(KeyModifiers::ALT) => {
            InputAction::HalfPageUp
        }
        (KeyCode::Char('d'), mods) if mods == KeyModifiers::CONTROL.union(KeyModifiers::ALT) => {
            InputAction::HalfPageDown
        }
        (KeyCode::Home, KeyModifiers::NONE) => InputAction::ScrollToTop,
        (KeyCode::End, KeyModifiers::NONE) => InputAction::ScrollToBottom,
        (KeyCode::PageUp, _) => InputAction::PageUp,
        (KeyCode::PageDown, _) => InputAction::PageDown,

        // Navigation: Up/Down depend on whether popup is active
        (KeyCode::Up, KeyModifiers::NONE) => {
            if state.has_popup {
                InputAction::MovePopupSelection(-1)
            } else {
                InputAction::HistoryPrev
            }
        }
        (KeyCode::Down, KeyModifiers::NONE) => {
            if state.has_popup {
                InputAction::MovePopupSelection(1)
            } else {
                InputAction::HistoryNext
            }
        }
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => InputAction::MovePopupSelection(-1),
        (KeyCode::Char('n'), KeyModifiers::CONTROL) => InputAction::MovePopupSelection(1),
        (KeyCode::Left, KeyModifiers::NONE) => InputAction::MoveCursorLeft,
        (KeyCode::Right, KeyModifiers::NONE) => InputAction::MoveCursorRight,

        // Tab confirms popup selection
        (KeyCode::Tab, KeyModifiers::NONE) => InputAction::ConfirmPopup,

        // Editing
        (KeyCode::Backspace, _) => InputAction::DeleteChar,
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => InputAction::InsertChar(c),

        _ => InputAction::None,
    }
}

#[cfg(test)]
mod scroll_keybinding_tests {
    use super::*;

    #[test]
    fn test_ctrl_up_scrolls_up() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::ScrollUp);
    }

    #[test]
    fn test_ctrl_down_scrolls_down() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::ScrollDown);
    }

    #[test]
    fn test_ctrl_alt_u_half_page_up() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(
            KeyCode::Char('u'),
            KeyModifiers::CONTROL.union(KeyModifiers::ALT),
        );
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::HalfPageUp);
    }

    #[test]
    fn test_ctrl_alt_d_half_page_down() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL.union(KeyModifiers::ALT),
        );
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::HalfPageDown);
    }

    #[test]
    fn test_home_scrolls_to_top() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::ScrollToTop);
    }

    #[test]
    fn test_end_scrolls_to_bottom() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::ScrollToBottom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

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

#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn test_up_without_popup_recalls_history() {
        // When no popup is active, Up should recall previous command from history
        let state = TuiState::new("plan");
        // popup is None by default

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::HistoryPrev);
    }

    #[test]
    fn test_down_without_popup_advances_history() {
        // When no popup is active, Down should go forward in history
        let state = TuiState::new("plan");

        let event = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::HistoryNext);
    }

    #[test]
    fn test_up_with_popup_moves_selection() {
        // When popup is active, Up should move popup selection (not history)
        let mut state = TuiState::new("plan");
        state.has_popup = true;

        let event = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        let action = map_key_event(&event, &state);

        assert_eq!(action, InputAction::MovePopupSelection(-1));
    }
}

#[cfg(test)]
mod slash_command_tests {
    use super::*;

    fn make_enter() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }

    #[test]
    fn test_enter_partial_command_with_popup_confirms() {
        // Input: "/hel" with popup open
        // Expected: ConfirmPopup (fill in the completion)
        let mut state = TuiState::new("plan");
        state.input_buffer = "/hel".to_string();
        state.has_popup = true;

        let action = map_key_event(&make_enter(), &state);
        assert!(
            matches!(action, InputAction::ConfirmPopup),
            "Partial command with popup should confirm, got: {:?}",
            action
        );
    }

    #[test]
    fn test_enter_exact_command_executes() {
        // Input: "/help" (exact match, no popup needed)
        // Expected: ExecuteSlashCommand("/help")
        let mut state = TuiState::new("plan");
        state.input_buffer = "/help".to_string();

        let action = map_key_event(&make_enter(), &state);
        assert!(
            matches!(action, InputAction::ExecuteSlashCommand(ref cmd) if cmd == "/help"),
            "Exact command should execute, got: {:?}",
            action
        );
    }

    #[test]
    fn test_enter_exact_command_with_args_executes() {
        // Input: "/mode code" (command with args)
        // Expected: ExecuteSlashCommand("/mode code")
        let mut state = TuiState::new("plan");
        state.input_buffer = "/mode code".to_string();

        let action = map_key_event(&make_enter(), &state);
        assert!(
            matches!(action, InputAction::ExecuteSlashCommand(ref cmd) if cmd == "/mode code"),
            "Exact command with args should execute, got: {:?}",
            action
        );
    }

    #[test]
    fn test_enter_unknown_command_no_popup_noop() {
        // Input: "/xyz" (unknown command, no popup)
        // Expected: None (do nothing)
        let mut state = TuiState::new("plan");
        state.input_buffer = "/xyz".to_string();
        state.has_popup = false;

        let action = map_key_event(&make_enter(), &state);
        assert!(
            matches!(action, InputAction::None),
            "Unknown command without popup should do nothing, got: {:?}",
            action
        );
    }

    #[test]
    fn test_enter_at_with_popup_confirms() {
        // Input: "@agent" with popup open
        // Expected: ConfirmPopup
        let mut state = TuiState::new("plan");
        state.input_buffer = "@agent".to_string();
        state.has_popup = true;

        let action = map_key_event(&make_enter(), &state);
        assert!(
            matches!(action, InputAction::ConfirmPopup),
            "@ trigger with popup should confirm, got: {:?}",
            action
        );
    }
}

#[cfg(test)]
mod readline_tests {
    use super::*;

    #[test]
    fn test_ctrl_w_delete_word_backward() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::DeleteWordBackward);
    }

    #[test]
    fn test_ctrl_u_delete_to_line_start() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::DeleteToLineStart);
    }

    #[test]
    fn test_ctrl_k_delete_to_line_end() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::DeleteToLineEnd);
    }

    #[test]
    fn test_ctrl_a_move_to_start() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::MoveCursorToStart);
    }

    #[test]
    fn test_ctrl_e_move_to_end() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::MoveCursorToEnd);
    }

    #[test]
    fn test_alt_b_move_word_backward() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::MoveWordBackward);
    }

    #[test]
    fn test_alt_f_move_word_forward() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::MoveWordForward);
    }

    #[test]
    fn test_ctrl_t_transpose_chars() {
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::TransposeChars);
    }

    #[test]
    fn test_alt_t_toggle_reasoning() {
        // Alt+T should toggle reasoning visibility
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('t'), KeyModifiers::ALT);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::ToggleReasoning);
    }

    #[test]
    fn test_alt_m_toggle_mouse_capture() {
        // Alt+M should toggle mouse capture for text selection
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::ALT);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::ToggleMouseCapture);
    }

    #[test]
    fn test_alt_c_copy_markdown() {
        // Alt+C should copy last message as markdown
        let state = TuiState::new("plan");
        let event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT);
        let action = map_key_event(&event, &state);
        assert_eq!(action, InputAction::CopyMarkdown);
    }
}
