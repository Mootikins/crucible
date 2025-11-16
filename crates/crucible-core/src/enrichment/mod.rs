//! Enrichment Trait Definitions
//!
//! This module defines the core traits for the enrichment layer.
//! Actual implementations live in the `crucible-enrichment` crate.
//!
//! ## Dependency Inversion Principle
//!
//! By defining traits in the core layer and implementations in infrastructure:
//! - Core remains pure with no dependencies on concrete implementations
//! - Infrastructure depends on core, not vice versa
//! - Easy to swap implementations or add new providers

pub mod config;
pub mod embedding;

// Re-export the embedding provider trait
pub use embedding::EmbeddingProvider;

// Re-export configuration types
pub use config::{
    CohereConfig, CustomConfig, EmbeddingProviderConfig, EnrichmentConfig, FastEmbedConfig,
    MockConfig, OllamaConfig, OpenAIConfig, PipelineConfig, VertexAIConfig,
};
