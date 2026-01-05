//! Enrichment types for the infrastructure layer
//!
//! This module defines additional enrichment types needed by the implementation
//! that may have dependencies not suitable for the core domain layer.

// Re-export core enrichment types
pub use crucible_core::enrichment::{
    BlockEmbedding, EmbeddingProvider, EnrichedNote, EnrichmentMetadata, InferredRelation,
    RelationType,
};
