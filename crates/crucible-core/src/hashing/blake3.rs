//! BLAKE3 Content Hasher Implementation
//!
//! This module provides a BLAKE3-based implementation of the ContentHasher trait
//! for content hashing and change detection.
//!
//! ## Features
//!
//! - **Cryptographically Secure**: Uses BLAKE3 for collision-resistant hashing
//! - **High Performance**: BLAKE3 is faster than SHA256 and optimized for modern CPUs
//! - **Deterministic**: Same input always produces the same output
//! - **Thread-Safe**: Safe to use across multiple threads

use crate::storage::ContentHasher;
use std::sync::atomic::{AtomicUsize, Ordering};

/// BLAKE3-based content hasher
///
/// This struct implements the ContentHasher trait using BLAKE3 cryptographic hash.
/// It provides deterministic, collision-resistant hashing for content blocks
/// with superior performance compared to SHA256.
#[derive(Debug, Clone)]
pub struct Blake3Hasher {
    /// Counter for tracking hash operations (useful for debugging/monitoring)
    operation_count: std::sync::Arc<AtomicUsize>,
}

impl Blake3Hasher {
    /// Create a new BLAKE3 hasher instance
    pub fn new() -> Self {
        Self {
            operation_count: std::sync::Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get the current operation count
    pub fn operation_count(&self) -> usize {
        self.operation_count.load(Ordering::Relaxed)
    }

    /// Reset the operation counter
    pub fn reset_counter(&self) {
        self.operation_count.store(0, Ordering::Relaxed);
    }

    /// Compute BLAKE3 hash of raw data and return as hex string
    fn compute_blake3_hash(&self, data: &[u8]) -> String {
        use blake3::Hasher;

        let mut hasher = Hasher::new();
        hasher.update(data);
        let result = hasher.finalize();
        hex::encode(result.as_bytes())
    }
}

impl Default for Blake3Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentHasher for Blake3Hasher {
    fn hash_block(&self, data: &[u8]) -> String {
        self.operation_count.fetch_add(1, Ordering::Relaxed);
        self.compute_blake3_hash(data)
    }

    fn algorithm_name(&self) -> &'static str {
        "blake3"
    }

    fn hash_length(&self) -> usize {
        32 // BLAKE3 produces 32-byte hashes (256 bits)
    }
}

/// Global BLAKE3 content hasher instance
pub static BLAKE3_CONTENT_HASHER: std::sync::LazyLock<Blake3Hasher> =
    std::sync::LazyLock::new(|| Blake3Hasher {
        operation_count: std::sync::Arc::new(AtomicUsize::new(0)),
    });

#[cfg(test)]
mod tests {
    use super::*;

    // BLAKE3 test vectors
    const TEST_VECTORS: &[(&[u8], &str)] = &[
        (
            b"",
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262",
        ),
        (
            b"a",
            "17762fddd969a453925d65717ac3eea21320b66b54342fde15128d6caf21215f",
        ),
        (
            b"abc",
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
        ),
    ];

    #[test]
    fn test_blake3_hasher_new() {
        let hasher = Blake3Hasher::new();
        assert_eq!(hasher.operation_count(), 0);
    }

    #[test]
    fn test_algorithm_name() {
        let hasher = Blake3Hasher::new();
        assert_eq!(hasher.algorithm_name(), "blake3");
    }

    #[test]
    fn test_hash_length() {
        let hasher = Blake3Hasher::new();
        assert_eq!(hasher.hash_length(), 32);
    }

    #[test]
    fn test_hash_block_deterministic() {
        let hasher = Blake3Hasher::new();
        let data = b"Hello, Crucible!";

        let hash1 = hasher.hash_block(data);
        let hash2 = hasher.hash_block(data);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_blake3_test_vectors() {
        let hasher = Blake3Hasher::new();

        for (data, expected_hash) in TEST_VECTORS {
            let computed_hash = hasher.hash_block(data);
            assert_eq!(computed_hash, *expected_hash);
        }
    }

    #[test]
    fn test_is_valid_hash() {
        let hasher = Blake3Hasher::new();

        let valid_hash = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";
        assert!(hasher.is_valid_hash(valid_hash));

        assert!(!hasher.is_valid_hash(""));
        assert!(!hasher.is_valid_hash("invalid"));
        assert!(!hasher.is_valid_hash("af1349b9f5f9")); // Wrong length
    }

    #[test]
    fn test_static_hasher() {
        let result = BLAKE3_CONTENT_HASHER.hash_block(b"test");
        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
