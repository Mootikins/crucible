//! Block Link Hash Computation
//!
//! This module provides utilities for computing content hashes for block links.
//! Block links use normalized content hashes for validation and CAS lookup.

use super::normalize_block_text;
use blake3;

/// Compute BLAKE3 hash of normalized block text
///
/// This function normalizes the block text and computes its BLAKE3 hash
/// for use in block link validation ([[Note#Heading^5#hash]]).
///
/// # Arguments
///
/// * `text` - The raw block text
///
/// # Returns
///
/// 32-byte BLAKE3 hash of the normalized text
///
/// # Examples
///
/// ```
/// use crucible_core::hashing::compute_block_hash;
///
/// let text = "  This is **bold** text  ";
/// let hash = compute_block_hash(text);
/// assert_eq!(hash.len(), 32);
/// ```
pub fn compute_block_hash(text: &str) -> [u8; 32] {
    let normalized = normalize_block_text(text);
    let mut hasher = blake3::Hasher::new();
    hasher.update(normalized.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Convert block hash to hex string for display/debugging
///
/// # Arguments
///
/// * `hash` - 32-byte BLAKE3 hash
///
/// # Returns
///
/// Hex-encoded string representation
pub fn hash_to_hex(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}

/// Parse hex string back to block hash
///
/// # Arguments
///
/// * `hex_str` - Hex-encoded hash string
///
/// # Returns
///
/// Result containing the 32-byte hash or error
pub fn hex_to_hash(hex_str: &str) -> Result<[u8; 32], hex::FromHexError> {
    let bytes = hex::decode(hex_str)?;
    if bytes.len() != 32 {
        return Err(hex::FromHexError::InvalidStringLength);
    }

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&bytes);
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_block_hash() {
        let text = "Hello world";
        let hash = compute_block_hash(text);
        assert_eq!(hash.len(), 32);

        // Same text should produce same hash
        let hash2 = compute_block_hash(text);
        assert_eq!(hash, hash2);

        // Different text should produce different hash
        let hash3 = compute_block_hash("Different text");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_normalized_hash_consistency() {
        // These should produce the same hash after normalization
        let text1 = "  Hello world  ";
        let text2 = "Hello world";
        let text3 = "Hello    world";

        let hash1 = compute_block_hash(text1);
        let hash2 = compute_block_hash(text2);
        let hash3 = compute_block_hash(text3);

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    #[test]
    fn test_markdown_preserved_in_hash() {
        // Markdown formatting should be preserved
        let with_formatting = "This is **bold** text";
        let without_formatting = "This is bold text";

        let hash1 = compute_block_hash(with_formatting);
        let hash2 = compute_block_hash(without_formatting);

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_to_hex() {
        let hash = [0x12, 0x34, 0x56, 0x78, 0xab, 0xcd, 0xef, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let hex_str = hash_to_hex(&hash);
        assert!(hex_str.starts_with("12345678abcdef"));
    }

    #[test]
    fn test_hex_to_hash() {
        let hex_str = "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0";
        let hash = hex_to_hash(hex_str).unwrap();
        assert_eq!(hash.len(), 32);

        // Round trip
        let hex_str2 = hash_to_hex(&hash);
        assert_eq!(hex_str, hex_str2);
    }

    #[test]
    fn test_hex_to_hash_invalid_length() {
        let result = hex_to_hash("short");
        assert!(result.is_err());
    }

    #[test]
    fn test_real_world_block() {
        // Simulate a real block from markdown
        let block_text = "- The **primary benefit** is `performance` and *reliability*";
        let hash = compute_block_hash(block_text);

        // Should be normalized to remove list marker
        let normalized = normalize_block_text(block_text);
        assert_eq!(normalized, "The **primary benefit** is `performance` and *reliability*");

        // Hash should be deterministic
        let hash2 = compute_block_hash(block_text);
        assert_eq!(hash, hash2);
    }
}
