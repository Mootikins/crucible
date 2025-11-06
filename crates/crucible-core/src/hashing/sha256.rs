//! SHA256 Content Hasher Implementation
//!
//! This module provides a SHA256-based implementation of the ContentHasher trait
//! for content-addressed storage with Merkle tree support.
//!
//! ## Features
//!
//! - **Cryptographically Secure**: Uses SHA256 for collision-resistant hashing
//! - **Deterministic**: Same input always produces the same output
//! - **High Performance**: Optimized for frequent hashing operations
//! - **Thread-Safe**: Safe to use across multiple threads
//! - **Standards Compliant**: Produces standard SHA256 hex digests

use sha2::{Digest, Sha256};
use crate::storage::ContentHasher;
use std::sync::atomic::{AtomicUsize, Ordering};

/// SHA256-based content hasher for content-addressed storage
///
/// This struct implements the ContentHasher trait using SHA256 cryptographic hash.
/// It provides deterministic, collision-resistant hashing for both content blocks
/// and Merkle tree node combinations.
#[derive(Debug, Clone)]
pub struct SHA256Hasher {
    /// Counter for tracking hash operations (useful for debugging/monitoring)
    operation_count: std::sync::Arc<AtomicUsize>,
}

impl SHA256Hasher {
    /// Create a new SHA256 hasher instance
    ///
    /// # Returns
    /// A new SHA256Hasher with zero operation count
    pub fn new() -> Self {
        Self {
            operation_count: std::sync::Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get the current operation count
    ///
    /// # Returns
    /// The number of hash operations performed by this instance
    pub fn operation_count(&self) -> usize {
        self.operation_count.load(Ordering::Relaxed)
    }

    /// Reset the operation counter
    pub fn reset_counter(&self) {
        self.operation_count.store(0, Ordering::Relaxed);
    }

    /// Compute SHA256 hash of raw data and return as hex string
    ///
    /// # Arguments
    /// * `data` - Raw bytes to hash
    ///
    /// # Returns
    /// 64-character hexadecimal string representing the SHA256 hash
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
    /// Compute SHA256 hash for a block of content
    ///
    /// # Arguments
    /// * `data` - The raw data to hash
    ///
    /// # Returns
    /// 64-character hexadecimal string representation of the SHA256 hash
    fn hash_block(&self, data: &[u8]) -> String {
        self.operation_count.fetch_add(1, Ordering::Relaxed);
        self.compute_sha256_hash(data)
    }

    /// Compute SHA256 hash for two child nodes in the Merkle tree
    ///
    /// This method combines the left and right child hashes by concatenating
    /// them and computing the SHA256 hash of the combined string.
    ///
    /// # Arguments
    /// * `left` - Hash of the left child node (hex string)
    /// * `right` - Hash of the right child node (hex string)
    ///
    /// # Returns
    /// 64-character hexadecimal string representation of the parent hash
    fn hash_nodes(&self, left: &str, right: &str) -> String {
        self.operation_count.fetch_add(1, Ordering::Relaxed);

        // Combine the two hash strings and hash the result
        let combined = format!("{}{}", left, right);
        self.compute_sha256_hash(combined.as_bytes())
    }

    /// Get the name of the hash algorithm
    fn algorithm_name(&self) -> &'static str {
        "sha256"
    }

    /// Get the length of the hash in bytes
    fn hash_length(&self) -> usize {
        32 // SHA256 produces 32-byte hashes
    }
}

/// Global SHA256 content hasher instance
///
/// This static instance can be used throughout the application for consistent
/// SHA256 hashing operations.
pub static SHA256_CONTENT_HASHER: std::sync::LazyLock<SHA256Hasher> = std::sync::LazyLock::new(|| {
    SHA256Hasher {
        operation_count: std::sync::Arc::new(AtomicUsize::new(0)),
    }
});

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Test vectors from RFC 6231 for SHA256
    const TEST_VECTORS: &[(&[u8], &str)] = &[
        (b"", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        (b"a", "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"),
        (b"abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
        (b"message digest", "f7846f55cf23e14eebeab5b4e1550cad5b509e3348fbc4efa3a1413d393cb650"),
        (b"abcdefghijklmnopqrstuvwxyz", "71c480df93d6ae2f1efad1447c66c9525e316218cf51fc8d9ed832f2daf18b73"),
        (b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "db4bfcbd4da0cd85a60c3c37d3fbd8805c77f15fc6b1fdfe614ee0a7c8fdb4c0"),
        (b"12345678901234567890123456789012345678901234567890123456789012345678901234567890", "f371bc4a311f2b009eef952dd83ca80e2b60026c8e935592d0f9c308453c813e"),
    ];

    #[test]
    fn test_sha256_hasher_new() {
        // Test that we can create a new hasher
        let hasher = SHA256Hasher::new();
        assert_eq!(hasher.operation_count(), 0);

        // Test default implementation
        let default_hasher = SHA256Hasher::default();
        assert_eq!(default_hasher.operation_count(), 0);
    }

    #[test]
    fn test_algorithm_name() {
        let hasher = SHA256Hasher::new();
        assert_eq!(hasher.algorithm_name(), "sha256");
    }

    #[test]
    fn test_hash_length() {
        let hasher = SHA256Hasher::new();
        assert_eq!(hasher.hash_length(), 32); // 32 bytes for SHA256
    }

    #[test]
    fn test_hash_block_empty_data() {
        let hasher = SHA256Hasher::new();
        let result = hasher.hash_block(b"");

        // SHA256 of empty string from RFC test vectors
        assert_eq!(result, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(result.len(), 64); // 32 bytes * 2 hex chars per byte
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_single_byte() {
        let hasher = SHA256Hasher::new();
        let result = hasher.hash_block(b"a");

        assert_eq!(result, "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_short_string() {
        let hasher = SHA256Hasher::new();
        let result = hasher.hash_block(b"abc");

        assert_eq!(result, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_long_string() {
        let hasher = SHA256Hasher::new();
        let data = b"abcdefghijklmnopqrstuvwxyz";
        let result = hasher.hash_block(data);

        assert_eq!(result, "71c480df93d6ae2f1efad1447c66c9525e316218cf51fc8d9ed832f2daf18b73");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_very_long_string() {
        let hasher = SHA256Hasher::new();
        let data = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let result = hasher.hash_block(data);

        assert_eq!(result, "db4bfcbd4da0cd85a60c3c37d3fbd8805c77f15fc6b1fdfe614ee0a7c8fdb4c0");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_large_data() {
        let hasher = SHA256Hasher::new();
        let data = b"12345678901234567890123456789012345678901234567890123456789012345678901234567890";
        let result = hasher.hash_block(data);

        assert_eq!(result, "f371bc4a311f2b009eef952dd83ca80e2b60026c8e935592d0f9c308453c813e");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_deterministic_behavior() {
        let hasher = SHA256Hasher::new();
        let data = b"Hello, Crucible!";

        // Hash the same data multiple times
        let hash1 = hasher.hash_block(data);
        let hash2 = hasher.hash_block(data);
        let hash3 = hasher.hash_block(data);

        // All hashes should be identical
        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
        assert_eq!(hasher.operation_count(), 3);

        // Test with different hasher instances
        let hasher2 = SHA256Hasher::new();
        let hash4 = hasher2.hash_block(data);
        assert_eq!(hash1, hash4);
    }

    #[test]
    fn test_hash_block_different_inputs() {
        let hasher = SHA256Hasher::new();

        let hash1 = hasher.hash_block(b"foo");
        let hash2 = hasher.hash_block(b"bar");
        let hash3 = hasher.hash_block(b"baz");

        // Different inputs should produce different hashes
        assert_ne!(hash1, hash2);
        assert_ne!(hash2, hash3);
        assert_ne!(hash1, hash3);

        // All should have same length
        assert_eq!(hash1.len(), 64);
        assert_eq!(hash2.len(), 64);
        assert_eq!(hash3.len(), 64);

        assert_eq!(hasher.operation_count(), 3);
    }

    #[test]
    fn test_hash_nodes() {
        let hasher = SHA256Hasher::new();

        let left = "abc123";
        let right = "def456";
        let result = hasher.hash_nodes(left, right);

        // Should be a valid hex string of correct length
        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hasher.operation_count(), 1);

        // Test different combinations
        let result2 = hasher.hash_nodes("def456", "abc123");
        assert_ne!(result, result2); // Order should matter
        assert_eq!(hasher.operation_count(), 2);
    }

    #[test]
    fn test_hash_nodes_deterministic() {
        let hasher = SHA256Hasher::new();

        let left = "abc123";
        let right = "def456";

        let hash1 = hasher.hash_nodes(left, right);
        let hash2 = hasher.hash_nodes(left, right);
        let hash3 = hasher.hash_nodes(left, right);

        // All should be identical
        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
        assert_eq!(hasher.operation_count(), 3);
    }

    #[test]
    fn test_hash_nodes_with_hashes() {
        let hasher = SHA256Hasher::new();

        // Test with actual SHA256 hashes (should be 64 chars each)
        let left = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"; // empty
        let right = "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"; // "a"

        let result = hasher.hash_nodes(left, right);

        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_nodes_empty_strings() {
        let hasher = SHA256Hasher::new();

        // Test with empty strings
        let result1 = hasher.hash_nodes("", "");
        let result2 = hasher.hash_nodes("", "abc");
        let result3 = hasher.hash_nodes("abc", "");

        // result1 should be different (empty + empty)
        assert_ne!(result1, result2);

        // result2 and result3 are the same because concatenation: "" + "abc" = "abc" + "" = "abc"
        assert_eq!(result2, result3);

        // All should be valid hex strings
        assert_eq!(result1.len(), 64);
        assert_eq!(result2.len(), 64);
        assert_eq!(result3.len(), 64);

        assert_eq!(hasher.operation_count(), 3);
    }

    #[test]
    fn test_rfc_test_vectors() {
        let hasher = SHA256Hasher::new();

        for (data, expected_hash) in TEST_VECTORS {
            let computed_hash = hasher.hash_block(data);
            assert_eq!(
                computed_hash, *expected_hash,
                "SHA256 test vector failed for input: {:?}",
                String::from_utf8_lossy(data)
            );
        }

        // Should have performed one hash operation per test vector
        assert_eq!(hasher.operation_count(), TEST_VECTORS.len());
    }

    #[test]
    fn test_operation_counter() {
        let hasher = SHA256Hasher::new();

        assert_eq!(hasher.operation_count(), 0);

        hasher.hash_block(b"test");
        assert_eq!(hasher.operation_count(), 1);

        hasher.hash_block(b"test2");
        assert_eq!(hasher.operation_count(), 2);

        hasher.hash_nodes("hash1", "hash2");
        assert_eq!(hasher.operation_count(), 3);

        hasher.reset_counter();
        assert_eq!(hasher.operation_count(), 0);
    }

    #[test]
    fn test_static_hasher() {
        // Test the static instance
        let result = SHA256_CONTENT_HASHER.hash_block(b"test");

        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));

        // Should be deterministic
        let result2 = SHA256_CONTENT_HASHER.hash_block(b"test");
        assert_eq!(result, result2);
    }

    #[test]
    fn test_hasher_clone() {
        let hasher1 = SHA256Hasher::new();
        hasher1.hash_block(b"test");
        assert_eq!(hasher1.operation_count(), 1);

        let hasher2 = hasher1.clone();
        assert_eq!(hasher2.operation_count(), 1); // Should preserve counter

        hasher2.hash_block(b"test2");
        // Both share the same Arc<AtomicUsize>, so counter is shared
        assert_eq!(hasher1.operation_count(), 2); // Shared counter
        assert_eq!(hasher2.operation_count(), 2); // Shared counter
    }

    #[test]
    fn test_is_valid_hash_implementation() {
        let hasher = SHA256Hasher::new();

        // Test with valid SHA256 hash (64 hex chars)
        let valid_hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(hasher.is_valid_hash(valid_hash));

        // Test with invalid hashes
        assert!(!hasher.is_valid_hash("")); // Empty
        assert!(!hasher.is_valid_hash("invalid")); // Not hex
        assert!(!hasher.is_valid_hash("e3b0c44")); // Wrong length
        assert!(!hasher.is_valid_hash("g3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")); // Invalid hex char
    }

    #[test]
    fn test_hash_block_different_sizes() {
        let hasher = SHA256Hasher::new();

        // Test various block sizes
        let test_data = vec![
            vec![], // 0 bytes
            vec![0x42], // 1 byte
            vec![0x41, 0x42, 0x43], // 3 bytes
            vec![0u8; 64], // 64 bytes
            vec![0u8; 1024], // 1KB
            vec![0u8; 65536], // 64KB
        ];

        let mut hashes = Vec::new();
        for data in &test_data {
            let hash = hasher.hash_block(data);
            assert_eq!(hash.len(), 64);
            assert!(hasher.is_valid_hash(&hash));
            hashes.push(hash);
        }

        // All hashes should be different
        for (i, hash1) in hashes.iter().enumerate() {
            for (j, hash2) in hashes.iter().enumerate() {
                if i != j {
                    assert_ne!(hash1, hash2, "Hashes for different sizes should be different");
                }
            }
        }

        assert_eq!(hasher.operation_count(), test_data.len());
    }

    #[test]
    fn test_unicode_content() {
        let hasher = SHA256Hasher::new();

        // Test with UTF-8 content
        let unicode_text = "Hello, 世界! 🚀 Crucible";
        let hash = hasher.hash_block(unicode_text.as_bytes());

        assert_eq!(hash.len(), 64);
        assert!(hasher.is_valid_hash(&hash));

        // Should be deterministic
        let hash2 = hasher.hash_block(unicode_text.as_bytes());
        assert_eq!(hash, hash2);

        assert_eq!(hasher.operation_count(), 2);
    }

    #[test]
    fn test_content_hasher_trait_compatibility() {
        // Test that our implementation works anywhere ContentHasher is required
        fn process_with_hasher<H: ContentHasher>(hasher: &H, data: &[u8]) -> String {
            hasher.hash_block(data)
        }

        let hasher = SHA256Hasher::new();
        let result = process_with_hasher(&hasher, b"test");

        assert_eq!(result.len(), 64);
        assert!(hasher.is_valid_hash(&result));
    }

    // Integration test with the MerkleTree (if available)
    #[test]
    fn test_merkle_tree_integration() {
        use crate::storage::merkle::MerkleTree;
        use crate::storage::HashedBlock;

        let hasher = SHA256Hasher::new();

        // Create some test blocks
        let block1 = HashedBlock::from_data(
            b"Block 1 content".to_vec(),
            0,
            0,
            false,
            &hasher,
        ).unwrap();

        let block2 = HashedBlock::from_data(
            b"Block 2 content".to_vec(),
            1,
            20,
            false,
            &hasher,
        ).unwrap();

        let block3 = HashedBlock::from_data(
            b"Block 3 content".to_vec(),
            2,
            40,
            true, // last block
            &hasher,
        ).unwrap();

        let blocks = vec![block1.clone(), block2.clone(), block3.clone()];

        // Create Merkle tree
        let tree = MerkleTree::from_blocks(&blocks, &hasher).unwrap();

        // Verify tree structure
        assert_eq!(tree.block_count, 3);
        assert_eq!(tree.leaf_hashes.len(), 3);
        assert!(tree.verify_integrity(&hasher).is_ok());

        // Verify all blocks are in the tree
        assert!(tree.nodes.contains_key(&block1.hash));
        assert!(tree.nodes.contains_key(&block2.hash));
        assert!(tree.nodes.contains_key(&block3.hash));
    }
}