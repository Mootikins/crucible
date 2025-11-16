//! Enrichment Types
//!
//! Re-exports enrichment types from crucible-core for convenience.
//! The actual domain types are now defined in crucible-core following
//! the Dependency Inversion Principle (SOLID).
//!
//! ## Architecture
//!
//! Domain types belong in the core layer (crucible-core) according to Clean
//! Architecture principles. This module simply re-exports them for backward
//! compatibility and convenience.

// Re-export all enrichment types from core
pub use crucible_core::enrichment::{
    BlockEmbedding, EnrichedNote, InferredRelation, NoteMetadata, RelationType,
};
