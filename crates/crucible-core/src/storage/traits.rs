//! Storage Traits and Types
//!
//! This module provides the `ContentHasher` trait for pluggable content hashing.

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
}
