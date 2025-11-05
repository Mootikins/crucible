//! Core Traits for Content-Addressed Storage
//!
//! This module defines the fundamental traits that enable content-addressed storage
//! with pluggable backends and hash algorithms.

use async_trait::async_trait;
use crate::storage::{StorageResult, MerkleTree};
use serde::{Deserialize, Serialize};

/// Core trait for content-addressed storage backends
///
/// This trait provides a unified interface for storing and retrieving content
/// based on its cryptographic hash. Implementations can use different storage
/// mechanisms like SurrealDB, file system, or in-memory storage.
#[async_trait]
pub trait ContentAddressedStorage: Send + Sync {
    /// Store a block of content with its hash as the key
    ///
    /// # Arguments
    /// * `hash` - The SHA256 hash of the block content
    /// * `data` - The raw block data to store
    ///
    /// # Returns
    /// `Ok(())` if successful, `Err(StorageError)` if storage fails
    async fn store_block(&self, hash: &str, data: &[u8]) -> StorageResult<()>;

    /// Retrieve a block by its hash
    ///
    /// # Arguments
    /// * `hash` - The SHA256 hash of the block to retrieve
    ///
    /// # Returns
    /// `Some(Vec<u8>)` if the block exists, `None` if not found, or error if retrieval fails
    async fn get_block(&self, hash: &str) -> StorageResult<Option<Vec<u8>>>;

    /// Store a complete Merkle tree structure
    ///
    /// # Arguments
    /// * `root_hash` - The root hash of the Merkle tree
    /// * `tree` - The complete Merkle tree structure
    ///
    /// # Returns
    /// `Ok(())` if successful, `Err(StorageError)` if storage fails
    async fn store_tree(&self, root_hash: &str, tree: &MerkleTree) -> StorageResult<()>;

    /// Retrieve a Merkle tree by its root hash
    ///
    /// # Arguments
    /// * `root_hash` - The root hash of the Merkle tree to retrieve
    ///
    /// # Returns
    /// `Some(MerkleTree)` if the tree exists, `None` if not found, or error if retrieval fails
    async fn get_tree(&self, root_hash: &str) -> StorageResult<Option<MerkleTree>>;

    /// Check if a block exists in storage
    ///
    /// # Arguments
    /// * `hash` - The hash of the block to check
    ///
    /// # Returns
    /// `true` if the block exists, `false` otherwise
    async fn block_exists(&self, hash: &str) -> StorageResult<bool>;

    /// Check if a Merkle tree exists in storage
    ///
    /// # Arguments
    /// * `root_hash` - The root hash of the tree to check
    ///
    /// # Returns
    /// `true` if the tree exists, `false` otherwise
    async fn tree_exists(&self, root_hash: &str) -> StorageResult<bool>;

    /// Delete a block from storage
    ///
    /// # Arguments
    /// * `hash` - The hash of the block to delete
    ///
    /// # Returns
    /// `Ok(true)` if deleted, `Ok(false)` if it didn't exist, `Err` if deletion failed
    async fn delete_block(&self, hash: &str) -> StorageResult<bool>;

    /// Delete a Merkle tree from storage
    ///
    /// # Arguments
    /// * `root_hash` - The root hash of the tree to delete
    ///
    /// # Returns
    /// `Ok(true)` if deleted, `Ok(false)` if it didn't exist, `Err` if deletion failed
    async fn delete_tree(&self, root_hash: &str) -> StorageResult<bool>;

    /// Get storage statistics
    ///
    /// # Returns
    /// Storage backend statistics or error if unavailable
    async fn get_stats(&self) -> StorageResult<StorageStats>;

    /// Perform maintenance operations (cleanup, optimization, etc.)
    ///
    /// # Returns
    /// `Ok(())` if successful, `Err(StorageError)` if maintenance fails
    async fn maintenance(&self) -> StorageResult<()>;
}

/// Trait for pluggable content hashing algorithms
///
/// This trait allows different hash algorithms to be used while maintaining
/// a consistent interface. The default implementation uses SHA256.
pub trait ContentHasher: Send + Sync {
    /// Compute hash for a block of content
    ///
    /// # Arguments
    /// * `data` - The raw data to hash
    ///
    /// # Returns
    /// Hexadecimal string representation of the hash
    fn hash_block(&self, data: &[u8]) -> String;

    /// Compute hash for two child nodes in the Merkle tree
    ///
    /// # Arguments
    /// * `left` - Hash of the left child node
    /// * `right` - Hash of the right child node
    ///
    /// # Returns
    /// Hexadecimal string representation of the parent hash
    fn hash_nodes(&self, left: &str, right: &str) -> String;

    /// Get the name of the hash algorithm
    fn algorithm_name(&self) -> &'static str;

    /// Get the length of the hash in bytes
    fn hash_length(&self) -> usize;

    /// Validate if a hash string has the correct format
    ///
    /// # Arguments
    /// * `hash` - The hash string to validate
    ///
    /// # Returns
    /// `true` if valid format, `false` otherwise
    fn is_valid_hash(&self, hash: &str) -> bool {
        hash.len() == self.hash_length() * 2 && // hex string is 2x byte length
        hash.chars().all(|c| c.is_ascii_hexdigit())
    }
}

/// Storage backend type identifier
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageBackend {
    InMemory,
    FileBased,
    SurrealDB,
    Custom(String),
}

/// Storage statistics for monitoring and diagnostics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    /// Type of storage backend
    pub backend: StorageBackend,
    /// Total number of stored blocks
    pub block_count: u64,
    /// Total size of stored blocks in bytes
    pub block_size_bytes: u64,
    /// Total number of stored trees
    pub tree_count: u64,
    /// Number of deduplicated blocks (same content, same hash)
    pub deduplication_savings: u64,
    /// Average block size in bytes
    pub average_block_size: f64,
    /// Largest block size in bytes
    pub largest_block_size: u64,
    /// Number of evicted blocks due to memory pressure (if applicable)
    pub evicted_blocks: u64,
    /// Storage quota usage if applicable
    pub quota_usage: Option<QuotaUsage>,
}

/// Storage quota information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaUsage {
    /// Current usage in bytes
    pub used_bytes: u64,
    /// Maximum allowed bytes
    pub limit_bytes: u64,
    /// Usage percentage (0.0 to 1.0)
    pub usage_percentage: f64,
}

impl QuotaUsage {
    pub fn new(used_bytes: u64, limit_bytes: u64) -> Self {
        let usage_percentage = if limit_bytes > 0 {
            used_bytes as f64 / limit_bytes as f64
        } else {
            0.0
        };

        Self {
            used_bytes,
            limit_bytes,
            usage_percentage,
        }
    }

    pub fn is_near_limit(&self, threshold: f64) -> bool {
        self.usage_percentage >= threshold
    }
}

impl Default for QuotaUsage {
    fn default() -> Self {
        Self {
            used_bytes: 0,
            limit_bytes: u64::MAX,
            usage_percentage: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock implementation for testing
    struct MockHasher;

    impl ContentHasher for MockHasher {
        fn hash_block(&self, data: &[u8]) -> String {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        }

        fn hash_nodes(&self, left: &str, right: &str) -> String {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};

            let combined = format!("{}{}", left, right);
            let mut hasher = DefaultHasher::new();
            combined.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        }

        fn algorithm_name(&self) -> &'static str {
            "mock"
        }

        fn hash_length(&self) -> usize {
            16 // MD5 length
        }
    }

    #[test]
    fn test_hasher_validation() {
        let hasher = MockHasher;

        // Valid MD5 hash (32 hex characters)
        assert!(hasher.is_valid_hash("5d41402abc4b2a76b9719d911017c592"));

        // Invalid hashes
        assert!(!hasher.is_valid_hash("")); // Empty
        assert!(!hasher.is_valid_hash("invalid")); // Not hex
        assert!(!hasher.is_valid_hash("5d41402abc4b2a76b9719d911017c59")); // Wrong length
    }

    #[test]
    fn test_quota_usage() {
        let quota = QuotaUsage::new(500, 1000);
        assert_eq!(quota.usage_percentage, 0.5);
        assert!(quota.is_near_limit(0.4));
        assert!(!quota.is_near_limit(0.6));

        let quota = QuotaUsage::new(900, 1000);
        assert_eq!(quota.usage_percentage, 0.9);
        assert!(quota.is_near_limit(0.8));
    }
}