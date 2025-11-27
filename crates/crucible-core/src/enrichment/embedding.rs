//! Embedding provider abstractions for the enrichment pipeline
//!
//! This module defines the core trait for embedding providers, following
//! the Dependency Inversion Principle. The trait is defined in the core
//! domain layer, while concrete implementations live in the infrastructure
//! layer (crucible-llm).

use anyhow::Result;

/// Abstract interface for text embedding providers
///
/// This trait defines the minimal contract that embedding providers must
/// implement to work with the enrichment pipeline. Implementations are
/// provided in the crucible-llm crate (Fastembed, OpenAI, etc.).
///
/// # Dependency Inversion
///
/// By defining this trait in the core layer with minimal dependencies,
/// we allow the domain logic to depend on abstractions rather than
/// concrete implementations. The infrastructure layer (crucible-llm)
/// depends on the core layer and provides concrete implementations.
///
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for a single text input
    ///
    /// # Arguments
    ///
    /// * `text` - The text to embed
    ///
    /// # Returns
    ///
    /// A vector of floating-point values representing the semantic embedding.
    /// Typical dimensions range from 384 (smaller models) to 3072 (larger models).
    ///
    /// # Errors
    ///
    /// Returns an error if the embedding generation fails (network error,
    /// API error, authentication failure, etc.)
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts in a batch
    ///
    /// Batch operations are more efficient than individual calls when processing
    /// multiple texts, as they reduce network overhead and may benefit from
    /// provider-side optimizations.
    ///
    /// # Arguments
    ///
    /// * `texts` - A slice of text strings to embed
    ///
    /// # Returns
    ///
    /// A vector of embedding vectors, one for each input text.
    /// The order of embeddings matches the order of input texts.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the embeddings fail.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Get the name of the model being used
    ///
    /// # Returns
    ///
    /// The model name as a string slice (e.g., "nomic-embed-text",
    /// "text-embedding-3-small")
    fn model_name(&self) -> &str;

    /// Get the version of the model being used
    ///
    /// # Returns
    ///
    /// The model version if available (e.g., "q8_0" for quantized models,
    /// "v1.5" for versioned models). Returns None for providers that don't
    /// track model versions.
    fn model_version(&self) -> Option<&str> {
        None
    }

    /// Get the dimensionality of embeddings produced by this provider
    ///
    /// # Returns
    ///
    /// The number of dimensions in each embedding vector
    fn dimensions(&self) -> usize;
}

/// A cached embedding result
///
/// Contains the embedding vector along with metadata about how it was generated.
/// Used for incremental embedding where we can reuse embeddings for unchanged content.
#[derive(Debug, Clone)]
pub struct CachedEmbedding {
    /// The embedding vector
    pub vector: Vec<f32>,
    /// BLAKE3 hash of the content that was embedded
    pub content_hash: String,
    /// Model name used to generate this embedding
    pub model: String,
    /// Model version (e.g., "q8_0" for quantized models)
    pub model_version: Option<String>,
}

/// Abstract interface for embedding cache
///
/// This trait allows looking up previously generated embeddings by content hash,
/// enabling incremental embedding. If the same content (by BLAKE3 hash) has already
/// been embedded by the same model+version, we can reuse the cached embedding
/// instead of calling the embedding provider again.
///
/// # Dependency Inversion
///
/// The trait is defined in crucible-core (domain layer), while implementations
/// live in crucible-surrealdb (infrastructure layer). This allows the enrichment
/// service to use caching without depending on storage implementation details.
#[async_trait::async_trait]
pub trait EmbeddingCache: Send + Sync {
    /// Look up a cached embedding by content hash and model
    ///
    /// # Arguments
    ///
    /// * `content_hash` - BLAKE3 hash of the content
    /// * `model` - Model name (e.g., "nomic-embed-text-v1.5")
    /// * `model_version` - Model version (e.g., "q8_0"), or None for unversioned models
    ///
    /// # Returns
    ///
    /// * `Ok(Some(CachedEmbedding))` if a matching embedding exists
    /// * `Ok(None)` if no matching embedding is found
    /// * `Err(...)` if the lookup fails
    async fn get_embedding(
        &self,
        content_hash: &str,
        model: &str,
        model_version: Option<&str>,
    ) -> Result<Option<CachedEmbedding>>;
}
