//! Storage Traits and Types
//!
//! This module provides content hashing and storage statistics types.
//!
//! # Overview
//!
//! - **`ContentHasher`** - Pluggable content hashing algorithms (BLAKE3, etc.)
//! - **`StorageStats`** - Storage backend statistics for monitoring
//! - **`StorageBackend`** - Backend type identifier

use serde::{Deserialize, Serialize};

/// Trait for pluggable content hashing algorithms
///
/// This trait allows different hash algorithms to be used while maintaining
/// a consistent interface. The default implementation uses BLAKE3.
pub trait ContentHasher: Send + Sync {
    /// Compute hash for a block of content
    ///
    /// # Arguments
    /// * `data` - The raw data to hash
    ///
    /// # Returns
    /// Hexadecimal string representation of the hash
    fn hash_block(&self, data: &[u8]) -> String;

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
    SQLite,
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
    /// Average block size in bytes
    pub average_block_size: f64,
    /// Largest block size in bytes
    pub largest_block_size: u64,
    /// Number of evicted blocks due to memory pressure (if applicable)
    pub evicted_blocks: u64,
    /// Storage quota usage if applicable
    pub quota_usage: Option<QuotaUsage>,
}

impl Default for StorageStats {
    fn default() -> Self {
        Self {
            backend: StorageBackend::InMemory,
            block_count: 0,
            block_size_bytes: 0,
            average_block_size: 0.0,
            largest_block_size: 0,
            evicted_blocks: 0,
            quota_usage: None,
        }
    }
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
            format!("{:016x}", hasher.finish())
        }

        fn algorithm_name(&self) -> &'static str {
            "mock"
        }

        fn hash_length(&self) -> usize {
            8 // 64-bit hash = 8 bytes
        }
    }

    #[test]
    fn test_hasher_validation() {
        let hasher = MockHasher;

        // Valid hash (16 hex characters for 8 bytes)
        assert!(hasher.is_valid_hash("0123456789abcdef"));

        // Invalid hashes
        assert!(!hasher.is_valid_hash("")); // Empty
        assert!(!hasher.is_valid_hash("invalid")); // Not hex
        assert!(!hasher.is_valid_hash("0123456789abcde")); // Wrong length
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

    #[test]
    fn test_storage_stats_default() {
        let stats = StorageStats::default();
        assert_eq!(stats.block_count, 0);
        assert_eq!(stats.block_size_bytes, 0);
        assert!(stats.quota_usage.is_none());
    }
}
