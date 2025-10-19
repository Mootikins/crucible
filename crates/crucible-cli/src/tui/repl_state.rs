// REPL state management
//
// Manages the REPL input buffer, cursor position, command history, and execution state.
// Designed for single-threaded access in the main TUI thread.

use std::collections::VecDeque;

/// REPL execution state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionState {
    /// Ready for input
    Idle,
    /// Currently executing a command
    Executing,
}

/// REPL input and history state
///
/// Manages:
/// - Current input buffer with cursor position
/// - Command history with navigation
/// - Execution state tracking
#[derive(Debug)]
pub struct ReplState {
    /// Current input buffer
    input: String,

    /// Cursor position (byte offset into input)
    cursor: usize,

    /// Command history (ring buffer)
    history: VecDeque<String>,

    /// History capacity
    history_capacity: usize,

    /// Current position in history (None = at prompt)
    history_index: Option<usize>,

    /// Execution state
    execution_state: ExecutionState,
}

impl ReplState {
    /// Create a new REPL state
    ///
    /// # Arguments
    /// - `history_capacity`: Maximum number of commands to keep in history
    pub fn new(history_capacity: usize) -> Self {
        Self {
            input: String::new(),
            cursor: 0,
            history: VecDeque::with_capacity(history_capacity),
            history_capacity,
            history_index: None,
            execution_state: ExecutionState::Idle,
        }
    }

    // --- Input Management ---

    /// Get current input buffer
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Get cursor position
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Insert character at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            // Find previous character boundary
            let mut idx = self.cursor - 1;
            while !self.input.is_char_boundary(idx) && idx > 0 {
                idx -= 1;
            }
            self.input.remove(idx);
            self.cursor = idx;
        }
    }

    /// Delete character at cursor (delete key)
    pub fn delete_char_forward(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
        }
    }

    /// Move cursor left
    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            let mut idx = self.cursor - 1;
            while !self.input.is_char_boundary(idx) && idx > 0 {
                idx -= 1;
            }
            self.cursor = idx;
        }
    }

    /// Move cursor right
    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            let mut idx = self.cursor + 1;
            while !self.input.is_char_boundary(idx) && idx < self.input.len() {
                idx += 1;
            }
            self.cursor = idx;
        }
    }

    /// Move cursor to start of line
    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end of line
    pub fn move_cursor_end(&mut self) {
        self.cursor = self.input.len();
    }

    /// Clear input buffer
    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.history_index = None;
    }

    /// Set input buffer (used for history navigation)
    pub fn set_input(&mut self, input: String) {
        self.cursor = input.len();
        self.input = input;
    }

    // --- History Management ---

    /// Add command to history
    ///
    /// Empty commands and duplicates of the last command are not added.
    pub fn add_history(&mut self, command: impl Into<String>) {
        let command = command.into();

        // Skip empty commands
        if command.trim().is_empty() {
            return;
        }

        // Skip duplicates of last command
        if self.history.back().map(|s| s.as_str()) == Some(command.as_str()) {
            return;
        }

        // Add to history, evicting oldest if at capacity
        if self.history.len() >= self.history_capacity {
            self.history.pop_front();
        }
        self.history.push_back(command);
    }

    /// Navigate to previous command in history (Up arrow)
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // First up arrow - go to most recent
                self.history_index = Some(self.history.len() - 1);
                if let Some(cmd) = self.history.back() {
                    self.set_input(cmd.clone());
                }
            }
            Some(idx) if idx > 0 => {
                // Go back in history
                self.history_index = Some(idx - 1);
                if let Some(cmd) = self.history.get(idx - 1) {
                    self.set_input(cmd.clone());
                }
            }
            _ => {
                // Already at oldest
            }
        }
    }

    /// Navigate to next command in history (Down arrow)
    pub fn history_next(&mut self) {
        match self.history_index {
            Some(idx) if idx < self.history.len() - 1 => {
                // Go forward in history
                self.history_index = Some(idx + 1);
                if let Some(cmd) = self.history.get(idx + 1) {
                    self.set_input(cmd.clone());
                }
            }
            Some(_) => {
                // At newest - return to empty prompt
                self.history_index = None;
                self.clear();
            }
            None => {
                // Already at prompt
            }
        }
    }

    /// Get history entries (oldest to newest)
    pub fn history(&self) -> impl Iterator<Item = &String> {
        self.history.iter()
    }

    // --- Execution State ---

    /// Get execution state
    pub fn execution_state(&self) -> ExecutionState {
        self.execution_state
    }

    /// Set execution state
    pub fn set_execution_state(&mut self, state: ExecutionState) {
        self.execution_state = state;
    }

    /// Check if ready for input
    pub fn is_idle(&self) -> bool {
        self.execution_state == ExecutionState::Idle
    }

    /// Check if executing
    pub fn is_executing(&self) -> bool {
        self.execution_state == ExecutionState::Executing
    }

    /// Submit current input (returns command and clears buffer)
    pub fn submit(&mut self) -> String {
        let command = self.input.clone();
        self.add_history(&command);
        self.clear();
        self.set_execution_state(ExecutionState::Executing);
        command
    }
}

impl Default for ReplState {
    fn default() -> Self {
        Self::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_input() {
        let mut repl = ReplState::new(10);

        repl.insert_char('h');
        repl.insert_char('i');
        assert_eq!(repl.input(), "hi");
        assert_eq!(repl.cursor(), 2);
    }

    #[test]
    fn test_backspace() {
        let mut repl = ReplState::new(10);
        repl.insert_char('a');
        repl.insert_char('b');
        repl.insert_char('c');

        repl.delete_char(); // Delete 'c'
        assert_eq!(repl.input(), "ab");
        assert_eq!(repl.cursor(), 2);
    }

    #[test]
    fn test_cursor_movement() {
        let mut repl = ReplState::new(10);
        repl.set_input("hello".to_string());

        repl.move_cursor_home();
        assert_eq!(repl.cursor(), 0);

        repl.move_cursor_right();
        assert_eq!(repl.cursor(), 1);

        repl.move_cursor_end();
        assert_eq!(repl.cursor(), 5);

        repl.move_cursor_left();
        assert_eq!(repl.cursor(), 4);
    }

    #[test]
    fn test_history_add() {
        let mut repl = ReplState::new(3);

        repl.add_history("first");
        repl.add_history("second");
        repl.add_history("third");

        assert_eq!(repl.history().count(), 3);
    }

    #[test]
    fn test_history_capacity() {
        let mut repl = ReplState::new(2);

        repl.add_history("first");
        repl.add_history("second");
        repl.add_history("third"); // Should evict "first"

        let history: Vec<_> = repl.history().cloned().collect();
        assert_eq!(history, vec!["second", "third"]);
    }

    #[test]
    fn test_history_navigation() {
        let mut repl = ReplState::new(10);

        repl.add_history("SELECT * FROM notes");
        repl.add_history(":help");

        // Up arrow - should show :help
        repl.history_prev();
        assert_eq!(repl.input(), ":help");

        // Up arrow again - should show SELECT
        repl.history_prev();
        assert_eq!(repl.input(), "SELECT * FROM notes");

        // Down arrow - should show :help
        repl.history_next();
        assert_eq!(repl.input(), ":help");

        // Down arrow - should clear (back to prompt)
        repl.history_next();
        assert_eq!(repl.input(), "");
    }

    #[test]
    fn test_skip_empty_commands() {
        let mut repl = ReplState::new(10);

        repl.add_history("   ");
        repl.add_history("");

        assert_eq!(repl.history().count(), 0);
    }

    #[test]
    fn test_skip_duplicate_last() {
        let mut repl = ReplState::new(10);

        repl.add_history("SELECT");
        repl.add_history("SELECT"); // Duplicate - should be skipped
        repl.add_history("INSERT");
        repl.add_history("SELECT"); // Not duplicate of last - should be added

        let history: Vec<_> = repl.history().cloned().collect();
        assert_eq!(history, vec!["SELECT", "INSERT", "SELECT"]);
    }

    #[test]
    fn test_submit() {
        let mut repl = ReplState::new(10);

        repl.set_input("test command".to_string());
        let command = repl.submit();

        assert_eq!(command, "test command");
        assert_eq!(repl.input(), ""); // Cleared
        assert_eq!(repl.history().count(), 1); // Added to history
        assert!(repl.is_executing()); // State changed
    }

    #[test]
    fn test_utf8_handling() {
        let mut repl = ReplState::new(10);

        // Insert multi-byte characters
        repl.insert_char('你');
        repl.insert_char('好');
        assert_eq!(repl.input(), "你好");

        // Cursor should be at correct position
        assert_eq!(repl.cursor(), 6); // 3 bytes * 2 chars

        // Backspace should remove whole character
        repl.delete_char();
        assert_eq!(repl.input(), "你");
        assert_eq!(repl.cursor(), 3);
    }
}
