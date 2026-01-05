//! Content Hashing Module
//!
//! This module provides content hashing implementations for block-level integrity
//! verification and change detection.
//!
//! ## Module Structure
//!
//! - [`algorithm`]: Trait-based algorithm abstraction (OCP-compliant)
//! - [`file_hasher`]: File-based content hashing implementation
//! - [`blake3`]: BLAKE3 algorithm implementation
//! - [`sha256`]: SHA256 algorithm implementation
//! - [`normalization`]: Block text normalization
//! - [`block_link`]: Block hash utilities

pub mod algorithm;
pub mod blake3;
pub mod block_link;
pub mod file_hasher;
pub mod normalization;
pub mod sha256;

// Re-export main types for convenience
pub use algorithm::{Blake3Algorithm, HashingAlgorithm, Sha256Algorithm};
pub use blake3::{Blake3Hasher, BLAKE3_CONTENT_HASHER};
pub use block_link::{compute_block_hash, hash_to_hex, hex_to_hash};
pub use file_hasher::{
    Blake3FileHasher, FileHasher, Sha256FileHasher, BLAKE3_HASHER, SHA256_HASHER,
};
pub use normalization::normalize_block_text;
pub use sha256::{SHA256Hasher, SHA256_CONTENT_HASHER};
