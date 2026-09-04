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

    /// The backend this provider speaks to, such as `fastembed` or `ollama`.
    ///
    /// A stored vector records `<provider_kind>/<model_name>`, because a model
    /// name alone is ambiguous across backends: Ollama's `nomic-embed-text`
    /// and fastembed's `nomic-embed-text-v1.5` are different models, and two
    /// backends may even serve the same name with different weights. The pair
    /// is the reuse key, so a vector is only ever reused for the backend that
    /// produced it.
    ///
    /// Required rather than defaulted: a new provider must answer for itself.
    fn provider_kind(&self) -> &'static str;

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
