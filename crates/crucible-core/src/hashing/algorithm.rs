//! Hashing Algorithm Trait and Implementations
//!
//! This module provides a trait-based abstraction for cryptographic hashing algorithms,
//! following the Open/Closed Principle (OCP). New algorithms can be added without
//! modifying existing code.
//!
//! # Design Pattern
//!
//! The `HashingAlgorithm` trait enables dependency injection and makes the system
//! extensible for new hashing algorithms without breaking existing code.
//!
//! # Examples
//!
//! ```rust
//! use crucible_core::hashing::algorithm::{HashingAlgorithm, Blake3Algorithm, Sha256Algorithm};
//!
//! // Use BLAKE3 (recommended for performance)
//! let hasher = Blake3Algorithm;
//! let hash = hasher.hash(b"Hello, world!");
//! println!("BLAKE3 hash: {}", hasher.to_hex(&hash));
//!
//! // Use SHA256 (for compatibility)
//! let hasher = Sha256Algorithm;
//! let hash = hasher.hash(b"Hello, world!");
//! println!("SHA256 hash: {}", hasher.to_hex(&hash));
//! ```

use std::fmt;

/// Trait for cryptographic hashing algorithms
///
/// This trait provides a uniform interface for different hashing algorithms,
/// enabling dependency injection and following the Open/Closed Principle.
/// New algorithms can be added by implementing this trait without modifying
/// existing code.
///
/// # Design Benefits
///
/// - **Extensibility**: Add new algorithms without changing existing code (OCP)
/// - **Testability**: Easy to create mock hashers for testing
/// - **Flexibility**: Switch algorithms at runtime via trait objects
/// - **Type Safety**: Compile-time guarantees about algorithm capabilities
///
/// # Thread Safety
///
/// All implementations must be `Send + Sync + Clone` to work across threads
/// and in async contexts.
pub trait HashingAlgorithm: Send + Sync + Clone + fmt::Debug {
    /// Compute hash for a block of data
    ///
    /// # Arguments
    ///
    /// * `data` - The raw data to hash
    ///
    /// # Returns
    ///
    /// Raw hash bytes (length depends on algorithm)
    fn hash(&self, data: &[u8]) -> Vec<u8>;

    /// Compute hash for two child nodes in a Merkle tree
    ///
    /// This is used for constructing Merkle trees where parent nodes
    /// are computed from the concatenation of their children.
    ///
    /// # Arguments
    ///
    /// * `left` - Hash of the left child node
    /// * `right` - Hash of the right child node
    ///
    /// # Returns
    ///
    /// Hash of the concatenated children
    fn hash_nodes(&self, left: &[u8], right: &[u8]) -> Vec<u8> {
        let mut combined = Vec::with_capacity(left.len() + right.len());
        combined.extend_from_slice(left);
        combined.extend_from_slice(right);
        self.hash(&combined)
    }

    /// Get the name of the hash algorithm
    ///
    /// # Returns
    ///
    /// Algorithm name (e.g., "BLAKE3", "SHA256")
    fn algorithm_name(&self) -> &'static str;

    /// Get the length of the hash in bytes
    ///
    /// # Returns
    ///
    /// Number of bytes in the hash output
    fn hash_length(&self) -> usize;

    /// Convert hash bytes to hexadecimal string
    ///
    /// # Arguments
    ///
    /// * `hash` - Raw hash bytes
    ///
    /// # Returns
    ///
    /// Hexadecimal string representation
    fn to_hex(&self, hash: &[u8]) -> String {
        hash.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    }

    /// Parse hexadecimal string to hash bytes
    ///
    /// # Arguments
    ///
    /// * `hex` - Hexadecimal string
    ///
    /// # Returns
    ///
    /// Raw hash bytes or error if invalid hex
    fn from_hex(&self, hex: &str) -> Result<Vec<u8>, String> {
        if !hex.len().is_multiple_of(2) {
            return Err("Hex string must have even length".to_string());
        }

        (0..hex.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&hex[i..i + 2], 16)
                    .map_err(|e| format!("Invalid hex character: {}", e))
            })
            .collect()
    }

    /// Validate if a hash string has the correct format
    ///
    /// # Arguments
    ///
    /// * `hash` - The hash string to validate
    ///
    /// # Returns
    ///
    /// `true` if valid format, `false` otherwise
    fn is_valid_hash(&self, hash: &str) -> bool {
        hash.len() == self.hash_length() * 2 && // hex string is 2x byte length
        hash.chars().all(|c| c.is_ascii_hexdigit())
    }
}

/// BLAKE3 hashing algorithm implementation
///
/// BLAKE3 is a cryptographic hash function that is:
/// - **Fast**: 10-20x faster than SHA256 on typical hardware
/// - **Secure**: Based on BLAKE2, audited and trusted
/// - **Parallel**: Can utilize multiple CPU cores
/// - **Flexible**: Fixed 32-byte output
///
/// # Performance
///
/// - ~10-20 GB/s on modern CPUs (single-threaded)
/// - ~100+ GB/s on modern CPUs (multi-threaded)
/// - Constant-time operations (no timing attacks)
///
/// # Use Cases
///
/// - Content addressing (recommended)
/// - File integrity verification
/// - Deduplication detection
/// - Fast hash tables
#[derive(Debug, Clone, Copy, Default)]
pub struct Blake3Algorithm;

impl HashingAlgorithm for Blake3Algorithm {
    fn hash(&self, data: &[u8]) -> Vec<u8> {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(data);
        hasher.finalize().as_bytes().to_vec()
    }

    fn algorithm_name(&self) -> &'static str {
        "blake3"
    }

    fn hash_length(&self) -> usize {
        32 // BLAKE3 always produces 32 bytes
    }
}

/// SHA256 hashing algorithm implementation
///
/// SHA256 is a cryptographic hash function from the SHA-2 family:
/// - **Widely supported**: Standard across many systems
/// - **Well-tested**: Extensively audited since 2001
/// - **Secure**: No known practical attacks
/// - **Slower**: ~5-10x slower than BLAKE3
///
/// # Performance
///
/// - ~1-2 GB/s on modern CPUs
/// - More CPU-intensive than BLAKE3
/// - No parallel processing capabilities
///
/// # Use Cases
///
/// - Compatibility with existing systems
/// - Regulatory compliance requirements
/// - Interoperability with other tools
#[derive(Debug, Clone, Copy, Default)]
pub struct Sha256Algorithm;

impl HashingAlgorithm for Sha256Algorithm {
    fn hash(&self, data: &[u8]) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().to_vec()
    }

    fn algorithm_name(&self) -> &'static str {
        "sha256"
    }

    fn hash_length(&self) -> usize {
        32 // SHA256 produces 32 bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blake3_basic() {
        let hasher = Blake3Algorithm;
        let hash = hasher.hash(b"Hello, world!");

        assert_eq!(hash.len(), 32);
        assert_eq!(hasher.hash_length(), 32);
        assert_eq!(hasher.algorithm_name(), "blake3");
    }

    #[test]
    fn test_blake3_deterministic() {
        let hasher = Blake3Algorithm;
        let hash1 = hasher.hash(b"test data");
        let hash2 = hasher.hash(b"test data");

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_blake3_different_inputs() {
        let hasher = Blake3Algorithm;
        let hash1 = hasher.hash(b"data1");
        let hash2 = hasher.hash(b"data2");

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_sha256_basic() {
        let hasher = Sha256Algorithm;
        let hash = hasher.hash(b"Hello, world!");

        assert_eq!(hash.len(), 32);
        assert_eq!(hasher.hash_length(), 32);
        assert_eq!(hasher.algorithm_name(), "sha256");
    }

    #[test]
    fn test_sha256_deterministic() {
        let hasher = Sha256Algorithm;
        let hash1 = hasher.hash(b"test data");
        let hash2 = hasher.hash(b"test data");

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hex_conversion() {
        let hasher = Blake3Algorithm;
        let hash = hasher.hash(b"test");
        let hex = hasher.to_hex(&hash);

        assert_eq!(hex.len(), 64); // 32 bytes * 2
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));

        let decoded = hasher.from_hex(&hex).unwrap();
        assert_eq!(decoded, hash);
    }

    #[test]
    fn test_hex_validation() {
        let hasher = Blake3Algorithm;

        // Valid hex of correct length
        let valid_hex = "a".repeat(64);
        assert!(hasher.is_valid_hash(&valid_hex));

        // Invalid: wrong length
        assert!(!hasher.is_valid_hash("abc"));

        // Invalid: non-hex characters
        assert!(!hasher.is_valid_hash(&"g".repeat(64)));

        // Invalid: odd length
        assert!(!hasher.is_valid_hash(&"a".repeat(63)));
    }

    #[test]
    fn test_hash_nodes() {
        let hasher = Blake3Algorithm;
        let left = hasher.hash(b"left");
        let right = hasher.hash(b"right");
        let parent = hasher.hash_nodes(&left, &right);

        assert_eq!(parent.len(), 32);

        // Should be deterministic
        let parent2 = hasher.hash_nodes(&left, &right);
        assert_eq!(parent, parent2);

        // Different order should produce different hash
        let parent_reversed = hasher.hash_nodes(&right, &left);
        assert_ne!(parent, parent_reversed);
    }

    #[test]
    fn test_algorithms_produce_different_hashes() {
        let blake3 = Blake3Algorithm;
        let sha256 = Sha256Algorithm;

        let data = b"same input";
        let blake3_hash = blake3.hash(data);
        let sha256_hash = sha256.hash(data);

        // Different algorithms should produce different hashes
        assert_ne!(blake3_hash, sha256_hash);

        // But both should be 32 bytes
        assert_eq!(blake3_hash.len(), 32);
        assert_eq!(sha256_hash.len(), 32);
    }

    #[test]
    fn test_empty_input() {
        let hasher = Blake3Algorithm;
        let hash = hasher.hash(b"");

        assert_eq!(hash.len(), 32);
        // Empty input should still produce valid hash
        assert!(!hash.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_from_hex_error_handling() {
        let hasher = Blake3Algorithm;

        // Odd length
        assert!(hasher.from_hex("abc").is_err());

        // Invalid hex character
        assert!(hasher.from_hex("gg").is_err());

        // Valid hex
        assert!(hasher.from_hex("abcd").is_ok());
    }
}
