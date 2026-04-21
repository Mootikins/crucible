//! Mock storage implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Statistics for mock storage operations
///
/// This structure tracks all operations performed on the mock storage,
/// enabling test assertions about storage usage patterns.
#[derive(Debug, Clone, Default)]
pub struct MockStorageStats {
    /// Number of store_block calls
    pub store_count: usize,
    /// Number of get_block calls
    pub get_count: usize,
    /// Number of block_exists calls
    pub exists_count: usize,
    /// Number of delete_block calls
    pub delete_count: usize,
    /// Total bytes stored
    pub total_bytes_stored: u64,
    /// Total bytes retrieved
    pub total_bytes_retrieved: u64,
}

/// Internal state for mock storage
#[derive(Debug, Default)]
struct MockStorageState {
    /// Stored blocks (hash -> data)
    blocks: HashMap<String, Vec<u8>>,
    /// Operation statistics
    stats: MockStorageStats,
    /// Whether to simulate errors
    simulate_errors: bool,
    /// Error message to return when simulating errors
    error_message: String,
}

/// Mock storage implementation for testing
///
/// This provides an in-memory storage implementation that tracks all operations
/// and supports error injection for testing error handling paths.
///
/// # Features
///
/// - **In-Memory**: All data stored in memory, no I/O overhead
/// - **Observable**: Tracks all operations via statistics
/// - **Error Injection**: Can simulate storage failures
/// - **Thread-Safe**: Uses Arc<Mutex<>> for concurrent access
///
/// # Examples
///
/// ```rust
/// use crate::test_support::mocks::MockStorage;
///
/// let storage = MockStorage::new();
///
/// // Check initial state
/// assert_eq!(storage.block_count(), 0);
///
/// // Error injection
/// storage.set_simulate_errors(true, "Storage full");
///
/// // Reset for normal operation
/// storage.set_simulate_errors(false, "");
/// ```
#[derive(Debug, Clone)]
pub struct MockStorage {
    state: Arc<Mutex<MockStorageState>>,
}

impl MockStorage {
    /// Create a new mock storage instance
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockStorageState::default())),
        }
    }

    /// Get operation statistics
    pub fn stats(&self) -> MockStorageStats {
        self.state.lock().unwrap().stats.clone()
    }

    /// Reset all stored data and statistics
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.blocks.clear();
        state.stats = MockStorageStats::default();
        state.simulate_errors = false;
        state.error_message.clear();
    }

    /// Configure error simulation
    ///
    /// # Arguments
    ///
    /// * `enabled` - Whether to simulate errors
    /// * `message` - Error message to return
    pub fn set_simulate_errors(&self, enabled: bool, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.simulate_errors = enabled;
        state.error_message = message.to_string();
    }

    /// Get the number of stored blocks
    pub fn block_count(&self) -> usize {
        self.state.lock().unwrap().blocks.len()
    }
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}
