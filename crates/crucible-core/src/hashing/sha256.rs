//! SHA256 Content Hasher Implementation
//!
//! This module provides a SHA256-based implementation of the ContentHasher trait
//! for content hashing and change detection.

use crate::storage::ContentHasher;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicUsize, Ordering};

/// SHA256-based content hasher
#[derive(Debug, Clone)]
pub struct SHA256Hasher {
    operation_count: std::sync::Arc<AtomicUsize>,
}

impl SHA256Hasher {
    pub fn new() -> Self {
        Self {
            operation_count: std::sync::Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn operation_count(&self) -> usize {
        self.operation_count.load(Ordering::Relaxed)
    }

    pub fn reset_counter(&self) {
        self.operation_count.store(0, Ordering::Relaxed);
    }

    fn compute_sha256_hash(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        hex::encode(result)
    }
}

impl Default for SHA256Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentHasher for SHA256Hasher {
    fn hash_block(&self, data: &[u8]) -> String {
        self.operation_count.fetch_add(1, Ordering::Relaxed);
        self.compute_sha256_hash(data)
    }

    fn algorithm_name(&self) -> &'static str {
        "sha256"
    }

    fn hash_length(&self) -> usize {
        32 // SHA256 produces 32-byte hashes
    }
}

/// Global SHA256 content hasher instance
pub static SHA256_CONTENT_HASHER: std::sync::LazyLock<SHA256Hasher> =
    std::sync::LazyLock::new(|| SHA256Hasher {
        operation_count: std::sync::Arc::new(AtomicUsize::new(0)),
    });

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_VECTORS: &[(&[u8], &str)] = &[
        (
            b"",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
    ];

    #[test]
    fn test_sha256_hasher_new() {
        let hasher = SHA256Hasher::new();
        assert_eq!(hasher.operation_count(), 0);
    }

    #[test]
    fn test_algorithm_name() {
        let hasher = SHA256Hasher::new();
        assert_eq!(hasher.algorithm_name(), "sha256");
    }

    #[test]
    fn test_hash_length() {
        let hasher = SHA256Hasher::new();
        assert_eq!(hasher.hash_length(), 32);
    }

    #[test]
    fn test_hash_block_deterministic() {
        let hasher = SHA256Hasher::new();
        let data = b"Hello, Crucible!";

        let hash1 = hasher.hash_block(data);
        let hash2 = hasher.hash_block(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_rfc_test_vectors() {
        let hasher = SHA256Hasher::new();

        for (data, expected_hash) in TEST_VECTORS {
            let computed_hash = hasher.hash_block(data);
            assert_eq!(computed_hash, *expected_hash);
        }
    }

    #[test]
    fn test_is_valid_hash() {
        let hasher = SHA256Hasher::new();

        let valid_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(hasher.is_valid_hash(valid_hash));

        assert!(!hasher.is_valid_hash(""));
        assert!(!hasher.is_valid_hash("invalid"));
        assert!(!hasher.is_valid_hash("e3b0c44")); // Wrong length
    }

    #[test]
    fn test_static_hasher() {
        let result = SHA256_CONTENT_HASHER.hash_block(b"test");
        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
