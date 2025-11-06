//! Content Hashing Module
//!
//! This module provides content hashing implementations for Merkle tree construction
//! and block-level integrity verification. It includes the new ContentHasher trait
//! implementation for file system operations as part of the architectural refactoring.
//!
//! ## Module Structure
//!
//! - [`file_hasher`]: File-based content hashing implementation
//! - [`block_hasher`]: AST block hashing implementation for Phase 2
//! - [`blake3`]: BLAKE3 algorithm implementation
//! - [`sha256`]: SHA256 algorithm implementation

pub mod blake3;
pub mod block_hasher;
pub mod file_hasher;
pub mod sha256;

// Re-export main types for convenience
pub use blake3::{Blake3Hasher, BLAKE3_CONTENT_HASHER};
pub use block_hasher::{BlockHasher, BlockHashStats, BLAKE3_BLOCK_HASHER, SHA256_BLOCK_HASHER};
pub use file_hasher::{FileHasher, BLAKE3_HASHER, SHA256_HASHER};
pub use sha256::{SHA256Hasher, SHA256_CONTENT_HASHER};

// Re-export Merkle tree types from storage module
pub use crate::storage::merkle::{MerkleTree, MerkleNode, TreeChange};