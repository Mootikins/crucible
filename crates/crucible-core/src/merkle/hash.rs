//! Efficient hash types for Merkle tree operations
//!
//! This module provides optimized hash types for Merkle tree construction,
//! following patterns from production Merkle tree implementations like Oxen AI.
//!
//! ## Design Philosophy
//!
//! - **Compact representation**: `[u8; 16]` instead of `String` for 16-byte hashes
//! - **BLAKE3-based**: Fast, cryptographically secure hashing
//! - **Zero-copy operations**: Hash comparison and combination without allocation
//! - **Debugging support**: Hex string conversion when needed

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A 16-byte hash used for Merkle tree nodes
///
/// This compact hash type uses 16 bytes (128 bits) instead of 32 bytes to optimize
/// memory usage in Merkle trees. For Merkle trees, collision resistance of 128 bits
/// is sufficient since we're dealing with document-scale data, not blockchain-scale.
///
/// ## Properties
///
/// - **Size**: 16 bytes (128 bits)
/// - **Copy**: Zero-cost cloning
/// - **Equality**: Fast byte-wise comparison
/// - **Hash**: Can be used as HashMap key
///
/// ## Example
///
/// ```rust,ignore
/// use crucible_core::merkle::NodeHash;
///
/// let hash1 = NodeHash::from_content(b"Hello, World!");
/// let hash2 = NodeHash::from_content(b"Hello, World!");
/// assert_eq!(hash1, hash2);
///
/// let combined = NodeHash::combine(&hash1, &hash2);
/// assert_ne!(combined, hash1);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeHash([u8; 16]);

impl NodeHash {
    /// Size of the hash in bytes
    pub const SIZE: usize = 16;

    /// Create a new NodeHash from raw bytes
    ///
    /// # Arguments
    ///
    /// * `bytes` - A 16-byte array containing the hash value
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let hash = NodeHash::new([0u8; 16]);
    /// ```
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Create a zero hash (all bytes are 0)
    ///
    /// This is useful for representing empty or null nodes.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let zero = NodeHash::zero();
    /// assert!(zero.is_zero());
    /// ```
    pub fn zero() -> Self {
        Self([0u8; 16])
    }

    /// Check if this is a zero hash
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let zero = NodeHash::zero();
    /// assert!(zero.is_zero());
    ///
    /// let hash = NodeHash::from_content(b"data");
    /// assert!(!hash.is_zero());
    /// ```
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 16]
    }

    /// Get the hash as a byte slice
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let hash = NodeHash::from_content(b"data");
    /// let bytes: &[u8] = hash.as_bytes();
    /// assert_eq!(bytes.len(), 16);
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Create a hash from content using BLAKE3
    ///
    /// This hashes the content and takes the first 16 bytes of the BLAKE3 digest.
    ///
    /// # Arguments
    ///
    /// * `content` - The content to hash
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let hash = NodeHash::from_content(b"Hello, World!");
    /// ```
    ///
    /// # Performance
    ///
    /// This is a hot path in tree construction. Inlined for performance.
    #[inline]
    pub fn from_content(content: &[u8]) -> Self {
        let digest = blake3::hash(content);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        Self(bytes)
    }

    /// Combine two hashes into a new hash
    ///
    /// This is the core operation for building Merkle trees. It takes two child
    /// node hashes and produces a parent node hash by hashing their concatenation.
    ///
    /// # Arguments
    ///
    /// * `left` - Hash of the left child node
    /// * `right` - Hash of the right child node
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let left = NodeHash::from_content(b"left");
    /// let right = NodeHash::from_content(b"right");
    /// let parent = NodeHash::combine(&left, &right);
    /// ```
    ///
    /// # Performance
    ///
    /// This is a hot path in tree construction. Inlined for performance.
    #[inline]
    pub fn combine(left: &NodeHash, right: &NodeHash) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(&left.0);
        hasher.update(&right.0);
        let digest = hasher.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        Self(bytes)
    }

    /// Combine multiple hashes into a single hash
    ///
    /// This is useful for aggregating section hashes into a root hash.
    ///
    /// # Arguments
    ///
    /// * `hashes` - Slice of hashes to combine
    ///
    /// # Returns
    ///
    /// The combined hash, or zero hash if the input is empty
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let hashes = vec![
    ///     NodeHash::from_content(b"one"),
    ///     NodeHash::from_content(b"two"),
    ///     NodeHash::from_content(b"three"),
    /// ];
    /// let combined = NodeHash::combine_many(&hashes);
    /// ```
    ///
    /// # Performance
    ///
    /// Optimized for combining multiple hashes in one pass.
    #[inline]
    pub fn combine_many(hashes: &[NodeHash]) -> Self {
        if hashes.is_empty() {
            return Self::zero();
        }

        let mut hasher = Hasher::new();
        for hash in hashes {
            hasher.update(&hash.0);
        }
        let digest = hasher.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        Self(bytes)
    }

    /// Convert the hash to a hexadecimal string
    ///
    /// This is primarily for debugging and display purposes.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let hash = NodeHash::from_content(b"data");
    /// println!("Hash: {}", hash.to_hex());
    /// ```
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Create a hash from a hexadecimal string
    ///
    /// # Arguments
    ///
    /// * `hex` - A hexadecimal string (32 characters for 16 bytes)
    ///
    /// # Errors
    ///
    /// Returns an error if the hex string is invalid or has the wrong length.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let hex = "0123456789abcdef0123456789abcdef";
    /// let hash = NodeHash::from_hex(hex).unwrap();
    /// assert_eq!(hash.to_hex(), hex);
    /// ```
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let bytes = hex::decode(hex).map_err(|e| format!("Invalid hex format: {}", e))?;
        if bytes.len() != 16 {
            return Err(format!(
                "Invalid hash length: expected 16 bytes, got {}",
                bytes.len()
            ));
        }
        let mut array = [0u8; 16];
        array.copy_from_slice(&bytes);
        Ok(Self(array))
    }
}

impl Default for NodeHash {
    fn default() -> Self {
        Self::zero()
    }
}

impl fmt::Debug for NodeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeHash({})", self.to_hex())
    }
}

impl fmt::Display for NodeHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl From<[u8; 16]> for NodeHash {
    fn from(bytes: [u8; 16]) -> Self {
        Self::new(bytes)
    }
}

impl From<&[u8; 16]> for NodeHash {
    fn from(bytes: &[u8; 16]) -> Self {
        Self::new(*bytes)
    }
}

impl AsRef<[u8]> for NodeHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_hash_creation() {
        let hash = NodeHash::new([1u8; 16]);
        assert_eq!(hash.as_bytes().len(), 16);
        assert!(!hash.is_zero());
    }

    #[test]
    fn test_zero_hash() {
        let zero = NodeHash::zero();
        assert!(zero.is_zero());
        assert_eq!(zero.as_bytes(), &[0u8; 16]);
    }

    #[test]
    fn test_from_content() {
        let content = b"Hello, World!";
        let hash1 = NodeHash::from_content(content);
        let hash2 = NodeHash::from_content(content);

        // Same content should produce same hash
        assert_eq!(hash1, hash2);
        assert!(!hash1.is_zero());
    }

    #[test]
    fn test_from_content_different() {
        let hash1 = NodeHash::from_content(b"Content 1");
        let hash2 = NodeHash::from_content(b"Content 2");

        // Different content should produce different hashes
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_combine_hashes() {
        let left = NodeHash::from_content(b"left");
        let right = NodeHash::from_content(b"right");

        let combined1 = NodeHash::combine(&left, &right);
        let combined2 = NodeHash::combine(&left, &right);

        // Same inputs should produce same output
        assert_eq!(combined1, combined2);

        // Combined hash should be different from inputs
        assert_ne!(combined1, left);
        assert_ne!(combined1, right);
    }

    #[test]
    fn test_combine_order_matters() {
        let left = NodeHash::from_content(b"left");
        let right = NodeHash::from_content(b"right");

        let combined_lr = NodeHash::combine(&left, &right);
        let combined_rl = NodeHash::combine(&right, &left);

        // Order should matter
        assert_ne!(combined_lr, combined_rl);
    }

    #[test]
    fn test_combine_many() {
        let hashes = vec![
            NodeHash::from_content(b"one"),
            NodeHash::from_content(b"two"),
            NodeHash::from_content(b"three"),
        ];

        let combined1 = NodeHash::combine_many(&hashes);
        let combined2 = NodeHash::combine_many(&hashes);

        assert_eq!(combined1, combined2);
        assert!(!combined1.is_zero());
    }

    #[test]
    fn test_combine_many_empty() {
        let hashes: Vec<NodeHash> = vec![];
        let combined = NodeHash::combine_many(&hashes);

        assert!(combined.is_zero());
    }

    #[test]
    fn test_hex_conversion() {
        let hash = NodeHash::from_content(b"test data");
        let hex = hash.to_hex();

        assert_eq!(hex.len(), 32); // 16 bytes = 32 hex chars

        let restored = NodeHash::from_hex(&hex).unwrap();
        assert_eq!(hash, restored);
    }

    #[test]
    fn test_hex_roundtrip() {
        let original = NodeHash::new([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0,
                                       0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0]);
        let hex = original.to_hex();
        let restored = NodeHash::from_hex(&hex).unwrap();

        assert_eq!(original, restored);
    }

    #[test]
    fn test_from_hex_invalid() {
        // Too short
        assert!(NodeHash::from_hex("1234").is_err());

        // Too long
        assert!(NodeHash::from_hex("123456789abcdef0123456789abcdef012345678").is_err());

        // Invalid hex characters
        assert!(NodeHash::from_hex("gggggggggggggggggggggggggggggggg").is_err());
    }

    #[test]
    fn test_default() {
        let hash: NodeHash = Default::default();
        assert!(hash.is_zero());
    }

    #[test]
    fn test_debug_display() {
        let hash = NodeHash::from_content(b"test");
        let debug = format!("{:?}", hash);
        let display = format!("{}", hash);

        assert!(debug.contains("NodeHash"));
        assert_eq!(display.len(), 32); // hex string
    }

    #[test]
    fn test_copy_trait() {
        let hash1 = NodeHash::from_content(b"test");
        let hash2 = hash1; // Copy, not move
        let hash3 = hash1; // Can copy again

        assert_eq!(hash1, hash2);
        assert_eq!(hash1, hash3);
    }

    #[test]
    fn test_hash_trait() {
        use std::collections::HashMap;

        let mut map = HashMap::new();
        let hash = NodeHash::from_content(b"key");

        map.insert(hash, "value");
        assert_eq!(map.get(&hash), Some(&"value"));
    }

    #[test]
    fn test_serialization() {
        let hash = NodeHash::from_content(b"test data");

        // Test JSON serialization
        let json = serde_json::to_string(&hash).unwrap();
        let restored: NodeHash = serde_json::from_str(&json).unwrap();

        assert_eq!(hash, restored);
    }

    #[test]
    fn test_from_array() {
        let array = [0xABu8; 16];
        let hash1 = NodeHash::from(array);
        let hash2 = NodeHash::from(&array);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.as_bytes(), &array);
    }

    #[test]
    fn test_as_ref() {
        let hash = NodeHash::from_content(b"test");
        let bytes: &[u8] = hash.as_ref();

        assert_eq!(bytes.len(), 16);
        assert_eq!(bytes, hash.as_bytes());
    }
}
