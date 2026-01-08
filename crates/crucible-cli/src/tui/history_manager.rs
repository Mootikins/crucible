//! History subsystem manager
//!
//! Manages command history navigation and state.

/// Manages command history navigation
#[derive(Debug, Clone, Default)]
pub struct HistoryManager {
    /// History entries
    entries: Vec<String>,
    /// Current position in history
    index: usize,
    /// Saved input before navigating history
    saved_input: String,
}

impl HistoryManager {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            index: 0,
            saved_input: String::new(),
        }
    }

    /// Add a new entry to history
    pub fn push(&mut self, entry: String) {
        self.entries.push(entry);
        self.index = self.entries.len();
    }

    /// Navigate to previous entry
    pub fn prev(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        // Save current input on first navigation
        if self.index == self.entries.len() {
            self.saved_input = current_input.to_string();
        }

        if self.index > 0 {
            self.index -= 1;
            self.entries.get(self.index).map(|s| s.as_str())
        } else {
            None
        }
    }

    /// Navigate to next entry
    pub fn next(&mut self) -> Option<&str> {
        if self.index < self.entries.len() {
            self.index += 1;
            if self.index < self.entries.len() {
                self.entries.get(self.index).map(|s| s.as_str())
            } else {
                // Return saved input when navigating past end
                Some(self.saved_input.as_str())
            }
        } else {
            None
        }
    }

    /// Get saved input (restored when navigating past end)
    pub fn saved_input(&self) -> &str {
        &self.saved_input
    }

    /// Reset to end of history
    pub fn reset(&mut self) {
        self.index = self.entries.len();
    }

    /// Get current history index
    pub fn index(&self) -> usize {
        self.index
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the last entry (for deduplication check)
    pub fn last(&self) -> Option<&str> {
        self.entries.last().map(|s| s.as_str())
    }

    /// Get entry by index
    pub fn get(&self, index: usize) -> Option<&str> {
        self.entries.get(index).map(|s| s.as_str())
    }
}
