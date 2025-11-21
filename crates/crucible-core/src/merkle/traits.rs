//! Merkle Tree Builder Trait
//!
//! Defines the contract for building merkle trees from parsed notes.
//! Following the Dependency Inversion Principle: core defines the abstraction,
//! infrastructure provides concrete implementations.
//!
//! ## Design Pattern
//!
//! This follows the **Generic Service with Builder Trait** pattern (Tower-style):
//! - Trait defines how to build a merkle tree
//! - Associated type specifies what kind of tree is produced
//! - Services can be generic over this trait
//! - Zero-cost abstraction at compile time
//!
//! Inspired by Tower's Service trait and Diesel's Backend trait patterns.

use crate::parser::ParsedNote;

/// Trait for building merkle trees from parsed notes
///
/// This abstraction enables services (like enrichment) to use merkle trees
/// without depending on concrete implementations from infrastructure crates.
///
/// ## Example
///
/// ```rust,ignore
/// // Infrastructure implementation
/// impl MerkleTreeBuilder for HybridMerkleTreeBuilder {
///     type Tree = HybridMerkleTree;
///
///     fn from_document(&self, doc: &ParsedNote) -> Self::Tree {
///         HybridMerkleTree::from_document(doc)
///     }
/// }
///
/// // Service using the builder
/// struct EnrichmentService<M: MerkleTreeBuilder> {
///     merkle_builder: M,
///     // ... other fields
/// }
/// ```
pub trait MerkleTreeBuilder: Send + Sync + Clone {
    /// The type of merkle tree produced by this builder
    type Tree: Clone + Send + Sync;

    /// Build a merkle tree from a parsed document
    ///
    /// # Arguments
    ///
    /// * `doc` - The parsed note to build the tree from
    ///
    /// # Returns
    ///
    /// A merkle tree representing the document's structure and content hashes
    fn from_document(&self, doc: &ParsedNote) -> Self::Tree;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Mock builder for testing
    #[derive(Clone)]
    struct MockTreeBuilder;

    #[derive(Clone)]
    struct MockTree {
        word_count: usize,
    }

    impl MerkleTreeBuilder for MockTreeBuilder {
        type Tree = MockTree;

        fn from_document(&self, doc: &ParsedNote) -> Self::Tree {
            MockTree {
                word_count: doc.metadata.word_count,
            }
        }
    }

    #[test]
    fn test_builder_trait_basics() {
        let builder = MockTreeBuilder;
        let note = crate::parser::ParsedNoteBuilder::new(PathBuf::from("test.md")).build();

        let tree = builder.from_document(&note);
        assert_eq!(tree.word_count, 0); // Empty note
    }

    #[test]
    fn test_builder_is_cloneable() {
        let builder = MockTreeBuilder;
        let _cloned = builder.clone();
        // Just verify it compiles
    }
}
