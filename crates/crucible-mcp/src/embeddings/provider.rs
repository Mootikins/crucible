// crates/crucible-mcp/src/embeddings/provider.rs

//! Provider trait and types for text embedding generation
//!
//! This module defines the core trait that all embedding providers must implement,
//! along with the response types returned by embedding operations.
//!
//! # Example
//!
//! ```rust,no_run
//! use crucible_mcp::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = EmbeddingConfig::default();
//! let provider: Arc<dyn EmbeddingProvider> = create_provider(config).await?;
//!
//! // Single text embedding
//! let response = provider.embed("Hello, world!").await?;
//! println!("Embedding dimensions: {}", response.dimensions);
//! println!("Model: {}", response.model);
//!
//! // Batch embedding
//! let texts = vec!["First text".to_string(), "Second text".to_string()];
//! let responses = provider.embed_batch(texts).await?;
//! println!("Generated {} embeddings", responses.len());
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::error::EmbeddingResult;

/// Response from an embedding API containing the generated vector
///
/// This struct encapsulates the embedding vector along with metadata about
/// the model used and the dimensionality of the embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// The embedding vector as a dense array of floating-point values
    ///
    /// Typical dimensions range from 384 (smaller models) to 3072 (larger models).
    /// These vectors can be compared using cosine similarity, dot product, or
    /// Euclidean distance for semantic search operations.
    pub embedding: Vec<f32>,

    /// The name of the model that generated this embedding
    ///
    /// Examples: "nomic-embed-text", "text-embedding-3-small", "text-embedding-ada-002"
    pub model: String,

    /// The dimensionality of the embedding vector
    ///
    /// This should always equal `embedding.len()`. It's provided separately
    /// for convenience and to allow validation of expected dimensions.
    pub dimensions: usize,

    /// Number of tokens in the input text (optional)
    pub tokens: Option<usize>,

    /// Additional metadata from the provider (optional)
    pub metadata: Option<serde_json::Value>,
}

impl EmbeddingResponse {
    /// Create a new embedding response
    ///
    /// # Arguments
    ///
    /// * `embedding` - The embedding vector
    /// * `model` - The model name that generated the embedding
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_mcp::embeddings::EmbeddingResponse;
    ///
    /// let embedding = vec![0.1, 0.2, 0.3, 0.4];
    /// let response = EmbeddingResponse::new(embedding, "test-model".to_string());
    /// assert_eq!(response.dimensions, 4);
    /// ```
    pub fn new(embedding: Vec<f32>, model: String) -> Self {
        let dimensions = embedding.len();
        Self {
            embedding,
            model,
            dimensions,
            tokens: None,
            metadata: None,
        }
    }

    /// Add token count information
    pub fn with_tokens(mut self, tokens: usize) -> Self {
        self.tokens = Some(tokens);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Validate that the embedding dimensions match expected dimensions
    ///
    /// # Arguments
    ///
    /// * `expected` - The expected number of dimensions
    ///
    /// # Returns
    ///
    /// `Ok(())` if dimensions match, `Err` if they don't
    ///
    /// # Example
    ///
    /// ```rust
    /// use crucible_mcp::embeddings::EmbeddingResponse;
    ///
    /// let response = EmbeddingResponse::new(vec![0.1; 768], "nomic-embed-text".to_string());
    /// assert!(response.validate_dimensions(768).is_ok());
    /// assert!(response.validate_dimensions(1536).is_err());
    /// ```
    pub fn validate_dimensions(&self, expected: usize) -> EmbeddingResult<()> {
        if self.dimensions != expected {
            return Err(super::error::EmbeddingError::InvalidDimensions {
                expected,
                actual: self.dimensions,
            });
        }
        Ok(())
    }

    /// Calculate cosine similarity with another embedding
    ///
    /// Returns a value between -1.0 and 1.0, where 1.0 means identical
    /// embeddings and -1.0 means opposite embeddings.
    ///
    /// # Panics
    ///
    /// Panics if the embeddings have different dimensions
    pub fn cosine_similarity(&self, other: &EmbeddingResponse) -> f32 {
        assert_eq!(
            self.dimensions, other.dimensions,
            "Cannot calculate cosine similarity for embeddings with different dimensions"
        );

        let dot_product: f32 = self
            .embedding
            .iter()
            .zip(other.embedding.iter())
            .map(|(a, b)| a * b)
            .sum();

        let norm_a: f32 = self.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = other.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }

        dot_product / (norm_a * norm_b)
    }
}

/// Trait for embedding providers
///
/// This trait defines the interface that all embedding providers must implement.
/// It supports both single and batch embedding operations, and provides metadata
/// about the provider and model being used.
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
///
/// # Example Implementation
///
/// ```rust,no_run
/// use async_trait::async_trait;
/// use crucible_mcp::embeddings::{EmbeddingProvider, EmbeddingResponse, EmbeddingResult};
///
/// struct MyProvider {
///     model: String,
///     dimensions: usize,
/// }
///
/// #[async_trait]
/// impl EmbeddingProvider for MyProvider {
///     async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
///         // Implementation here
///         todo!()
///     }
///
///     async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
///         // Default implementation can iterate over single embeds
///         let mut results = Vec::new();
///         for text in texts {
///             results.push(self.embed(&text).await?);
///         }
///         Ok(results)
///     }
///
///     fn model_name(&self) -> &str {
///         &self.model
///     }
///
///     fn dimensions(&self) -> usize {
///         self.dimensions
///     }
///
///     fn provider_name(&self) -> &str {
///         "MyProvider"
///     }
/// }
/// ```
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding for a single text input
    ///
    /// # Arguments
    ///
    /// * `text` - The text to embed
    ///
    /// # Returns
    ///
    /// An `EmbeddingResponse` containing the embedding vector and metadata
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The API request fails
    /// - Authentication fails
    /// - Rate limits are exceeded
    /// - The response is invalid or malformed
    /// - A timeout occurs
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use crucible_mcp::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EmbeddingConfig::default();
    /// let provider = create_provider(config).await?;
    ///
    /// let response = provider.embed("Search query text").await?;
    /// println!("Generated embedding with {} dimensions", response.dimensions);
    /// # Ok(())
    /// # }
    /// ```
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse>;

    /// Generate embeddings for multiple texts in a single batch
    ///
    /// Batch operations are more efficient than individual calls when processing
    /// multiple texts, as they reduce network overhead and may benefit from
    /// provider-side optimizations.
    ///
    /// # Arguments
    ///
    /// * `texts` - A vector of texts to embed
    ///
    /// # Returns
    ///
    /// A vector of `EmbeddingResponse` objects, one for each input text.
    /// The order of responses matches the order of input texts.
    ///
    /// # Errors
    ///
    /// Returns an error if any of the embeddings fail. The behavior on partial
    /// failure depends on the provider implementation - some may return all
    /// successful embeddings, while others may fail the entire batch.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use crucible_mcp::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EmbeddingConfig::default();
    /// let provider = create_provider(config).await?;
    ///
    /// let texts = vec![
    ///     "First document".to_string(),
    ///     "Second document".to_string(),
    ///     "Third document".to_string(),
    /// ];
    ///
    /// let responses = provider.embed_batch(texts).await?;
    /// assert_eq!(responses.len(), 3);
    /// # Ok(())
    /// # }
    /// ```
    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>>;

    /// Get the name of the model being used
    ///
    /// # Returns
    ///
    /// The model name as a string slice
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use crucible_mcp::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EmbeddingConfig::default();
    /// let provider = create_provider(config).await?;
    ///
    /// println!("Using model: {}", provider.model_name());
    /// # Ok(())
    /// # }
    /// ```
    fn model_name(&self) -> &str;

    /// Get the dimensionality of embeddings produced by this provider
    ///
    /// # Returns
    ///
    /// The number of dimensions in the embedding vectors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use crucible_mcp::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EmbeddingConfig::default();
    /// let provider = create_provider(config).await?;
    ///
    /// println!("Embedding dimensions: {}", provider.dimensions());
    /// # Ok(())
    /// # }
    /// ```
    fn dimensions(&self) -> usize;

    /// Get the name of the embedding provider
    ///
    /// # Returns
    ///
    /// The provider name as a string slice (e.g., "Ollama", "OpenAI")
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use crucible_mcp::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EmbeddingConfig::default();
    /// let provider = create_provider(config).await?;
    ///
    /// println!("Provider: {}", provider.provider_name());
    /// # Ok(())
    /// # }
    /// ```
    fn provider_name(&self) -> &str;

    /// Check if the provider is healthy/available
    ///
    /// # Returns
    ///
    /// `Ok(true)` if the provider is healthy, `Ok(false)` if it's not responding,
    /// or an error if the health check cannot be performed.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use crucible_mcp::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EmbeddingConfig::default();
    /// let provider = create_provider(config).await?;
    ///
    /// if provider.health_check().await? {
    ///     println!("Provider is healthy");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    async fn health_check(&self) -> EmbeddingResult<bool> {
        // Default implementation: try to embed a test string
        match self.embed("test").await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_response_creation() {
        let embedding = vec![0.1, 0.2, 0.3, 0.4];
        let response = EmbeddingResponse::new(embedding.clone(), "test-model".to_string());

        assert_eq!(response.embedding, embedding);
        assert_eq!(response.model, "test-model");
        assert_eq!(response.dimensions, 4);
        assert!(response.tokens.is_none());
        assert!(response.metadata.is_none());
    }

    #[test]
    fn test_embedding_response_with_tokens() {
        let embedding = vec![0.1, 0.2, 0.3];
        let response = EmbeddingResponse::new(embedding, "test-model".to_string())
            .with_tokens(10);

        assert_eq!(response.tokens, Some(10));
        assert_eq!(response.dimensions, 3);
    }

    #[test]
    fn test_embedding_response_with_metadata() {
        let embedding = vec![0.1, 0.2];
        let metadata = serde_json::json!({"provider": "test"});
        let response = EmbeddingResponse::new(embedding, "test-model".to_string())
            .with_metadata(metadata.clone());

        assert_eq!(response.metadata, Some(metadata));
        assert_eq!(response.dimensions, 2);
    }

    #[test]
    fn test_validate_dimensions() {
        let response = EmbeddingResponse::new(vec![0.1; 768], "test-model".to_string());

        assert!(response.validate_dimensions(768).is_ok());
        assert!(response.validate_dimensions(1536).is_err());
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let embedding1 = EmbeddingResponse::new(vec![1.0, 0.0, 0.0], "test".to_string());
        let embedding2 = EmbeddingResponse::new(vec![1.0, 0.0, 0.0], "test".to_string());

        let similarity = embedding1.cosine_similarity(&embedding2);
        assert!((similarity - 1.0).abs() < 1e-6, "Expected 1.0, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let embedding1 = EmbeddingResponse::new(vec![1.0, 0.0, 0.0], "test".to_string());
        let embedding2 = EmbeddingResponse::new(vec![0.0, 1.0, 0.0], "test".to_string());

        let similarity = embedding1.cosine_similarity(&embedding2);
        assert!(similarity.abs() < 1e-6, "Expected 0.0, got {}", similarity);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let embedding1 = EmbeddingResponse::new(vec![1.0, 0.0, 0.0], "test".to_string());
        let embedding2 = EmbeddingResponse::new(vec![-1.0, 0.0, 0.0], "test".to_string());

        let similarity = embedding1.cosine_similarity(&embedding2);
        assert!((similarity + 1.0).abs() < 1e-6, "Expected -1.0, got {}", similarity);
    }

    #[test]
    #[should_panic(expected = "Cannot calculate cosine similarity for embeddings with different dimensions")]
    fn test_cosine_similarity_different_dimensions() {
        let embedding1 = EmbeddingResponse::new(vec![1.0, 0.0], "test".to_string());
        let embedding2 = EmbeddingResponse::new(vec![1.0, 0.0, 0.0], "test".to_string());

        embedding1.cosine_similarity(&embedding2);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let embedding1 = EmbeddingResponse::new(vec![0.0, 0.0, 0.0], "test".to_string());
        let embedding2 = EmbeddingResponse::new(vec![1.0, 0.0, 0.0], "test".to_string());

        let similarity = embedding1.cosine_similarity(&embedding2);
        assert_eq!(similarity, 0.0, "Zero vector should have 0.0 similarity");
    }
}
