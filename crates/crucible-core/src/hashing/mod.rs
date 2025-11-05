//! Content Hashing Module
//!
//! This module provides content hashing implementations for Merkle tree construction
//! and block-level integrity verification.

pub mod blake3;
pub mod sha256;

// Re-export main types for convenience
pub use blake3::{Blake3Hasher, BLAKE3_CONTENT_HASHER};
pub use sha256::{SHA256Hasher, SHA256_CONTENT_HASHER};

// Re-export Merkle tree types from storage module
pub use crate::storage::merkle::{MerkleTree, MerkleNode, TreeChange};