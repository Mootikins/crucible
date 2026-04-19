//! Mock hash lookup storage.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::traits::change_detection::{
    BatchLookupConfig, HashLookupResult, HashLookupStorage, StoredHash,
};
use crate::types::hashing::{FileHash, FileHashInfo, HashAlgorithm, HashError};

/// Internal state for mock hash lookup storage
#[derive(Debug, Default)]
struct MockHashLookupStorageState {
    /// Stored hashes (relative_path -> StoredHash)
    stored_hashes: HashMap<String, StoredHash>,
    /// Operation counts
    lookup_count: usize,
    batch_lookup_count: usize,
    store_count: usize,
    /// Error simulation
    simulate_errors: bool,
    error_message: String,
}

/// Mock hash lookup storage for testing
///
/// Provides an in-memory implementation of hash lookup storage with
/// full operation tracking and error injection capabilities.
///
/// # Examples
///
/// ```rust
/// use crucible_core::test_support::mocks::MockHashLookupStorage;
/// use crucible_core::traits::change_detection::HashLookupStorage;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let storage = MockHashLookupStorage::new();
///
/// // Query returns None for missing files
/// let result = storage.lookup_file_hash("test.md").await?;
/// assert!(result.is_none());
///
/// // Can verify operation was tracked
/// assert_eq!(storage.operation_counts().0, 1);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MockHashLookupStorage {
    state: Arc<Mutex<MockHashLookupStorageState>>,
}

impl MockHashLookupStorage {
    /// Create a new mock hash lookup storage
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockHashLookupStorageState::default())),
        }
    }

    /// Add a stored hash for testing
    pub fn add_stored_hash(&self, path: String, stored: StoredHash) {
        let mut state = self.state.lock().unwrap();
        state.stored_hashes.insert(path, stored);
    }

    /// Configure error simulation
    pub fn set_simulate_errors(&self, enabled: bool, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.simulate_errors = enabled;
        state.error_message = message.to_string();
    }

    /// Get operation counts: (lookups, batch_lookups, stores)
    pub fn operation_counts(&self) -> (usize, usize, usize) {
        let state = self.state.lock().unwrap();
        (
            state.lookup_count,
            state.batch_lookup_count,
            state.store_count,
        )
    }

    /// Reset all data and statistics
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.stored_hashes.clear();
        state.lookup_count = 0;
        state.batch_lookup_count = 0;
        state.store_count = 0;
        state.simulate_errors = false;
        state.error_message.clear();
    }
}

impl Default for MockHashLookupStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HashLookupStorage for MockHashLookupStorage {
    async fn lookup_file_hash(&self, relative_path: &str) -> Result<Option<StoredHash>, HashError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        state.lookup_count += 1;
        Ok(state.stored_hashes.get(relative_path).cloned())
    }

    async fn lookup_file_hashes_batch(
        &self,
        relative_paths: &[String],
        _config: Option<BatchLookupConfig>,
    ) -> Result<HashLookupResult, HashError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        state.batch_lookup_count += 1;

        let mut result = HashLookupResult::new();
        result.total_queried = relative_paths.len();
        result.database_round_trips = 1;

        for path in relative_paths {
            match state.stored_hashes.get(path) {
                Some(stored) => {
                    result.found_files.insert(path.clone(), stored.clone());
                }
                None => {
                    result.missing_files.push(path.clone());
                }
            }
        }

        Ok(result)
    }

    async fn lookup_files_by_content_hashes(
        &self,
        content_hashes: &[FileHash],
    ) -> Result<HashMap<String, Vec<StoredHash>>, HashError> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        let mut result: HashMap<String, Vec<StoredHash>> = HashMap::new();

        for stored in state.stored_hashes.values() {
            if content_hashes.contains(&stored.content_hash) {
                result
                    .entry(stored.content_hash.to_hex())
                    .or_default()
                    .push(stored.clone());
            }
        }

        Ok(result)
    }

    async fn lookup_changed_files_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<StoredHash>, HashError> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        let mut results: Vec<StoredHash> = state
            .stored_hashes
            .values()
            .filter(|stored| stored.modified_at > since)
            .cloned()
            .collect();

        if let Some(limit) = limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn check_file_needs_update(
        &self,
        relative_path: &str,
        new_hash: &FileHash,
    ) -> Result<bool, HashError> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        match state.stored_hashes.get(relative_path) {
            Some(stored) => Ok(stored.content_hash != *new_hash),
            None => Ok(true), // File doesn't exist, needs update
        }
    }

    async fn store_hashes(&self, files: &[FileHashInfo]) -> Result<(), HashError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        state.store_count += files.len();

        for file in files {
            let stored = StoredHash::new(
                format!("mock:{}", file.relative_path.replace('/', "_")),
                file.relative_path.clone(),
                file.content_hash,
                file.size,
                chrono::Utc::now(),
            );
            state
                .stored_hashes
                .insert(file.relative_path.clone(), stored);
        }

        Ok(())
    }

    async fn remove_hashes(&self, paths: &[String]) -> Result<(), HashError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        for path in paths {
            state.stored_hashes.remove(path);
        }

        Ok(())
    }

    async fn get_all_hashes(&self) -> Result<HashMap<String, FileHashInfo>, HashError> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        let mut result = HashMap::new();
        for (path, stored) in &state.stored_hashes {
            result.insert(
                path.clone(),
                stored.to_file_hash_info(HashAlgorithm::Blake3),
            );
        }

        Ok(result)
    }

    async fn clear_all_hashes(&self) -> Result<(), HashError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        state.stored_hashes.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_hash_lookup_storage() {
        let storage = MockHashLookupStorage::new();

        // Empty lookup
        let result = storage.lookup_file_hash("test.md").await.unwrap();
        assert!(result.is_none());

        // Add hash
        let stored = StoredHash::new(
            "mock:test_md".to_string(),
            "test.md".to_string(),
            FileHash::new([1u8; 32]),
            1024,
            chrono::Utc::now(),
        );
        storage.add_stored_hash("test.md".to_string(), stored.clone());

        // Lookup should find it
        let result = storage.lookup_file_hash("test.md").await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().relative_path, "test.md");

        // Check operation counts
        let (lookups, _, _) = storage.operation_counts();
        assert_eq!(lookups, 2);
    }

    #[tokio::test]
    async fn test_mock_hash_lookup_batch() {
        let storage = MockHashLookupStorage::new();

        // Add some hashes
        for i in 1..=3 {
            let stored = StoredHash::new(
                format!("mock:file{}_md", i),
                format!("file{}.md", i),
                FileHash::new([i as u8; 32]),
                1024,
                chrono::Utc::now(),
            );
            storage.add_stored_hash(format!("file{}.md", i), stored);
        }

        // Batch lookup
        let paths = vec![
            "file1.md".to_string(),
            "file2.md".to_string(),
            "missing.md".to_string(),
        ];
        let result = storage
            .lookup_file_hashes_batch(&paths, None)
            .await
            .unwrap();

        assert_eq!(result.found_files.len(), 2);
        assert_eq!(result.missing_files.len(), 1);
        assert_eq!(result.total_queried, 3);
    }
}
