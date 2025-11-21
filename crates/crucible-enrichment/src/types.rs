//! Enrichment types for the infrastructure layer
//!
//! This module defines additional enrichment types needed by the implementation
//! that may have dependencies not suitable for the core domain layer.

use crucible_merkle::HybridMerkleTree;
use crucible_core::parser::ParsedNote;

// Re-export core enrichment types
pub use crucible_core::enrichment::{
    BlockEmbedding, EnrichedNote as CoreEnrichedNote, EmbeddingProvider,
    InferredRelation, NoteMetadata, RelationType,
};

/// Infrastructure-layer enriched note with Merkle tree
///
/// This type extends the core EnrichedNote with a Merkle tree for change detection.
/// The Merkle tree is stored separately to avoid circular dependencies between
/// core and merkle crates.
#[derive(Debug, Clone)]
pub struct EnrichedNoteWithTree {
    /// The core enriched note
    pub core: CoreEnrichedNote,

    /// Merkle tree for change detection
    pub merkle_tree: HybridMerkleTree,
}

impl EnrichedNoteWithTree {
    /// Create a new enriched note with tree
    pub fn new(
        parsed: ParsedNote,
        merkle_tree: HybridMerkleTree,
        embeddings: Vec<BlockEmbedding>,
        metadata: NoteMetadata,
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

// Re-export as EnrichedNote for backward compatibility within this crate
pub use EnrichedNoteWithTree as EnrichedNote;
