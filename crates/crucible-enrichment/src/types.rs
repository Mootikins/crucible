//! Enrichment types for the infrastructure layer
//!
//! This module defines additional enrichment types needed by the implementation
//! that may have dependencies not suitable for the core domain layer.

use crucible_core::parser::ParsedNote;

// Re-export core enrichment types
pub use crucible_core::enrichment::{
    BlockEmbedding, EmbeddingProvider, EnrichedNote as CoreEnrichedNote, EnrichmentMetadata,
    InferredRelation, RelationType,
};

/// Infrastructure-layer enriched note with Merkle tree
///
/// This type extends the core EnrichedNote with a Merkle tree for change detection.
/// The generic `T` parameter allows using any merkle tree implementation.
///
/// ## Generic Pattern
///
/// This follows the Tower-style generic pattern where the tree type is determined
/// by the `MerkleTreeBuilder` used by the enrichment service.
#[derive(Debug, Clone)]
pub struct EnrichedNoteWithTree<T: Clone + Send + Sync> {
    /// The core enriched note
    pub core: CoreEnrichedNote,

    /// Merkle tree for change detection (generic over tree type)
    pub merkle_tree: T,
}

impl<T: Clone + Send + Sync> EnrichedNoteWithTree<T> {
    /// Create a new enriched note with tree
    pub fn new(
        parsed: ParsedNote,
        merkle_tree: T,
        embeddings: Vec<BlockEmbedding>,
        metadata: EnrichmentMetadata,
        inferred_relations: Vec<InferredRelation>,
    ) -> Self {
        Self {
            core: CoreEnrichedNote::new(parsed, embeddings, metadata, inferred_relations),
            merkle_tree,
        }
    }

    /// Get the note path
    pub fn path(&self) -> &std::path::Path {
        self.core.path()
    }

    /// Get the note ID
    pub fn id(&self) -> String {
        self.core.id()
    }
}
