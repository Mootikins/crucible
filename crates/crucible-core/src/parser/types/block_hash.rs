//! Block hash type for content-addressed storage

use serde::{Deserialize, Serialize};

/// A BLAKE3 hash used for block-level content addressing
///
/// Similar to FileHash but specifically used for individual content blocks
/// extracted from documents (headings, paragraphs, code blocks, etc.).
///
/// This is the canonical definition of BlockHash in the Crucible system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash([u8; 32]);

impl BlockHash {
    /// Create a new BlockHash from raw bytes
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

    /// Create a BlockHash from a hex string
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let bytes = hex::decode(hex).map_err(|_| "Invalid hex format".to_string())?;
        if bytes.len() != 32 {
            return Err("Invalid hash length: expected 32 bytes".to_string());
        }
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes);
        Ok(Self(array))
    }

    /// Create a zero hash
    pub fn zero() -> Self {
        Self([0u8; 32])
    }

    /// Check if this is a zero hash
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; 32]
    }
}

impl Default for BlockHash {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<[u8; 32]> for BlockHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self::new(bytes)
    }
}

impl From<&[u8; 32]> for BlockHash {
    fn from(bytes: &[u8; 32]) -> Self {
        Self::new(*bytes)
    }
}

impl std::fmt::Display for BlockHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl std::str::FromStr for BlockHash {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}
