//! Embedding provider abstractions for the enrichment pipeline
//!
//! This module defines the canonical trait for embedding providers. All
//! embedding implementations (Ollama, FastEmbed, OpenAI, Burn, etc.) implement
//! this trait directly. There is no adapter layer.
//!
//! # Dependency Inversion
//!
//! By defining this trait in the core layer with minimal dependencies,
//! we allow the domain logic to depend on abstractions rather than
//! concrete implementations. The infrastructure layer (crucible-llm)
//! depends on the core layer and provides concrete implementations.

use anyhow::Result;

/// Canonical interface for text embedding providers
///
/// This trait defines the full contract that embedding providers must implement.
/// It supports single and batch embedding, metadata queries, health checking,
/// and model discovery.
///
/// Implementations are provided in the crucible-llm crate (FastEmbed, Ollama,
/// OpenAI, Burn, etc.).
///
/// # Object Safety
///
/// This trait is object-safe, meaning it can be used as `Arc<dyn EmbeddingProvider>`
/// for dynamic dispatch. This allows different providers to be swapped at runtime
/// based on configuration.
///
/// # Async Methods
///
/// All embedding methods are async to support non-blocking I/O operations when
/// communicating with remote embedding APIs.
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

    /// Get the name of the embedding provider
    ///
    /// # Returns
    ///
    /// The provider name as a string slice (e.g., "Ollama", "FastEmbed", "OpenAI")
    fn provider_name(&self) -> &str;

    /// Check if the provider is healthy/available
    ///
    /// # Returns
    ///
    /// `Ok(true)` if the provider is healthy, `Ok(false)` if it's not responding,
    /// or an error if the health check cannot be performed.
    async fn health_check(&self) -> Result<bool> {
        // Default implementation: try to embed a test string
        match self.embed("test").await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// List available model names from this provider
    ///
    /// Queries the provider to discover what embedding models are available.
    /// Returns model identifiers as strings. For richer metadata, use
    /// provider-specific APIs in crucible-llm.
    ///
    /// # Returns
    ///
    /// A vector of model name strings
    ///
    /// # Errors
    ///
    /// Returns an error if model discovery fails or is not supported.
    async fn list_models(&self) -> Result<Vec<String>>;
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
/// live in the storage layer (crucible-sqlite, crucible-lance). This allows the enrichment
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
