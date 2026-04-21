//! Mock hashing algorithm.

use crate::hashing::algorithm::HashingAlgorithm;

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
/// use crate::test_support::mocks::MockHashingAlgorithm;
/// use crate::hashing::algorithm::HashingAlgorithm;
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

    fn algorithm_name(&self) -> &'static str {
        "MockHash"
    }

    fn hash_length(&self) -> usize {
        32
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

        let decoded = hasher.parse_hex(&hex).unwrap();
        assert_eq!(decoded, hash);
    }
}
