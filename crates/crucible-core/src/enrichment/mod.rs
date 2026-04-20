//! Enrichment domain types.
//!
//! Defines types for note enrichment (embeddings, metadata). The concrete
//! implementation lives in `crucible-daemon::enrichment` — there is one,
//! and one is enough.

pub mod embedding;
pub mod types;

pub use embedding::{CachedEmbedding, EmbeddingProvider};
pub use types::{BlockEmbedding, EnrichedNote, EnrichmentMetadata};

pub use crucible_config::{
    CohereConfig, CustomConfig, EmbeddingProviderConfig, EnrichmentConfig, FastEmbedConfig,
    MockConfig, OllamaConfig, OpenAIConfig, PipelineConfig, VertexAIConfig,
};
