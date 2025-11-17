//! Merkle Tree Implementation for Content-Addressed Storage
//!
//! This crate provides Merkle tree functionality for Crucible's content hashing
//! and incremental processing system.
//!
//! ## Purpose
//!
//! Pure transformation: `ParsedNote` → `HybridMerkleTree` + diff capability
//!
//! ## Architecture
//!
//! Following clean separation of concerns:
//! - This crate: Merkle tree building, diffing, hashing
//! - crucible-core: Storage traits (MerkleStore)
//! - Infrastructure crates: Persistence implementations
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crucible_merkle::HybridMerkleTree;
//! use crucible_parser::ParsedNote;
//!
//! // Build tree from parsed note
//! let tree = HybridMerkleTree::from_document(&parsed_note);
//!
//! // Compare with previous version
//! let diff = tree.diff(&old_tree);
//! let changed_blocks = diff.changed_block_ids();
//! ```

pub mod hash;
pub mod hybrid;
pub mod storage;
pub mod thread_safe;
pub mod virtual_section;

// Re-export main types
pub use hash::*;
pub use hybrid::*;
pub use storage::*;
pub use thread_safe::*;
pub use virtual_section::*;
