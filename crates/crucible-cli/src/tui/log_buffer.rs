// Ring buffer for log entries
//
// Implements a fixed-capacity circular buffer for storing log entries.
// When capacity is reached, oldest entries are evicted (FIFO).
//
// Design choice: VecDeque for O(1) push/pop operations.
// Memory bound: capacity * sizeof(LogEntry) - typically ~20 * ~200 bytes = 4KB

use crate::tui::events::LogEntry;
use std::collections::VecDeque;

/// Fixed-size ring buffer for log entries
///
/// Automatically evicts oldest entries when capacity is reached.
/// Thread-safe when wrapped in Arc<Mutex<_>>, but designed for single-threaded
/// access in the main TUI thread.
#[derive(Debug)]
pub struct LogBuffer {
    /// Internal storage (VecDeque for efficient front/back operations)
    entries: VecDeque<LogEntry>,

    /// Maximum number of entries to keep
    capacity: usize,
}

impl LogBuffer {
    /// Create a new log buffer with the given capacity
    ///
    /// # Arguments
    /// - `capacity`: Maximum number of log entries to retain
    ///
    /// # Example
    /// ```
    /// use crucible_cli::tui::LogBuffer;
    /// let buffer = LogBuffer::new(20); // Keep last 20 logs
    /// ```
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push a new log entry, evicting oldest if at capacity
    ///
    /// Time complexity: O(1) amortized
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front(); // Drop oldest
        }
        self.entries.push_back(entry);
    }

    /// Get the number of entries currently stored
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the buffer is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get iterator over entries (oldest to newest)
    pub fn entries(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }

    /// Get iterator over entries in reverse (newest to oldest)
    pub fn entries_rev(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter().rev()
    }

    /// Get the last N entries (most recent)
    ///
    /// If N > buffer size, returns all entries.
    pub fn last_n(&self, n: usize) -> impl Iterator<Item = &LogEntry> {
        let skip = self.entries.len().saturating_sub(n);
        self.entries.iter().skip(skip)
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Resize buffer capacity
    ///
    /// If new capacity is smaller than current size, oldest entries are dropped.
    pub fn resize(&mut self, new_capacity: usize) {
        while self.entries.len() > new_capacity {
            self.entries.pop_front();
        }
        self.capacity = new_capacity;
        self.entries.shrink_to(new_capacity);
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(20) // Default to 20 lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_log(message: &str) -> LogEntry {
        LogEntry::new(tracing::Level::INFO, "test", message)
    }

    #[test]
    fn test_basic_push() {
        let mut buffer = LogBuffer::new(3);
        buffer.push(make_log("A"));
        buffer.push(make_log("B"));

        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn test_capacity_enforcement() {
        let mut buffer = LogBuffer::new(3);
        buffer.push(make_log("A"));
        buffer.push(make_log("B"));
        buffer.push(make_log("C"));
        buffer.push(make_log("D")); // Should evict "A"

        assert_eq!(buffer.len(), 3);

        let messages: Vec<_> = buffer.entries().map(|e| e.message.as_str()).collect();
        assert_eq!(messages, vec!["B", "C", "D"]);
    }

    #[test]
    fn test_last_n() {
        let mut buffer = LogBuffer::new(5);
        for i in 0..5 {
            buffer.push(make_log(&format!("Entry {}", i)));
        }

        let last_2: Vec<_> = buffer.last_n(2).map(|e| e.message.as_str()).collect();
        assert_eq!(last_2, vec!["Entry 3", "Entry 4"]);
    }

    #[test]
    fn test_last_n_more_than_size() {
        let mut buffer = LogBuffer::new(3);
        buffer.push(make_log("A"));
        buffer.push(make_log("B"));

        let last_10: Vec<_> = buffer.last_n(10).map(|e| e.message.as_str()).collect();
        assert_eq!(last_10, vec!["A", "B"]); // Returns all available
    }

    #[test]
    fn test_clear() {
        let mut buffer = LogBuffer::new(3);
        buffer.push(make_log("A"));
        buffer.push(make_log("B"));

        buffer.clear();
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_resize_shrink() {
        let mut buffer = LogBuffer::new(5);
        for i in 0..5 {
            buffer.push(make_log(&format!("{}", i)));
        }

        buffer.resize(3); // Shrink to 3
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.capacity(), 3);

        let messages: Vec<_> = buffer.entries().map(|e| e.message.as_str()).collect();
        assert_eq!(messages, vec!["2", "3", "4"]); // Oldest 2 dropped
    }

    #[test]
    fn test_resize_grow() {
        let mut buffer = LogBuffer::new(3);
        buffer.push(make_log("A"));

        buffer.resize(10); // Grow
        assert_eq!(buffer.capacity(), 10);
        assert_eq!(buffer.len(), 1); // Existing entry preserved
    }

    #[test]
    fn test_iteration_order() {
        let mut buffer = LogBuffer::new(3);
        buffer.push(make_log("First"));
        buffer.push(make_log("Second"));
        buffer.push(make_log("Third"));

        // Forward iteration (oldest to newest)
        let forward: Vec<_> = buffer.entries().map(|e| e.message.as_str()).collect();
        assert_eq!(forward, vec!["First", "Second", "Third"]);

        // Reverse iteration (newest to oldest)
        let reverse: Vec<_> = buffer.entries_rev().map(|e| e.message.as_str()).collect();
        assert_eq!(reverse, vec!["Third", "Second", "First"]);
    }
}
