//! Mock content hasher.

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::hashing::algorithm::HashingAlgorithm;
use crate::traits::change_detection::ContentHasher;
use crate::types::hashing::{
    BlockHash, BlockHashInfo, FileHash, FileHashInfo, HashAlgorithm, HashError,
};

use super::hashing::MockHashingAlgorithm;

/// Internal state for mock content hasher
#[derive(Debug, Default)]
struct MockContentHasherState {
    /// Configured file hashes (path -> hash)
    file_hashes: HashMap<String, Vec<u8>>,
    /// Configured block hashes (content -> hash)
    block_hashes: HashMap<String, Vec<u8>>,
    /// Operation counts
    hash_file_count: usize,
    hash_block_count: usize,
    /// Whether to simulate errors
    simulate_errors: bool,
    /// Error message for simulated errors
    error_message: String,
}

/// Mock content hasher implementation for testing
///
/// This provides a configurable content hasher that can return predetermined
/// hash values for testing purposes. It supports:
///
/// - **Configured Responses**: Set specific hashes for paths/content
/// - **Deterministic**: Falls back to mock algorithm for unconfigured inputs
/// - **Error Injection**: Simulate hashing failures
/// - **Observable**: Track operation counts
///
/// # Examples
///
/// ```rust
/// use crate::test_support::mocks::MockContentHasher;
/// use crate::traits::change_detection::ContentHasher;
/// use std::path::Path;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let hasher = MockContentHasher::new();
///
/// // Configure specific hash for a path
/// hasher.set_file_hash("test.md", vec![1u8; 32]);
///
/// let hash = hasher.hash_file(Path::new("test.md")).await?;
/// assert_eq!(hash.as_bytes(), &vec![1u8; 32]);
///
/// // Unconfigured paths use deterministic fallback
/// let hash2 = hasher.hash_file(Path::new("other.md")).await?;
/// assert_eq!(hash2.as_bytes().len(), 32);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct MockContentHasher {
    state: Arc<Mutex<MockContentHasherState>>,
    algorithm: MockHashingAlgorithm,
}

impl MockContentHasher {
    /// Create a new mock content hasher
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockContentHasherState::default())),
            algorithm: MockHashingAlgorithm::new(),
        }
    }

    /// Set a specific hash for a file path
    pub fn set_file_hash(&self, path: &str, hash: Vec<u8>) {
        let mut state = self.state.lock().unwrap();
        state.file_hashes.insert(path.to_string(), hash);
    }

    /// Set a specific hash for content
    pub fn set_block_hash(&self, content: &str, hash: Vec<u8>) {
        let mut state = self.state.lock().unwrap();
        state.block_hashes.insert(content.to_string(), hash);
    }

    /// Configure error simulation
    pub fn set_simulate_errors(&self, enabled: bool, message: &str) {
        let mut state = self.state.lock().unwrap();
        state.simulate_errors = enabled;
        state.error_message = message.to_string();
    }

    /// Get operation statistics
    pub fn operation_counts(&self) -> (usize, usize) {
        let state = self.state.lock().unwrap();
        (state.hash_file_count, state.hash_block_count)
    }

    /// Reset all configured hashes and statistics
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.file_hashes.clear();
        state.block_hashes.clear();
        state.hash_file_count = 0;
        state.hash_block_count = 0;
        state.simulate_errors = false;
        state.error_message.clear();
    }
}

impl Default for MockContentHasher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentHasher for MockContentHasher {
    fn algorithm(&self) -> HashAlgorithm {
        HashAlgorithm::Blake3 // Mock as BLAKE3 for compatibility
    }

    async fn hash_file(&self, path: &Path) -> Result<FileHash, HashError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        state.hash_file_count += 1;

        let path_str = path.to_string_lossy().to_string();

        // Use configured hash if available, otherwise use deterministic fallback
        let hash_bytes = state
            .file_hashes
            .get(&path_str)
            .cloned()
            .unwrap_or_else(|| self.algorithm.hash(path_str.as_bytes()));

        // Ensure hash is exactly 32 bytes
        let mut hash = [0u8; 32];
        let copy_len = hash_bytes.len().min(32);
        hash[..copy_len].copy_from_slice(&hash_bytes[..copy_len]);

        Ok(FileHash::new(hash))
    }

    async fn hash_files_batch(
        &self,
        paths: &[std::path::PathBuf],
    ) -> Result<Vec<FileHash>, HashError> {
        let mut results = Vec::with_capacity(paths.len());
        for path in paths {
            results.push(self.hash_file(path).await?);
        }
        Ok(results)
    }

    async fn hash_block(&self, content: &str) -> Result<BlockHash, HashError> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(HashError::IoError {
                error: state.error_message.clone(),
            });
        }

        state.hash_block_count += 1;

        // Use configured hash if available, otherwise use deterministic fallback
        let hash_bytes = state
            .block_hashes
            .get(content)
            .cloned()
            .unwrap_or_else(|| self.algorithm.hash(content.as_bytes()));

        // Ensure hash is exactly 32 bytes
        let mut hash = [0u8; 32];
        let copy_len = hash_bytes.len().min(32);
        hash[..copy_len].copy_from_slice(&hash_bytes[..copy_len]);

        Ok(BlockHash::new(hash))
    }

    async fn hash_blocks_batch(&self, contents: &[String]) -> Result<Vec<BlockHash>, HashError> {
        let mut results = Vec::with_capacity(contents.len());
        for content in contents {
            results.push(self.hash_block(content).await?);
        }
        Ok(results)
    }

    async fn hash_file_info(
        &self,
        path: &Path,
        relative_path: String,
    ) -> Result<FileHashInfo, HashError> {
        let hash = self.hash_file(path).await?;

        // Mock file size and modification time
        let size = 1024u64; // Default mock size
        let modified = SystemTime::now();

        Ok(FileHashInfo::new(
            hash,
            size,
            modified,
            self.algorithm(),
            relative_path,
        ))
    }

    async fn hash_block_info(
        &self,
        content: &str,
        block_type: String,
        start_offset: usize,
        end_offset: usize,
    ) -> Result<BlockHashInfo, HashError> {
        let hash = self.hash_block(content).await?;

        Ok(BlockHashInfo::new(
            hash,
            block_type,
            start_offset,
            end_offset,
            self.algorithm(),
        ))
    }

    async fn verify_file_hash(
        &self,
        path: &Path,
        expected_hash: &FileHash,
    ) -> Result<bool, HashError> {
        let actual_hash = self.hash_file(path).await?;
        Ok(actual_hash == *expected_hash)
    }

    async fn verify_block_hash(
        &self,
        content: &str,
        expected_hash: &BlockHash,
    ) -> Result<bool, HashError> {
        let actual_hash = self.hash_block(content).await?;
        Ok(actual_hash == *expected_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_content_hasher() {
        let hasher = MockContentHasher::new();

        // Configured hash
        hasher.set_file_hash("test.md", vec![1u8; 32]);
        let hash = hasher.hash_file(Path::new("test.md")).await.unwrap();
        assert_eq!(hash.as_bytes(), &[1u8; 32]);

        // Unconfigured hash uses fallback
        let hash2 = hasher.hash_file(Path::new("other.md")).await.unwrap();
        assert_eq!(hash2.as_bytes().len(), 32);

        // Operation tracking
        let (file_count, block_count) = hasher.operation_counts();
        assert_eq!(file_count, 2);
        assert_eq!(block_count, 0);
    }

    #[tokio::test]
    async fn test_mock_content_hasher_blocks() {
        let hasher = MockContentHasher::new();

        hasher.set_block_hash("content", vec![2u8; 32]);
        let hash = hasher.hash_block("content").await.unwrap();
        assert_eq!(hash.as_bytes(), &[2u8; 32]);

        let (_, block_count) = hasher.operation_counts();
        assert_eq!(block_count, 1);
    }
}
