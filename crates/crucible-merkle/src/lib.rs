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

pub mod builder;
pub mod hash;
pub mod hybrid;
pub mod storage;
pub mod thread_safe;
pub mod virtual_section;

// Re-export main types
pub use builder::*;
pub use hash::*;
pub use hybrid::*;
pub use storage::*;
pub use thread_safe::*;
pub use virtual_section::*;
