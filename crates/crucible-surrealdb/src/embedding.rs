//! Embedding Storage Module
//!
//! Pure storage operations for vector embeddings. This module is intentionally
//! minimal - all embedding generation and enrichment logic lives in the
//! crucible-enrichment crate.
//!
//! ## Architecture
//!
//! Following clean architecture principles:
//! - This module: Pure I/O (store, retrieve, delete, search)
//! - crucible-enrichment: Business logic (generation, orchestration)
//! - Clear separation of concerns

use std::sync::Arc;
use anyhow::Result;
use async_trait::async_trait;

use crate::SurrealClient;

// Re-export storage functions from kiln_integration
pub use crate::kiln_integration::{
    clear_document_embeddings,
    get_database_stats,
    get_document_embeddings,
    get_embedding_by_content_hash,
    semantic_search,
    store_document_embedding,
    CachedEmbedding,
};

/// SurrealDB implementation of the EmbeddingCache trait
///
/// Wraps a SurrealDB client to provide embedding cache lookups.
/// This enables incremental embedding by checking if content has
/// already been embedded before calling the embedding provider.
pub struct SurrealEmbeddingCache {
    client: Arc<SurrealClient>,
}

impl SurrealEmbeddingCache {
    /// Create a new embedding cache backed by SurrealDB
    pub fn new(client: Arc<SurrealClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl crucible_core::enrichment::EmbeddingCache for SurrealEmbeddingCache {
    async fn get_embedding(
        &self,
        content_hash: &str,
        model: &str,
        model_version: Option<&str>,
    ) -> Result<Option<crucible_core::enrichment::CachedEmbedding>> {
        // Use the kiln_integration lookup function
        let result = get_embedding_by_content_hash(
            &self.client,
            content_hash,
            model,
            model_version.unwrap_or(""),
        ).await?;

        // Convert from our internal CachedEmbedding to the core type
        Ok(result.map(|cached| crucible_core::enrichment::CachedEmbedding {
            vector: cached.vector,
            content_hash: cached.content_hash,
            model: cached.model,
            model_version: Some(cached.model_version),
        }))
    }
}
