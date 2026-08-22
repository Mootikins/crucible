//! Hash metadata types for content addressing and change detection
//!
//! `BlockHash` is the one content hash type. `FileHash` is an alias
//! that older call sites use for the hash of a whole file.

use std::fmt;

use serde::{Deserialize, Serialize};

// Re-export BlockHash from parser to avoid duplication
pub use crate::parser::types::BlockHash;

/// A BLAKE3 hash of a whole file
///
/// The file hash and the block hash are the same 32-byte value type.
/// The alias keeps the name at call sites that hash a whole file.
pub type FileHash = BlockHash;

/// Hash algorithms that the hash types can name
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    /// BLAKE3 - the only algorithm Crucible uses
    #[default]
    Blake3,
}

impl HashAlgorithm {
    /// Get the string representation of this algorithm
    pub fn as_str(&self) -> &'static str {
        match self {
            HashAlgorithm::Blake3 => "blake3",
        }
    }
}

impl fmt::Display for HashAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
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
    /// Get the file hash
    pub fn hash(&self) -> FileHash {
        self.content_hash
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

    /// Start position in the source note
    pub start_offset: usize,

    /// End position in the source note
    pub end_offset: usize,

    /// Hash algorithm used
    pub algorithm: HashAlgorithm,
}

impl BlockHashInfo {
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

    #[test]
    fn file_hash_and_block_hash_are_one_type() {
        let bytes = [0x5au8; 32];
        let file_hash = FileHash::new(bytes);
        let block_hash = BlockHash::new(bytes);
        assert_eq!(file_hash, block_hash);

        let parsed: FileHash = file_hash.to_hex().parse().unwrap();
        assert_eq!(parsed, block_hash);
    }

    #[test]
    fn file_hash_serializes_as_block_hash() {
        let hash = FileHash::new([0x01u8; 32]);
        let json = serde_json::to_string(&hash).unwrap();
        let back: BlockHash = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hash);
    }

    #[test]
    fn hash_algorithm_name() {
        assert_eq!(HashAlgorithm::Blake3.as_str(), "blake3");
        assert_eq!(HashAlgorithm::default(), HashAlgorithm::Blake3);
    }

    #[test]
    fn block_hash_info_length() {
        let info = BlockHashInfo {
            content_hash: BlockHash::new([0xaau8; 32]),
            block_type: "heading".to_string(),
            start_offset: 0,
            end_offset: 50,
            algorithm: HashAlgorithm::Blake3,
        };

        assert_eq!(info.content_length(), 50);
        assert_eq!(info.hash(), BlockHash::new([0xaau8; 32]));
    }
}
