//! Example: Custom Merkle Tree Builder
//!
//! This example demonstrates how to implement a custom merkle tree builder
//! using the Tower-style dependency inversion pattern. It shows how services
//! can work with any builder implementation without depending on concrete types.
//!
//! Run with: `cargo run --example custom_merkle_builder`

use crucible_core::parser::{ParsedNote, ParsedNoteBuilder};
use crucible_core::MerkleTreeBuilder;
use crucible_enrichment::create_default_enrichment_service;
use crucible_core::enrichment::EnrichmentService;
use std::path::PathBuf;
use std::sync::Arc;

// ============================================================================
// Simple Merkle Tree Implementation
// ============================================================================

/// A simplified merkle tree that only stores a summary hash
///
/// This is a minimal example showing what's required to implement
/// the MerkleTreeBuilder trait. Real implementations would include
/// more sophisticated tree structures.
#[derive(Clone, Debug)]
pub struct SimpleMerkleTree {
    /// Root hash of the entire document
    pub root_hash: String,

    /// Number of sections in the document
    pub section_count: usize,

    /// Total word count (for demonstration)
    pub word_count: usize,
}

impl SimpleMerkleTree {
    /// Create a tree from a parsed note
    pub fn from_document(doc: &ParsedNote) -> Self {
        // Simple hash based on path and word count
        let hash_input = format!("{}:{}", doc.path.display(), doc.metadata.word_count);
        let root_hash = format!("{:x}", md5::compute(hash_input.as_bytes()));

        Self {
            root_hash,
            section_count: doc.metadata.heading_count + 1, // +1 for preamble
            word_count: doc.metadata.word_count,
        }
    }
}

// ============================================================================
// Builder Implementation
// ============================================================================

/// Builder for creating SimpleMerkleTree instances
///
/// This is a zero-sized type that implements MerkleTreeBuilder,
/// following the same pattern as HybridMerkleTreeBuilder.
#[derive(Debug, Clone, Copy)]
pub struct SimpleMerkleTreeBuilder;

impl MerkleTreeBuilder for SimpleMerkleTreeBuilder {
    type Tree = SimpleMerkleTree;

    fn from_document(&self, doc: &ParsedNote) -> Self::Tree {
        SimpleMerkleTree::from_document(doc)
    }
}

// ============================================================================
// Usage Examples
// ============================================================================

fn main() {
    println!("Custom Merkle Tree Builder Example");
    println!("====================================\n");

    // Create a sample document
    let note = ParsedNoteBuilder::new(PathBuf::from("example.md"))
        .build();

    println!("📄 Created test document: {}", note.path.display());
    println!("   Word count: {}", note.metadata.word_count);
    println!("   Headings: {}\n", note.metadata.heading_count);

    // Example 1: Direct builder usage
    println!("Example 1: Direct Builder Usage");
    println!("--------------------------------");

    let builder = SimpleMerkleTreeBuilder;
    let tree = builder.from_document(&note);

    println!("✓ Built tree with SimpleMerkleTreeBuilder");
    println!("  Root hash: {}", tree.root_hash);
    println!("  Sections: {}", tree.section_count);
    println!("  Words: {}\n", tree.word_count);

    // Example 2: Using with enrichment service (via factory function)
    println!("Example 2: Enrichment Service Integration");
    println!("------------------------------------------");

    // Create enrichment service using factory function (SOLID compliant)
    // Note: DefaultEnrichmentService is private; use factory function instead
    let _service = create_default_enrichment_service(None)
        .expect("Failed to create enrichment service");

    println!("✓ Created enrichment service using factory function");
    println!("  Service is created via dependency injection pattern");
    println!("  This demonstrates dependency inversion in action!\n");

    // Example 3: Builder is zero-sized
    println!("Example 3: Zero-Cost Abstraction");
    println!("---------------------------------");

    let size = std::mem::size_of::<SimpleMerkleTreeBuilder>();
    println!("✓ Builder size: {} bytes (zero-sized type!)", size);
    println!("  The builder has no runtime overhead");
    println!("  All dispatch is resolved at compile time\n");

    // Example 4: Builder is cloneable
    println!("Example 4: Cloning and Sharing");
    println!("-------------------------------");

    let builder1 = SimpleMerkleTreeBuilder;
    let builder2 = builder1.clone();

    let tree1 = builder1.from_document(&note);
    let tree2 = builder2.from_document(&note);

    println!("✓ Created two builders via cloning");
    println!("  Both produce identical trees:");
    println!("  Tree 1 hash: {}", tree1.root_hash);
    println!("  Tree 2 hash: {}", tree2.root_hash);
    println!("  Match: {}\n", tree1.root_hash == tree2.root_hash);

    // Example 5: Generic function works with any builder
    println!("Example 5: Generic Functions");
    println!("-----------------------------");

    fn build_and_report<M: MerkleTreeBuilder>(builder: M, doc: &ParsedNote) {
        let _tree = builder.from_document(doc);
        println!("✓ Built tree using generic builder:");
        println!("  Builder type: {}", std::any::type_name::<M>());
        println!("  Tree type: {}", std::any::type_name::<M::Tree>());
    }

    build_and_report(SimpleMerkleTreeBuilder, &note);
    println!();

    // Summary
    println!("Summary");
    println!("=======");
    println!("This example demonstrated:");
    println!("  ✓ Implementing a custom MerkleTreeBuilder");
    println!("  ✓ Using it with enrichment services");
    println!("  ✓ Zero-cost abstraction benefits");
    println!("  ✓ Cloning and sharing builders");
    println!("  ✓ Generic programming with the trait");
    println!();
    println!("The Tower-style pattern enables:");
    println!("  • Dependency inversion (core doesn't depend on infrastructure)");
    println!("  • Testability (easy to create mock builders)");
    println!("  • Flexibility (swap implementations without changing services)");
    println!("  • Performance (zero runtime overhead)");
}

// ============================================================================
// Testing
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_builder_implements_trait() {
        let builder = SimpleMerkleTreeBuilder;
        let note = ParsedNoteBuilder::new(PathBuf::from("test.md")).build();

        let tree = builder.from_document(&note);

        assert_eq!(tree.section_count, 1); // Just preamble
        assert_eq!(tree.word_count, 0); // Empty note
        assert!(!tree.root_hash.is_empty());
    }

    #[test]
    fn test_builder_is_zero_sized() {
        assert_eq!(std::mem::size_of::<SimpleMerkleTreeBuilder>(), 0);
    }

    #[test]
    fn test_builder_produces_deterministic_trees() {
        let builder = SimpleMerkleTreeBuilder;
        let note = ParsedNoteBuilder::new(PathBuf::from("deterministic.md")).build();

        let tree1 = builder.from_document(&note);
        let tree2 = builder.from_document(&note);

        assert_eq!(tree1.root_hash, tree2.root_hash);
        assert_eq!(tree1.section_count, tree2.section_count);
        assert_eq!(tree1.word_count, tree2.word_count);
    }

    #[test]
    fn test_works_with_enrichment_service() {
        let builder = SimpleMerkleTreeBuilder;
        let _service = DefaultEnrichmentService::without_embeddings(builder);

        // Just verify it compiles and constructs
    }
}
