//! Action execution for TUI state
//!
//! Handles InputAction dispatch and state mutations.

use crate::tui::InputAction;
use crate::tui::state::TuiState;
use crucible_core::traits::chat::cycle_mode_id;

/// Executes input actions against TuiState
///
/// Provides a clean separation between action dispatch logic
/// and state management, following SOLID principles.
pub struct ActionExecutor;

impl ActionExecutor {
    /// Execute an action against the given TuiState
    ///
    /// Returns Some(message) if the action should trigger a message send,
    /// None otherwise.
    pub fn execute_action(state: &mut TuiState, action: InputAction) -> Option<String> {
        match action {
            InputAction::SendMessage(msg) => {
                // Return message to send - caller is responsible for clearing input
                state.status_error = None;
                Some(msg)
            }
            InputAction::CycleMode => {
                let new_mode_id = cycle_mode_id(&state.view.mode_id);
                state.view.mode_id = new_mode_id.to_string();
                state.mode_name =
                    crucible_core::traits::chat::mode_display_name(new_mode_id).to_string();
                None
            }
            InputAction::Exit => {
                state.should_exit = true;
                None
            }
            InputAction::Cancel => {
                // Track Ctrl+C for double-press detection
                // Also clear input buffer and reset cursor
                state.ctrl_c_count += 1;
                state.last_ctrl_c = Some(std::time::Instant::now());
                state.view.input_buffer.clear();
                state.view.cursor_position = 0;
                None
            }
            InputAction::ToggleReasoning => {
                state.view.show_reasoning = !state.view.show_reasoning;
                None
            }
            InputAction::ExecuteSlashCommand(_cmd) => {
                // TODO: Slash command execution
                None
            }
            // All input-related actions now handled by ViewState/runner:
            // - InsertNewline, InsertChar, DeleteChar
            // - MoveCursorLeft, MoveCursorRight, MoveCursorToStart, MoveCursorToEnd
            // - MoveWordBackward, MoveWordForward
            // - DeleteWordBackward, DeleteToLineStart, DeleteToLineEnd
            // - TransposeChars
            // - MovePopupSelection, ConfirmPopup
            // - Scroll actions (handled by view)
            InputAction::InsertNewline => {
                state.view.input_buffer.push('\n');
                None
            }
            InputAction::InsertChar(c) => {
                state.view.input_buffer.insert(state.view.cursor_position, c);
                state.view.cursor_position += c.len_utf8();
                None
            }
            InputAction::DeleteChar => {
                if state.view.cursor_position < state.view.input_buffer.len() {
                    // Find the end of the current character (for UTF-8 support)
                    let char_end = state.view.input_buffer[state.view.cursor_position..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| state.view.cursor_position + i)
                        .unwrap_or(state.view.input_buffer.len());
                    state.view.input_buffer.replace_range(state.view.cursor_position..char_end, "");
                }
                None
            }
            InputAction::MoveCursorLeft => {
                if state.view.cursor_position > 0 {
                    state.view.cursor_position = state.view.input_buffer[..state.view.cursor_position]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                }
                None
            }
            InputAction::MoveCursorRight => {
                if state.view.cursor_position < state.view.input_buffer.len() {
                    state.view.cursor_position = state.view.input_buffer[state.view.cursor_position..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| state.view.cursor_position + i)
                        .unwrap_or(state.view.input_buffer.len());
                }
                None
            }
            InputAction::MovePopupSelection(_) | InputAction::ConfirmPopup => {
                // Handled by runner
                None
            }
            InputAction::DeleteWordBackward => {
                // Find word boundary and delete
                let before = &state.view.input_buffer[..state.view.cursor_position];
                if let Some(pos) = before.rfind(char::is_whitespace) {
                    let delete_from = pos + 1;
                    let delete_to = state.view.cursor_position;
                    state.view.input_buffer.replace_range(delete_from..delete_to, "");
                    state.view.cursor_position = delete_from;
                } else {
                    state.view.input_buffer.clear();
                    state.view.cursor_position = 0;
                }
                None
            }
            InputAction::DeleteToLineStart => {
                state.view.input_buffer = state.view.input_buffer[state.view.cursor_position..].to_string();
                state.view.cursor_position = 0;
                None
            }
            InputAction::DeleteToLineEnd => {
                state.view.input_buffer.truncate(state.view.cursor_position);
                None
            }
            InputAction::MoveCursorToStart => {
                state.view.cursor_position = 0;
                None
            }
            InputAction::MoveCursorToEnd => {
                state.view.cursor_position = state.view.input_buffer.len();
                None
            }
            InputAction::MoveWordBackward => {
                let before = &state.view.input_buffer[..state.view.cursor_position];
                // Find the last space, then move to start of word after it
                if let Some(space_pos) = before.rfind(char::is_whitespace) {
                    // Find start of word after the space
                    let after_space = &state.view.input_buffer[space_pos..];
                    let word_start = after_space.find(|c: char| !c.is_whitespace()).unwrap_or(0);
                    state.view.cursor_position = space_pos + word_start;
                } else {
                    state.view.cursor_position = 0;
                }
                None
            }
            InputAction::MoveWordForward => {
                let after = &state.view.input_buffer[state.view.cursor_position..];
                // Find first whitespace
                if let Some((ws_offset, ws_char)) = after.char_indices().find(|(_, c)| c.is_whitespace()) {
                    // Move past the whitespace character
                    state.view.cursor_position += ws_offset + ws_char.len_utf8();
                    // Skip additional whitespace
                    while state.view.cursor_position < state.view.input_buffer.len() {
                        if let Some(c) = state.view.input_buffer[state.view.cursor_position..].chars().next() {
                            if c.is_whitespace() {
                                state.view.cursor_position += c.len_utf8();
                            } else {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                } else {
                    state.view.cursor_position = state.view.input_buffer.len();
                }
                None
            }
            InputAction::TransposeChars => {
                let len = state.view.input_buffer.chars().count();
                if len >= 2 && state.view.cursor_position > 0 {
                    let chars: Vec<char> = state.view.input_buffer.chars().collect();
                    // Convert byte position to character position
                    let char_pos = state.view.input_buffer[..state.view.cursor_position].chars().count();

                    let (i, j) = if char_pos >= len {
                        // At end: swap last two characters
                        (len - 2, len - 1)
                    } else {
                        // In middle: swap character before cursor with cursor
                        (char_pos - 1, char_pos)
                    };

                    let mut new_chars = chars;
                    new_chars.swap(i, j);
                    state.view.input_buffer = new_chars.into_iter().collect();

                    // Calculate new cursor position (byte index after swapped char)
                    let new_cursor = if char_pos < len {
                        state.view.input_buffer
                            .char_indices()
                            .nth(char_pos + 1)
                            .map(|(idx, _)| idx)
                            .unwrap_or(state.view.input_buffer.len())
                    } else {
                        state.view.input_buffer.len()
                    };
                    state.view.cursor_position = new_cursor;
                }
                None
            }
            InputAction::ScrollUp
            | InputAction::ScrollDown
            | InputAction::PageUp
            | InputAction::PageDown
            | InputAction::HalfPageUp
            | InputAction::HalfPageDown
            | InputAction::ScrollToTop
            | InputAction::ScrollToBottom
            | InputAction::HistoryPrev
            | InputAction::HistoryNext
            | InputAction::ToggleMouseCapture
            | InputAction::CopyMarkdown
            | InputAction::None => None,
        }
    }
}
