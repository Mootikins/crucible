//! Enrichment Layer
//!
//! This module provides the enrichment orchestration layer for the Crucible knowledge management system.
//! It coordinates embedding generation, metadata extraction, and relation inference following clean
//! architecture principles with proper separation of concerns.
//!
//! ## Architecture
//!
//! The enrichment layer sits between parsing and storage:
//! - **Input**: ParsedNote (from parser) + changed block IDs (from Merkle diff)
//! - **Processing**: Parallel enrichment operations (embeddings, metadata, relations)
//! - **Output**: EnrichedNote (ready for storage)
//!
//! ## Components
//!
//! - **config**: Configuration types for embedding enrichment operations
//! - **types**: EnrichedNote and related types
//! - **service**: EnrichmentService orchestrator (future)
//! - **metadata**: Metadata extraction (future)
//! - **relations**: Relation inference (future)

pub mod config;
pub mod types;

// Re-export commonly used configuration types
pub use config::{
    BatchIncrementalResult, DocumentEmbedding, EmbeddingConfig, EmbeddingError,
    EmbeddingErrorType, EmbeddingModel, EmbeddingProcessingResult, IncrementalProcessingResult,
    PrivacyMode, RetryProcessingResult, ThreadPoolMetrics, validate_embedding_config,
};

// Re-export enrichment types
pub use types::{
    BlockEmbedding, EnrichedNote, InferredRelation, InferredRelationType, NoteMetadata,
};
