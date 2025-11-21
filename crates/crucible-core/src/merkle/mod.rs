//! Merkle Tree Abstractions
//!
//! This module defines traits for merkle tree operations without depending
//! on concrete implementations. Follows the Dependency Inversion Principle.

pub mod traits;

pub use traits::MerkleTreeBuilder;
