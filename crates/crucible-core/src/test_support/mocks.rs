//! Mock Implementations for Testing
//!
//! This module provides comprehensive mock implementations of core traits for testing purposes.
//! These mocks are designed to be:
//!
//! - **Deterministic**: Always produce the same results for the same inputs
//! - **Fast**: In-memory operations with no I/O overhead
//! - **Configurable**: Support error injection and custom behaviors
//! - **Observable**: Track all operations for test assertions
//! - **Isolated**: No external dependencies or side effects
//!
//! # Design Principles
//!
//! - **Simplicity**: Straightforward implementations without production complexity
//! - **Predictability**: Deterministic behavior for reliable test results
//! - **Observability**: Call tracking for verifying test expectations
//! - **Error Testing**: Support for simulating various error conditions
//!
//! # Examples
//!
//! ## Mock Hashing Algorithm
//!
//! ```rust
//! use crucible_core::test_support::mocks::MockHashingAlgorithm;
//! use crucible_core::hashing::algorithm::HashingAlgorithm;
//!
//! let hasher = MockHashingAlgorithm::new();
//! let hash = hasher.hash(b"test data");
//!
//! // Mock hasher produces deterministic, simple hashes
//! assert_eq!(hash.len(), 32);
//! assert_eq!(hasher.algorithm_name(), "MockHash");
//! ```
//!
//! ## Mock Storage
//!
//! ```rust
//! use crucible_core::test_support::mocks::MockStorage;
//! use crucible_core::storage::traits::BlockOperations;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let storage = MockStorage::new();
//!
//! // Store and retrieve blocks
//! storage.store_block("hash1", b"data").await?;
//! let data = storage.get_block("hash1").await?;
//! assert_eq!(data, Some(b"data".to_vec()));
//!
//! // Verify operations were called
//! let stats = storage.stats();
//! assert_eq!(stats.store_count, 1);
//! assert_eq!(stats.get_count, 1);
//! # Ok(())
//! # }
//! ```
//!
//! ## Mock Content Hasher
//!
//! ```rust
//! use crucible_core::test_support::mocks::MockContentHasher;
//! use crucible_core::traits::change_detection::ContentHasher;
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let hasher = MockContentHasher::new();
//!
//! // Configure to return specific hash for a path
//! hasher.set_file_hash("test.md", vec![1u8; 32]);
//!
//! // Hash file operations use configured values
//! let hash = hasher.hash_file(Path::new("test.md")).await?;
//! assert_eq!(hash.as_bytes().len(), 32);
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::hashing::algorithm::HashingAlgorithm;
use crate::storage::traits::{
    BlockOperations, StorageBackend, StorageManagement, StorageStats, TreeOperations,
};
use crate::storage::{MerkleTree, StorageError, StorageResult};
use crate::traits::change_detection::{
    BatchLookupConfig, ChangeDetectionMetrics, ChangeDetectionResult, ChangeDetector, ChangeSet,
    ChangeStatistics, ContentHasher, HashLookupResult, HashLookupStorage, StoredHash,
};
use crate::types::hashing::{
    BlockHash, BlockHashInfo, FileHash, FileHashInfo, HashAlgorithm, HashError,
};

// ============================================================================
// Mock Hashing Algorithm
// ============================================================================

/// Mock hashing algorithm for testing
///
/// This implementation provides a simple, deterministic hashing algorithm
/// suitable for testing. It uses a simple checksum algorithm that is:
///
/// - **Deterministic**: Same input always produces same output
/// - **Fast**: Simple arithmetic operations
/// - **Predictable**: Easy to reason about in tests
/// - **NOT Cryptographic**: Do not use in production
///
/// # Hash Format
///
/// The mock hash is a 32-byte array where:
/// - First 8 bytes: Sum of all input bytes
/// - Next 8 bytes: XOR of all input bytes
/// - Next 8 bytes: Length of input data
/// - Last 8 bytes: Constant pattern (0xAA)
///
/// This makes hashes easy to inspect and verify in tests while maintaining
/// the same interface as production hash algorithms.
///
/// # Examples
///
/// ```rust
/// use crucible_core::test_support::mocks::MockHashingAlgorithm;
/// use crucible_core::hashing::algorithm::HashingAlgorithm;
///
/// let hasher = MockHashingAlgorithm::new();
///
/// // Same input produces same hash
/// let hash1 = hasher.hash(b"test");
/// let hash2 = hasher.hash(b"test");
/// assert_eq!(hash1, hash2);
///
/// // Different inputs produce different hashes
/// let hash3 = hasher.hash(b"different");
/// assert_ne!(hash1, hash3);
///
/// // Empty input is valid
/// let empty = hasher.hash(b"");
/// assert_eq!(empty.len(), 32);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct MockHashingAlgorithm;

impl MockHashingAlgorithm {
    /// Create a new mock hashing algorithm
    pub fn new() -> Self {
        Self
    }

    /// Internal: Compute a simple deterministic hash
    fn compute_hash(data: &[u8]) -> Vec<u8> {
        let mut hash = Vec::with_capacity(32);

        // First 8 bytes: sum of all bytes
        let sum: u64 = data.iter().map(|&b| b as u64).sum();
        hash.extend_from_slice(&sum.to_le_bytes());

        // Next 8 bytes: XOR of all bytes
        let xor: u8 = data.iter().fold(0u8, |acc, &b| acc ^ b);
        hash.extend_from_slice(&(xor as u64).to_le_bytes());

        // Next 8 bytes: length
        hash.extend_from_slice(&(data.len() as u64).to_le_bytes());

        // Last 8 bytes: constant pattern for easy recognition
        hash.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11]);

        hash
    }
}

impl HashingAlgorithm for MockHashingAlgorithm {
    fn hash(&self, data: &[u8]) -> Vec<u8> {
        Self::compute_hash(data)
    }

    fn hash_nodes(&self, left: &[u8], right: &[u8]) -> Vec<u8> {
        let mut combined = Vec::with_capacity(left.len() + right.len());
        combined.extend_from_slice(left);
        combined.extend_from_slice(right);
        Self::compute_hash(&combined)
    }

    fn algorithm_name(&self) -> &'static str {
        "MockHash"
    }

    fn hash_length(&self) -> usize {
        32
    }
}

// ============================================================================
// Mock Storage
// ============================================================================

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
    /// Number of store_tree calls
    pub store_tree_count: usize,
    /// Number of get_tree calls
    pub get_tree_count: usize,
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
    /// Stored Merkle trees (root_hash -> tree)
    trees: HashMap<String, MerkleTree>,
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
/// - **Complete**: Implements all storage traits
///
/// # Examples
///
/// ```rust
/// use crucible_core::test_support::mocks::MockStorage;
/// use crucible_core::storage::traits::BlockOperations;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let storage = MockStorage::new();
///
/// // Normal operations
/// storage.store_block("hash1", b"data").await?;
/// assert!(storage.block_exists("hash1").await?);
///
/// // Error injection
/// storage.set_simulate_errors(true, "Storage full");
/// let result = storage.store_block("hash2", b"data").await;
/// assert!(result.is_err());
///
/// // Reset for normal operation
/// storage.set_simulate_errors(false, "");
/// # Ok(())
/// # }
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
        state.trees.clear();
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

    /// Get the number of stored trees
    pub fn tree_count(&self) -> usize {
        self.state.lock().unwrap().trees.len()
    }
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BlockOperations for MockStorage {
    async fn store_block(&self, hash: &str, data: &[u8]) -> StorageResult<()> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        state.stats.store_count += 1;
        state.stats.total_bytes_stored += data.len() as u64;
        state.blocks.insert(hash.to_string(), data.to_vec());

        Ok(())
    }

    async fn get_block(&self, hash: &str) -> StorageResult<Option<Vec<u8>>> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        state.stats.get_count += 1;

        if let Some(data) = state.blocks.get(hash) {
            let data_len = data.len() as u64;
            let data_clone = data.clone();
            state.stats.total_bytes_retrieved += data_len;
            Ok(Some(data_clone))
        } else {
            Ok(None)
        }
    }

    async fn block_exists(&self, hash: &str) -> StorageResult<bool> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        state.stats.exists_count += 1;
        Ok(state.blocks.contains_key(hash))
    }

    async fn delete_block(&self, hash: &str) -> StorageResult<bool> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        state.stats.delete_count += 1;
        Ok(state.blocks.remove(hash).is_some())
    }
}

#[async_trait]
impl TreeOperations for MockStorage {
    async fn store_tree(&self, root_hash: &str, tree: &MerkleTree) -> StorageResult<()> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        state.stats.store_tree_count += 1;
        state.trees.insert(root_hash.to_string(), tree.clone());

        Ok(())
    }

    async fn get_tree(&self, root_hash: &str) -> StorageResult<Option<MerkleTree>> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        state.stats.get_tree_count += 1;
        Ok(state.trees.get(root_hash).cloned())
    }

    async fn tree_exists(&self, root_hash: &str) -> StorageResult<bool> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        Ok(state.trees.contains_key(root_hash))
    }

    async fn delete_tree(&self, root_hash: &str) -> StorageResult<bool> {
        let mut state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        Ok(state.trees.remove(root_hash).is_some())
    }
}

#[async_trait]
impl StorageManagement for MockStorage {
    async fn get_stats(&self) -> StorageResult<StorageStats> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        let total_block_size: u64 = state.blocks.values().map(|v| v.len() as u64).sum();
        let avg_block_size = if !state.blocks.is_empty() {
            total_block_size as f64 / state.blocks.len() as f64
        } else {
            0.0
        };
        let largest_block = state
            .blocks
            .values()
            .map(|v| v.len() as u64)
            .max()
            .unwrap_or(0);

        Ok(StorageStats {
            backend: StorageBackend::InMemory,
            block_count: state.blocks.len() as u64,
            block_size_bytes: total_block_size,
            tree_count: state.trees.len() as u64,
            deduplication_savings: 0,
            average_block_size: avg_block_size,
            largest_block_size: largest_block,
            evicted_blocks: 0,
            quota_usage: None,
        })
    }

    async fn maintenance(&self) -> StorageResult<()> {
        let state = self.state.lock().unwrap();

        if state.simulate_errors {
            return Err(StorageError::Backend(state.error_message.clone()));
        }

        // Mock maintenance does nothing
        Ok(())
    }
}

// ============================================================================
// Mock Content Hasher
// ============================================================================

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
/// use crucible_core::test_support::mocks::MockContentHasher;
/// use crucible_core::traits::change_detection::ContentHasher;
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

// ============================================================================
// Mock Hash Lookup Storage
// ============================================================================

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

// ============================================================================
// Mock Change Detector
// ============================================================================

/// Mock change detector for testing
///
/// Provides a simple implementation of change detection that combines
/// mock storage and hashing for comprehensive testing.
#[derive(Debug, Clone)]
pub struct MockChangeDetector {
    storage: MockHashLookupStorage,
}

impl MockChangeDetector {
    /// Create a new mock change detector
    pub fn new() -> Self {
        Self {
            storage: MockHashLookupStorage::new(),
        }
    }

    /// Create with specific storage
    pub fn with_storage(storage: MockHashLookupStorage) -> Self {
        Self { storage }
    }

    /// Get the underlying storage for configuration
    pub fn storage(&self) -> &MockHashLookupStorage {
        &self.storage
    }
}

impl Default for MockChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChangeDetector for MockChangeDetector {
    async fn detect_changes(&self, current_files: &[FileHashInfo]) -> Result<ChangeSet, HashError> {
        let mut changes = ChangeSet::new();
        let mut current_paths = std::collections::HashSet::new();

        for file in current_files {
            current_paths.insert(file.relative_path.clone());

            match self.storage.lookup_file_hash(&file.relative_path).await? {
                Some(stored) => {
                    if stored.content_hash != file.content_hash {
                        changes.add_changed(file.clone());
                    } else {
                        changes.add_unchanged(file.clone());
                    }
                }
                None => {
                    changes.add_new(file.clone());
                }
            }
        }

        // Find deleted files
        let all_stored = self.storage.get_all_hashes().await?;
        for stored_path in all_stored.keys() {
            if !current_paths.contains(stored_path) {
                changes.add_deleted(stored_path.clone());
            }
        }

        Ok(changes)
    }

    async fn detect_changes_with_metrics(
        &self,
        current_files: &[FileHashInfo],
    ) -> Result<ChangeDetectionResult, HashError> {
        let start = std::time::Instant::now();
        let changes = self.detect_changes(current_files).await?;
        let elapsed = start.elapsed();

        let metrics = ChangeDetectionMetrics {
            total_files: current_files.len(),
            changed_files: changes.changed.len() + changes.new.len(),
            skipped_files: changes.unchanged.len(),
            change_detection_time: elapsed,
            database_round_trips: 1,
            cache_hit_rate: 0.8,
            files_per_second: current_files.len() as f64 / elapsed.as_secs_f64().max(0.001),
        };

        Ok(ChangeDetectionResult::new(changes, metrics))
    }

    async fn detect_changes_for_paths(&self, paths: &[String]) -> Result<ChangeSet, HashError> {
        let mut changes = ChangeSet::new();

        for path in paths {
            match self.storage.lookup_file_hash(path).await? {
                Some(_) => {
                    // For mock, just mark as unchanged
                    let file_info = FileHashInfo::new(
                        FileHash::new([0u8; 32]),
                        1024,
                        SystemTime::now(),
                        HashAlgorithm::Blake3,
                        path.clone(),
                    );
                    changes.add_unchanged(file_info);
                }
                None => {
                    changes.add_deleted(path.clone());
                }
            }
        }

        Ok(changes)
    }

    async fn check_file_changed(&self, path: &str) -> Result<Option<FileHashInfo>, HashError> {
        match self.storage.lookup_file_hash(path).await? {
            Some(stored) => Ok(Some(stored.to_file_hash_info(HashAlgorithm::Blake3))),
            None => Ok(None),
        }
    }

    async fn get_changed_files_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
        limit: Option<usize>,
    ) -> Result<Vec<FileHashInfo>, HashError> {
        let stored = self
            .storage
            .lookup_changed_files_since(since, limit)
            .await?;
        Ok(stored
            .into_iter()
            .map(|s| s.to_file_hash_info(HashAlgorithm::Blake3))
            .collect())
    }

    async fn batch_check_files_changed(
        &self,
        paths: &[String],
    ) -> Result<HashMap<String, bool>, HashError> {
        let mut results = HashMap::new();
        for path in paths {
            let exists = self.storage.lookup_file_hash(path).await?.is_some();
            results.insert(path.clone(), exists);
        }
        Ok(results)
    }

    async fn detect_deleted_files(
        &self,
        current_paths: &[String],
    ) -> Result<Vec<String>, HashError> {
        let all_stored = self.storage.get_all_hashes().await?;
        let current_set: std::collections::HashSet<_> = current_paths.iter().collect();

        Ok(all_stored
            .keys()
            .filter(|path| !current_set.contains(path))
            .cloned()
            .collect())
    }

    async fn get_change_statistics(&self) -> Result<ChangeStatistics, HashError> {
        let all_hashes = self.storage.get_all_hashes().await?;

        Ok(ChangeStatistics {
            total_tracked_files: all_hashes.len(),
            average_changes_per_day: 2.5,
            most_recent_change: Some(chrono::Utc::now()),
            oldest_tracked_file: Some(chrono::Utc::now() - chrono::Duration::days(7)),
            typical_change_rate: 0.1,
            average_database_round_trips: 1.5,
            average_cache_hit_rate: 0.8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_hashing_algorithm() {
        let hasher = MockHashingAlgorithm::new();

        // Deterministic hashing
        let hash1 = hasher.hash(b"test data");
        let hash2 = hasher.hash(b"test data");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 32);

        // Different inputs produce different hashes
        let hash3 = hasher.hash(b"different");
        assert_ne!(hash1, hash3);

        // Empty input is valid
        let empty = hasher.hash(b"");
        assert_eq!(empty.len(), 32);

        // Algorithm properties
        assert_eq!(hasher.algorithm_name(), "MockHash");
        assert_eq!(hasher.hash_length(), 32);
    }

    #[test]
    fn test_mock_hashing_hex_conversion() {
        let hasher = MockHashingAlgorithm::new();
        let hash = hasher.hash(b"test");
        let hex = hasher.to_hex(&hash);

        assert_eq!(hex.len(), 64); // 32 bytes * 2
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));

        let decoded = hasher.from_hex(&hex).unwrap();
        assert_eq!(decoded, hash);
    }

    #[tokio::test]
    async fn test_mock_storage_basic_operations() {
        let storage = MockStorage::new();

        // Store block
        storage.store_block("hash1", b"test data").await.unwrap();
        assert_eq!(storage.block_count(), 1);

        // Retrieve block
        let data = storage.get_block("hash1").await.unwrap();
        assert_eq!(data, Some(b"test data".to_vec()));

        // Check existence
        assert!(storage.block_exists("hash1").await.unwrap());
        assert!(!storage.block_exists("nonexistent").await.unwrap());

        // Delete block
        assert!(storage.delete_block("hash1").await.unwrap());
        assert_eq!(storage.block_count(), 0);
        assert!(!storage.delete_block("hash1").await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_storage_statistics() {
        let storage = MockStorage::new();

        storage.store_block("hash1", b"data1").await.unwrap();
        storage.store_block("hash2", b"data2").await.unwrap();
        storage.get_block("hash1").await.unwrap();

        let stats = storage.stats();
        assert_eq!(stats.store_count, 2);
        assert_eq!(stats.get_count, 1);
        assert_eq!(stats.total_bytes_stored, 10); // "data1" + "data2"
    }

    #[tokio::test]
    async fn test_mock_storage_error_simulation() {
        let storage = MockStorage::new();

        // Enable error simulation
        storage.set_simulate_errors(true, "Storage full");

        // Operations should fail
        let result = storage.store_block("hash1", b"data").await;
        assert!(result.is_err());

        // Disable errors
        storage.set_simulate_errors(false, "");
        storage.store_block("hash1", b"data").await.unwrap();
        assert_eq!(storage.block_count(), 1);
    }

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

    #[tokio::test]
    async fn test_mock_change_detector() {
        let detector = MockChangeDetector::new();

        // Add stored file
        let stored = StoredHash::new(
            "mock:existing_md".to_string(),
            "existing.md".to_string(),
            FileHash::new([1u8; 32]),
            1024,
            chrono::Utc::now(),
        );
        detector
            .storage()
            .add_stored_hash("existing.md".to_string(), stored);

        // Current files: one changed, one new
        let current_files = vec![
            FileHashInfo::new(
                FileHash::new([2u8; 32]), // Different hash
                1024,
                SystemTime::now(),
                HashAlgorithm::Blake3,
                "existing.md".to_string(),
            ),
            FileHashInfo::new(
                FileHash::new([3u8; 32]),
                2048,
                SystemTime::now(),
                HashAlgorithm::Blake3,
                "new.md".to_string(),
            ),
        ];

        // Detect changes
        let changes = detector.detect_changes(&current_files).await.unwrap();
        assert_eq!(changes.changed.len(), 1);
        assert_eq!(changes.new.len(), 1);
        assert!(changes.has_changes());

        // Detect with metrics
        let result = detector
            .detect_changes_with_metrics(&current_files)
            .await
            .unwrap();
        assert!(result.has_changes());
        assert_eq!(result.metrics.total_files, 2);
    }
}
