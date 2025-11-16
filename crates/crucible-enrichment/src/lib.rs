//! # Crucible Enrichment
//!
//! Enrichment layer for the Crucible knowledge management system.
//!
//! This crate provides enrichment operations for parsed notes including:
//! - **Embedding generation**: Vector embeddings for semantic search
//! - **Metadata extraction**: Word counts, complexity scoring, reading time
//! - **Relation inference**: Semantic similarity, clustering (future)
//! - **Breadcrumb computation**: Heading hierarchy for context
//!
//! ## Architecture
//!
//! The enrichment layer sits between parsing and storage:
//! 1. Receives `ParsedNote` from `crucible-parser`
//! 2. Enriches with embeddings, metadata, and relations
//! 3. Returns `EnrichedNote` for storage in database
//!
//! ## Clean Architecture
//!
//! - **Dependencies**: Depends on `crucible-core` (traits only) and `crucible-parser` (AST)
//! - **Inversion**: Uses `EmbeddingProvider` trait from `crucible-core`
//! - **Pure functions**: Business logic is testable and reusable
//!
//! ## Modules
//!
//! - **config**: Configuration types for enrichment operations
//! - **types**: EnrichedNote and related types
//! - **service**: EnrichmentService orchestrator

pub mod config;
pub mod document_processor;
pub mod service;
pub mod types;

// Re-export commonly used configuration types
pub use config::{
    BatchIncrementalResult, DocumentEmbedding, EmbeddingConfig, EmbeddingError,
    EmbeddingErrorType, EmbeddingModel, EmbeddingProcessingResult, IncrementalProcessingResult,
    PrivacyMode, RetryProcessingResult, ThreadPoolMetrics, validate_embedding_config,
};

// Re-export enrichment types (now defined in crucible-core)
pub use types::{
    BlockEmbedding, EnrichedNote, InferredRelation, NoteMetadata, RelationType,
};

// Re-export service
pub use service::{DefaultEnrichmentService, DEFAULT_MAX_BATCH_SIZE, DEFAULT_MIN_WORDS_FOR_EMBEDDING};

// Re-export document processor
pub use document_processor::{
    DocumentProcessor, DocumentProcessingResult, ProcessingMetrics, ProcessorConfig,
};
