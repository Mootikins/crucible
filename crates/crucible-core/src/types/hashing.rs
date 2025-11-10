//! Hashing types for content addressing and change detection
//!
//! This module defines the core types used for content hashing, including
//! file hashes, block hashes, and hash algorithms. These types provide
//! the foundation for content-addressed storage and change detection
//! throughout the Crucible system.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

// Re-export BlockHash from parser to avoid duplication
pub use crucible_parser::types::BlockHash;

/// A BLAKE3 hash used for content addressing
///
/// This type wraps a 32-byte BLAKE3 hash and provides convenient
/// methods for serialization, deserialization, and display.
///
/// # Examples
///
/// // TODO: Add example once API stabilizes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileHash([u8; 32]);

impl FileHash {
    /// Create a new FileHash from raw bytes
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the hash as a byte slice
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Get the hash as a hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Create a FileHash from a hex string
    pub fn from_hex(hex: &str) -> Result<Self, HashError> {
        let bytes = hex::decode(hex).map_err(|_| HashError::InvalidHexFormat)?;
        if bytes.len() != 32 {
            return Err(HashError::InvalidLength { len: bytes.len() });
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self(array))
    }

    /// Create a zero hash (all zeros)
    pub fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Check if this is a zero hash
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

impl fmt::Display for FileHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl FromStr for FileHash {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// Hash algorithms supported by the ContentHasher trait
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HashAlgorithm {
    /// BLAKE3 - fast, secure, and designed for modern systems
    Blake3,
    /// SHA256 - widely supported and well-understood
    Sha256,
}

impl HashAlgorithm {
    /// Get the hash output size in bytes for this algorithm
    pub fn output_size(&self) -> usize {
        match self {
            HashAlgorithm::Blake3 => 32,
            HashAlgorithm::Sha256 => 32,
        }
    }

    /// Get the string representation of this algorithm
    pub fn as_str(&self) -> &'static str {
        match self {
            HashAlgorithm::Blake3 => "blake3",
            HashAlgorithm::Sha256 => "sha256",
        }
    }
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for HashAlgorithm {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "blake3" => Ok(HashAlgorithm::Blake3),
            "sha256" => Ok(HashAlgorithm::Sha256),
            _ => Err(HashError::UnknownAlgorithm {
                algorithm: s.to_string(),
            }),
        }
    }
}

/// Errors that can occur during hash operations
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HashError {
    /// Invalid hex format provided
    #[error("Invalid hex format")]
    InvalidHexFormat,

    /// Invalid hash length (expected 32 bytes for BLAKE3/SHA256)
    #[error("Invalid hash length: {len} bytes (expected 32)")]
    InvalidLength { len: usize },

    /// Unknown hash algorithm specified
    #[error("Unknown hash algorithm: {algorithm}")]
    UnknownAlgorithm { algorithm: String },

    /// I/O error during hashing operation
    #[error("I/O error during hashing: {error}")]
    IoError { error: String },
}

impl From<std::io::Error> for HashError {
    fn from(err: std::io::Error) -> Self {
        HashError::IoError {
            error: err.to_string(),
        }
    }
}

/// Information about a hashed file including metadata
///
/// This type combines the file content hash with important metadata
/// for change detection and file system operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHashInfo {
    /// The content hash of the file
    pub content_hash: FileHash,

    /// File size in bytes
    pub size: u64,

    /// Last modification timestamp
    pub modified: std::time::SystemTime,

    /// Hash algorithm used
    pub algorithm: HashAlgorithm,

    /// Relative path from the vault root
    pub relative_path: String,
}

impl FileHashInfo {
    /// Create a new FileHashInfo
    pub fn new(
        content_hash: FileHash,
        size: u64,
        modified: std::time::SystemTime,
        algorithm: HashAlgorithm,
        relative_path: String,
    ) -> Self {
        Self {
            content_hash,
            size,
            modified,
            algorithm,
            relative_path,
        }
    }

    /// Get the file hash
    pub fn hash(&self) -> FileHash {
        self.content_hash
    }

    /// Check if this file info matches another based on content hash
    pub fn content_matches(&self, other: &FileHashInfo) -> bool {
        self.content_hash == other.content_hash
    }

    /// Check if metadata matches (size and modification time)
    pub fn metadata_matches(&self, other: &FileHashInfo) -> bool {
        self.size == other.size && self.modified == other.modified
    }
}

/// Information about a hashed content block
///
/// This type represents a single block of content (heading, paragraph,
/// code block, etc.) with its hash and metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockHashInfo {
    /// The content hash of the block
    pub content_hash: BlockHash,

    /// Block type (heading, paragraph, code, etc.)
    pub block_type: String,

    /// Start position in the source document
    pub start_offset: usize,

    /// End position in the source document
    pub end_offset: usize,

    /// Hash algorithm used
    pub algorithm: HashAlgorithm,
}

impl BlockHashInfo {
    /// Create a new BlockHashInfo
    pub fn new(
        content_hash: BlockHash,
        block_type: String,
        start_offset: usize,
        end_offset: usize,
        algorithm: HashAlgorithm,
    ) -> Self {
        Self {
            content_hash,
            block_type,
            start_offset,
            end_offset,
            algorithm,
        }
    }

    /// Get the block hash
    pub fn hash(&self) -> BlockHash {
        self.content_hash
    }

    /// Get the block content length
    pub fn content_length(&self) -> usize {
        self.end_offset - self.start_offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_file_hash_roundtrip() {
        let hash_bytes = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98,
            0x76, 0x54, 0x32, 0x10,
        ];

        let hash = FileHash::new(hash_bytes);
        let hex_str = hash.to_hex();
        let parsed_hash = FileHash::from_hex(&hex_str).unwrap();

        assert_eq!(hash, parsed_hash);
        assert_eq!(hash.to_string(), hex_str);
    }

    #[test]
    fn test_block_hash_roundtrip() {
        let hash_bytes = [
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
            0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
            0x66, 0x77, 0x88, 0x99,
        ];

        let hash = BlockHash::new(hash_bytes);
        let hex_str = hash.to_hex();
        let parsed_hash = BlockHash::from_hex(&hex_str).unwrap();

        assert_eq!(hash, parsed_hash);
        assert_eq!(hash.to_string(), hex_str);
    }

    #[test]
    fn test_hash_algorithm() {
        assert_eq!(HashAlgorithm::Blake3.output_size(), 32);
        assert_eq!(HashAlgorithm::Sha256.output_size(), 32);
        assert_eq!(HashAlgorithm::Blake3.as_str(), "blake3");
        assert_eq!(HashAlgorithm::Sha256.as_str(), "sha256");

        let parsed: HashAlgorithm = "blake3".parse().unwrap();
        assert_eq!(parsed, HashAlgorithm::Blake3);

        let parsed: HashAlgorithm = "SHA256".parse().unwrap();
        assert_eq!(parsed, HashAlgorithm::Sha256);
    }

    #[test]
    fn test_file_hash_info() {
        let hash = FileHash::new([
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98,
            0x76, 0x54, 0x32, 0x10,
        ]);

        let info1 = FileHashInfo::new(
            hash,
            1024,
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "test.md".to_string(),
        );

        let info2 = FileHashInfo::new(
            hash,
            2048, // Different size
            SystemTime::now(),
            HashAlgorithm::Blake3,
            "test.md".to_string(),
        );

        assert!(info1.content_matches(&info2)); // Same content hash
        assert!(!info1.metadata_matches(&info2)); // Different size
    }

    #[test]
    fn test_block_hash_info() {
        let hash = BlockHash::new([
            0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
            0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55,
            0x66, 0x77, 0x88, 0x99,
        ]);

        let info = BlockHashInfo::new(hash, "heading".to_string(), 0, 50, HashAlgorithm::Blake3);

        assert_eq!(info.content_length(), 50);
        assert_eq!(info.block_type, "heading");
        assert_eq!(info.hash(), hash);
    }

    #[test]
    fn test_zero_hash() {
        let file_hash = FileHash::zero();
        assert!(file_hash.is_zero());

        let block_hash = BlockHash::zero();
        assert!(block_hash.is_zero());

        let non_zero = FileHash::new([1u8; 32]);
        assert!(!non_zero.is_zero());
    }
}
