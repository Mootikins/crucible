//! Content Hashing Module
//!
//! This module provides content hashing implementations for Merkle tree construction
//! and block-level integrity verification. It includes the new ContentHasher trait
//! implementation for file system operations as part of the architectural refactoring.
//!
//! ## Module Structure
//!
//! - [`algorithm`]: Trait-based algorithm abstraction (OCP-compliant)
//! - [`file_hasher`]: File-based content hashing implementation
//! - [`block_hasher`]: AST block hashing implementation for Phase 2
//! - [`ast_converter`]: AST block to HashedBlock conversion (SRP-compliant)
//! - [`blake3`]: BLAKE3 algorithm implementation
//! - [`sha256`]: SHA256 algorithm implementation

pub mod algorithm;
pub mod ast_converter;
pub mod blake3;
pub mod block_hasher;
pub mod file_hasher;
pub mod sha256;

// Re-export main types for convenience
pub use algorithm::{HashingAlgorithm, Blake3Algorithm, Sha256Algorithm};
pub use ast_converter::{
    ASTBlockConverter, ConversionStats,
    Blake3ASTBlockConverter, Sha256ASTBlockConverter,
};
pub use blake3::{Blake3Hasher, BLAKE3_CONTENT_HASHER};
#[allow(deprecated)] // Re-exporting for backward compatibility
pub use block_hasher::{
    BlockHasher, BlockHashStats,
    Blake3BlockHasher, Sha256BlockHasher,
    new_blake3_block_hasher, new_sha256_block_hasher,
    BLAKE3_BLOCK_HASHER, SHA256_BLOCK_HASHER,
};
pub use file_hasher::{
    FileHasher,
    Blake3FileHasher, Sha256FileHasher,
    BLAKE3_HASHER, SHA256_HASHER
};
pub use sha256::{SHA256Hasher, SHA256_CONTENT_HASHER};

// Re-export Merkle tree types from storage module
pub use crate::storage::merkle::{MerkleTree, MerkleNode, TreeChange};