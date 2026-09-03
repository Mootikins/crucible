//! Enrichment domain types.
//!
//! Defines types for note enrichment (embeddings, metadata). The concrete
//! implementation lives in `crucible-daemon::enrichment` — there is one,
//! and one is enough.

pub mod embedding;
pub mod eval;
pub mod geometry;
pub mod types;

pub use embedding::EmbeddingProvider;
pub use eval::{GoldenQuery, GoldenSet};
pub use types::{BlockEmbedding, EnrichedNote, EnrichmentMetadata};

pub use crate::config::{
    EmbeddingProviderConfig, EnrichmentConfig, FastEmbedConfig, MockConfig, OllamaConfig,
    OpenAIConfig, PipelineConfig,
};
