//! Enrichment Trait Definitions and Domain Types
//!
//! This module defines the core traits and types for the enrichment layer.
//! Actual implementations live in the `crucible-enrichment` crate.
//!
//! ## Dependency Inversion Principle (SOLID)
//!
//! By defining traits and domain types in the core layer:
//! - Core remains pure with no dependencies on concrete implementations
//! - Infrastructure depends on core, not vice versa
//! - Easy to swap implementations or add new providers
//! - High-level modules depend on abstractions, not concretions

pub mod embedding;
pub mod service;
pub mod storage;
pub mod types;

// Re-export the embedding provider trait and cache types
pub use embedding::{CachedEmbedding, EmbeddingCache, EmbeddingProvider};

// Re-export the enrichment service trait
pub use service::EnrichmentService;

// Re-export the enriched note storage trait
pub use storage::EnrichedNoteStore;

// Re-export domain types
pub use types::{BlockEmbedding, EnrichedNote, EnrichmentMetadata, InferredRelation, RelationType};

// Re-export configuration types from crucible-config to maintain backward compatibility
// Configuration lives in crucible-config to simplify dependency graph
pub use crucible_config::{
    embedding::EmbeddingProviderType, CohereConfig, CustomConfig, EmbeddingProviderConfig,
    EnrichmentConfig, FastEmbedConfig, MockConfig, OllamaConfig, OpenAIConfig, PipelineConfig,
    VertexAIConfig,
};
