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
//! ## Clean Architecture (SOLID Phase 5)
//!
//! - **Dependencies**: Depends on `crucible-core` (traits only) and `crucible-parser` (AST)
//! - **Inversion**: Uses `EmbeddingProvider` trait from `crucible-core`
//! - **Private Implementation**: `DefaultEnrichmentService` is private, use factory function
//! - **Pure functions**: Business logic is testable and reusable
//!
//! ## Usage
//!
//! ```rust,no_run
//! use crucible_enrichment::create_default_enrichment_service;
//! use crucible_core::enrichment::EnrichmentService;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Use factory function to create service
//!     let service = create_default_enrichment_service(None)?;
//!
//!     // Use via trait interface
//!     // service.enrich_note(...).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Modules
//!
//! - **types**: EnrichedNote and related types (public)
//! - **service**: EnrichmentService implementation (private, use factory)

pub mod event_handler;
pub mod types;

// Re-export enrichment types (domain types are public)
pub use types::{
    BlockEmbedding, EnrichedNoteWithTree, EnrichmentMetadata, InferredRelation, RelationType,
};

// Re-export event handler
pub use event_handler::EmbeddingHandler;

// Re-export constants (configuration values)
pub use service::{DEFAULT_MAX_BATCH_SIZE, DEFAULT_MIN_WORDS_FOR_EMBEDDING};

// PRIVATE: Service implementation - use factory function instead
pub(crate) mod service;

// Factory function - public API for creating the service
use crucible_core::enrichment::{EmbeddingProvider, EnrichmentService};
use crucible_merkle::HybridMerkleTreeBuilder;
use std::sync::Arc;

/// Create a DefaultEnrichmentService with optional embedding provider.
///
/// This is the public factory function for creating enrichment services.
/// It enforces dependency inversion by returning a trait object.
///
/// # Arguments
///
/// * `embedding_provider` - Optional embedding provider for semantic enrichment
///
/// # Returns
///
/// A trait object implementing `EnrichmentService`
pub fn create_default_enrichment_service(
    embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
) -> anyhow::Result<Arc<dyn EnrichmentService>> {
    let merkle_builder = HybridMerkleTreeBuilder;

    let service = if let Some(provider) = embedding_provider {
        service::DefaultEnrichmentService::new(merkle_builder, provider)
    } else {
        service::DefaultEnrichmentService::without_embeddings(merkle_builder)
    };

    Ok(Arc::new(service))
}
