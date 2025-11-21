//! Merkle Tree Builder Implementation
//!
//! Implements the `MerkleTreeBuilder` trait from crucible-core for `HybridMerkleTree`.
//! This enables dependency injection and allows services to use merkle trees
//! without depending on concrete types.

use crate::HybridMerkleTree;
use crucible_core::parser::ParsedNote;
use crucible_core::MerkleTreeBuilder;

/// Builder for creating `HybridMerkleTree` instances
///
/// This is a zero-sized type that implements the `MerkleTreeBuilder` trait,
/// enabling dependency injection in services like enrichment.
///
/// ## Example
///
/// ```rust
/// use crucible_merkle::HybridMerkleTreeBuilder;
/// use crucible_core::MerkleTreeBuilder;
/// use std::path::PathBuf;
///
/// let builder = HybridMerkleTreeBuilder;
/// let note = crucible_core::parser::ParsedNoteBuilder::new(PathBuf::from("test.md")).build();
/// let tree = builder.from_document(&note);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct HybridMerkleTreeBuilder;

impl MerkleTreeBuilder for HybridMerkleTreeBuilder {
    type Tree = HybridMerkleTree;

    fn from_document(&self, doc: &ParsedNote) -> Self::Tree {
        HybridMerkleTree::from_document(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_builder_creates_tree() {
        let builder = HybridMerkleTreeBuilder;
        let note = crucible_core::parser::ParsedNoteBuilder::new(PathBuf::from("test.md")).build();

        let tree = builder.from_document(&note);

        // Verify tree was created (empty note has preamble section)
        assert!(tree.section_count() >= 1);
        assert_eq!(tree.total_blocks, 0);
    }

    #[test]
    fn test_builder_is_zero_sized() {
        // Verify builder is zero-sized (compile-time check)
        assert_eq!(std::mem::size_of::<HybridMerkleTreeBuilder>(), 0);
    }

    #[test]
    fn test_builder_is_copy() {
        let builder = HybridMerkleTreeBuilder;
        let _copy = builder; // Should work because it's Copy
        let _another = builder; // Original should still be usable
    }
}
