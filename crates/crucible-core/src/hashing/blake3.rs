//! BLAKE3 Content Hasher Implementation
//!
//! This module provides a BLAKE3-based implementation of the ContentHasher trait
//! for content-addressed storage with Merkle tree support.
//!
//! ## Features
//!
//! - **Cryptographically Secure**: Uses BLAKE3 for collision-resistant hashing
//! - **High Performance**: BLAKE3 is faster than SHA256 and optimized for modern CPUs
//! - **Deterministic**: Same input always produces the same output
//! - **Thread-Safe**: Safe to use across multiple threads
//! - **Modern**: Built with performance and security in mind

use crate::storage::ContentHasher;
use std::sync::atomic::{AtomicUsize, Ordering};

/// BLAKE3-based content hasher for content-addressed storage
///
/// This struct implements the ContentHasher trait using BLAKE3 cryptographic hash.
/// It provides deterministic, collision-resistant hashing for both content blocks
/// and Merkle tree node combinations with superior performance compared to SHA256.
#[derive(Debug, Clone)]
pub struct Blake3Hasher {
    /// Counter for tracking hash operations (useful for debugging/monitoring)
    operation_count: std::sync::Arc<AtomicUsize>,
}

impl Blake3Hasher {
    /// Create a new BLAKE3 hasher instance
    ///
    /// # Returns
    /// A new Blake3Hasher with zero operation count
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

    /// Compute BLAKE3 hash of raw data and return as hex string
    ///
    /// # Arguments
    /// * `data` - Raw bytes to hash
    ///
    /// # Returns
    /// 64-character hexadecimal string representing the BLAKE3 hash
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
    /// Compute BLAKE3 hash for a block of content
    ///
    /// # Arguments
    /// * `data` - The raw data to hash
    ///
    /// # Returns
    /// 64-character hexadecimal string representation of the BLAKE3 hash
    fn hash_block(&self, data: &[u8]) -> String {
        self.operation_count.fetch_add(1, Ordering::Relaxed);
        self.compute_blake3_hash(data)
    }

    /// Compute BLAKE3 hash for two child nodes in the Merkle tree
    ///
    /// This method combines the left and right child hashes by concatenating
    /// them and computing the BLAKE3 hash of the combined string.
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
        self.compute_blake3_hash(combined.as_bytes())
    }

    /// Get the name of the hash algorithm
    fn algorithm_name(&self) -> &'static str {
        "BLAKE3"
    }

    /// Get the length of the hash in bytes
    fn hash_length(&self) -> usize {
        32 // BLAKE3 produces 32-byte hashes (256 bits)
    }
}

/// Global BLAKE3 content hasher instance
///
/// This static instance can be used throughout the application for consistent
/// BLAKE3 hashing operations.
pub static BLAKE3_CONTENT_HASHER: std::sync::LazyLock<Blake3Hasher> = std::sync::LazyLock::new(|| {
    Blake3Hasher {
        operation_count: std::sync::Arc::new(AtomicUsize::new(0)),
    }
});

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // BLAKE3 test vectors - these are known BLAKE3 hashes for test inputs
    const TEST_VECTORS: &[(&[u8], &str)] = &[
        (b"", "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"),
        (b"a", "17762fddd969a453925d65717ac3eea21320b66b54342fde15128d6caf21215f"),
        (b"abc", "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"),
        (b"message digest", "7bc2a2eeb95ddbf9b7ecf6adcb76b453091c58dc43955e1d9482b1942f08d19b"),
        (b"abcdefghijklmnopqrstuvwxyz", "2468eec8894acfb4e4df3a51ea916ba115d48268287754290aae8e9e6228e85f"),
        (b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "8bee3200baa9f3a1acd279f049f914f110e730555ff15109bd59cdd73895e239"),
    ];

    #[test]
    fn test_blake3_hasher_new() {
        // Test that we can create a new hasher
        let hasher = Blake3Hasher::new();
        assert_eq!(hasher.operation_count(), 0);

        // Test default implementation
        let default_hasher = Blake3Hasher::default();
        assert_eq!(default_hasher.operation_count(), 0);
    }

    #[test]
    fn test_algorithm_name() {
        let hasher = Blake3Hasher::new();
        assert_eq!(hasher.algorithm_name(), "BLAKE3");
    }

    #[test]
    fn test_hash_length() {
        let hasher = Blake3Hasher::new();
        assert_eq!(hasher.hash_length(), 32); // 32 bytes for BLAKE3
    }

    #[test]
    fn test_hash_block_empty_data() {
        let hasher = Blake3Hasher::new();
        let result = hasher.hash_block(b"");

        // BLAKE3 of empty string - this should be a known value
        assert_eq!(result, "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262");
        assert_eq!(result.len(), 64); // 32 bytes * 2 hex chars per byte
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_single_byte() {
        let hasher = Blake3Hasher::new();
        let result = hasher.hash_block(b"a");

        assert_eq!(result, "17762fddd969a453925d65717ac3eea21320b66b54342fde15128d6caf21215f");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_short_string() {
        let hasher = Blake3Hasher::new();
        let result = hasher.hash_block(b"abc");

        assert_eq!(result, "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_long_string() {
        let hasher = Blake3Hasher::new();
        let data = b"abcdefghijklmnopqrstuvwxyz";
        let result = hasher.hash_block(data);

        assert_eq!(result, "2468eec8894acfb4e4df3a51ea916ba115d48268287754290aae8e9e6228e85f");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_block_very_long_string() {
        let hasher = Blake3Hasher::new();
        let data = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let result = hasher.hash_block(data);

        assert_eq!(result, "8bee3200baa9f3a1acd279f049f914f110e730555ff15109bd59cdd73895e239");
        assert_eq!(result.len(), 64);
        assert_eq!(hasher.operation_count(), 1);
    }

    
    #[test]
    fn test_hash_block_deterministic_behavior() {
        let hasher = Blake3Hasher::new();
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
        let hasher2 = Blake3Hasher::new();
        let hash4 = hasher2.hash_block(data);
        assert_eq!(hash1, hash4);
    }

    #[test]
    fn test_hash_block_different_inputs() {
        let hasher = Blake3Hasher::new();

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
        let hasher = Blake3Hasher::new();

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
        let hasher = Blake3Hasher::new();

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
        let hasher = Blake3Hasher::new();

        // Test with actual BLAKE3 hashes (should be 64 chars each)
        let left = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"; // empty
        let right = "17762fddd969a4532612c0126bba068202b5a3dba6e59ce6c7a5ca1976dbac24"; // "a"

        let result = hasher.hash_nodes(left, right);

        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hasher.operation_count(), 1);
    }

    #[test]
    fn test_hash_nodes_empty_strings() {
        let hasher = Blake3Hasher::new();

        // Test with empty strings
        let result1 = hasher.hash_nodes("", "");
        let result2 = hasher.hash_nodes("", "abc");
        let result3 = hasher.hash_nodes("abc", "");
        let result4 = hasher.hash_nodes("abc", "def");
        let result5 = hasher.hash_nodes("def", "abc");

        // The empty string case should be different from others
        assert_ne!(result1, result2);
        assert_ne!(result1, result3);

        // Note: result2 and result3 are the same because "" + "abc" = "abc" = "abc" + ""
        // This is expected behavior with simple concatenation
        assert_eq!(result2, result3);

        // Order should matter when both strings are non-empty
        assert_ne!(result4, result5);

        // All should be valid hex strings
        assert_eq!(result1.len(), 64);
        assert_eq!(result2.len(), 64);
        assert_eq!(result3.len(), 64);
        assert_eq!(result4.len(), 64);
        assert_eq!(result5.len(), 64);

        assert_eq!(hasher.operation_count(), 5);
    }

    #[test]
    fn test_blake3_test_vectors() {
        let hasher = Blake3Hasher::new();

        // Test with our known correct BLAKE3 test vectors
        for (data, expected_hash) in TEST_VECTORS {
            let computed_hash = hasher.hash_block(data);
            assert_eq!(
                computed_hash, *expected_hash,
                "BLAKE3 test vector failed for input: {:?}",
                String::from_utf8_lossy(data)
            );
        }

        // Should have performed one hash operation per test vector
        assert_eq!(hasher.operation_count(), TEST_VECTORS.len());
    }

    #[test]
    fn test_operation_counter() {
        let hasher = Blake3Hasher::new();

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
        let result = BLAKE3_CONTENT_HASHER.hash_block(b"test");

        assert_eq!(result.len(), 64);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));

        // Should be deterministic
        let result2 = BLAKE3_CONTENT_HASHER.hash_block(b"test");
        assert_eq!(result, result2);
    }

    #[test]
    fn test_hasher_clone() {
        let hasher1 = Blake3Hasher::new();
        hasher1.hash_block(b"test");
        assert_eq!(hasher1.operation_count(), 1);

        let hasher2 = hasher1.clone();
        assert_eq!(hasher2.operation_count(), 1); // Should preserve counter

        hasher2.hash_block(b"test2");
        // Note: Since they share the same AtomicUsize, both will see the updated count
        assert_eq!(hasher1.operation_count(), 2);
        assert_eq!(hasher2.operation_count(), 2);
    }

    #[test]
    fn test_is_valid_hash_implementation() {
        let hasher = Blake3Hasher::new();

        // Test with valid BLAKE3 hash (64 hex chars)
        let valid_hash = "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262";
        assert!(hasher.is_valid_hash(valid_hash));

        // Test with invalid hashes
        assert!(!hasher.is_valid_hash("")); // Empty
        assert!(!hasher.is_valid_hash("invalid")); // Not hex
        assert!(!hasher.is_valid_hash("af1349b9f5f9")); // Wrong length
        assert!(!hasher.is_valid_hash("gf1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262")); // Invalid hex char
    }

    #[test]
    fn test_hash_block_different_sizes() {
        let hasher = Blake3Hasher::new();

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
        let hasher = Blake3Hasher::new();

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

        let hasher = Blake3Hasher::new();
        let result = process_with_hasher(&hasher, b"test");

        assert_eq!(result.len(), 64);
        assert!(hasher.is_valid_hash(&result));
    }

    // Integration test with the MerkleTree (if available)
    #[test]
    fn test_merkle_tree_integration() {
        use crate::storage::merkle::MerkleTree;
        use crate::storage::HashedBlock;

        let hasher = Blake3Hasher::new();

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

    
    #[test]
    fn test_cryptographic_properties() {
        let hasher = Blake3Hasher::new();

        // Test avalanche effect: small change in input should produce completely different output
        let hash1 = hasher.hash_block(b"The quick brown fox jumps over the lazy dog");
        let hash2 = hasher.hash_block(b"The quick brown fox jumps over the lazy cog"); // Changed 'd' to 'g'
        let hash3 = hasher.hash_block(b"the quick brown fox jumps over the lazy dog"); // Changed 'T' to 't'

        // All hashes should be different
        assert_ne!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_ne!(hash2, hash3);

        // Count differing bits between hash1 and hash2 (should be roughly half of bits)
        let diff_bits = hash1
            .chars()
            .zip(hash2.chars())
            .map(|(a, b)| if a != b { 1 } else { 0 })
            .sum::<u32>();

        // Should have significant differences (not a strict test due to randomness)
        assert!(diff_bits > 32, "Avalanche effect not observed: only {} bits differ", diff_bits);
    }

    #[test]
    fn test_large_data_hashing() {
        let hasher = Blake3Hasher::new();

        // Create a large dataset (1MB)
        let large_data = vec![0x42u8; 1024 * 1024];

        let start = Instant::now();
        let hash = hasher.hash_block(&large_data);
        let duration = start.elapsed();

        // Should complete in reasonable time (less than 1 second on modern hardware)
        assert!(duration.as_secs() < 1, "BLAKE3 should hash 1MB quickly, took {:?}", duration);

        // Hash should be valid
        assert_eq!(hash.len(), 64);
        assert!(hasher.is_valid_hash(&hash));

        // Should be deterministic
        let hash2 = hasher.hash_block(&large_data);
        assert_eq!(hash, hash2);
    }
}