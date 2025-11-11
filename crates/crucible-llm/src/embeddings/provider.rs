// crates/crucible-mcp/src/embeddings/provider.rs

//! Provider trait and types for text embedding generation
//!
//! This module defines the core trait that all embedding providers must implement,
//! along with the response types returned by embedding operations.
//!
//! # Example
//!
//! ```rust,no_run
//! // DEPRECATED: MCP functionality has been removed
//! // This example is for historical reference only
//! use crucible_llm::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
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
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::error::EmbeddingResult;

/// Model family or architecture type
///
/// Categorizes models by their underlying architecture. This is useful for
/// understanding compatibility, expected performance characteristics, and
/// appropriate use cases.
///
/// # Design Note
///
/// This enum is non-exhaustive to allow adding new architectures without
/// breaking changes. Unknown architectures are captured in the `Other` variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ModelFamily {
    /// BERT-based models (Bidirectional Encoder Representations from Transformers)
    ///
    /// Examples: nomic-embed-text, sentence-transformers
    Bert,

    /// GPT-based models (Generative Pre-trained Transformer)
    ///
    /// Examples: text-embedding-ada-002, text-embedding-3-small
    Gpt,

    /// LLaMA-based models
    ///
    /// Examples: llama2-embedding
    Llama,

    /// T5-based models (Text-to-Text Transfer Transformer)
    T5,

    /// CLIP-based models (Contrastive Language-Image Pre-training)
    ///
    /// Multimodal models for text and image embeddings
    Clip,

    /// MPNet-based models
    Mpnet,

    /// RoBERTa-based models
    Roberta,

    /// Other or unknown architecture
    ///
    /// Contains the architecture name as reported by the provider
    Other(String),
}

impl ModelFamily {
    /// Parse a model family from a string
    ///
    /// Case-insensitive matching against known families.
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bert" => Self::Bert,
            "gpt" => Self::Gpt,
            "llama" => Self::Llama,
            "t5" => Self::T5,
            "clip" => Self::Clip,
            "mpnet" => Self::Mpnet,
            "roberta" => Self::Roberta,
            other => Self::Other(other.to_string()),
        }
    }

    /// Get the canonical string representation of this family
    pub fn as_str(&self) -> &str {
        match self {
            Self::Bert => "bert",
            Self::Gpt => "gpt",
            Self::Llama => "llama",
            Self::T5 => "t5",
            Self::Clip => "clip",
            Self::Mpnet => "mpnet",
            Self::Roberta => "roberta",
            Self::Other(s) => s.as_str(),
        }
    }
}

/// Parameter size representation
///
/// Represents the number of trainable parameters in a model using standard
/// suffixes (M for million, B for billion). This is stored as a structured
/// type rather than a raw string to enable sorting and comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParameterSize {
    /// The numeric value (e.g., 137 for "137M")
    value: u32,

    /// The unit (Million or Billion)
    unit: ParameterUnit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
enum ParameterUnit {
    Million, // M
    Billion, // B
}

impl ParameterSize {
    /// Create a new parameter size
    pub fn new(value: u32, millions: bool) -> Self {
        Self {
            value,
            unit: if millions {
                ParameterUnit::Million
            } else {
                ParameterUnit::Billion
            },
        }
    }

    /// Parse a parameter size from a string like "137M" or "7B"
    pub fn from_str(s: &str) -> Option<Self> {
        let s = s.trim().to_uppercase();

        if let Some(value_str) = s.strip_suffix('M') {
            let value = value_str.parse::<f32>().ok()?;
            Some(Self::new((value).round() as u32, true))
        } else if let Some(value_str) = s.strip_suffix('B') {
            let value = value_str.parse::<f32>().ok()?;
            Some(Self::new((value).round() as u32, false))
        } else {
            None
        }
    }

    /// Get the approximate parameter count as an integer
    pub fn approximate_count(&self) -> u64 {
        match self.unit {
            ParameterUnit::Million => self.value as u64 * 1_000_000,
            ParameterUnit::Billion => self.value as u64 * 1_000_000_000,
        }
    }
}

impl std::fmt::Display for ParameterSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.unit {
            ParameterUnit::Million => write!(f, "{}M", self.value),
            ParameterUnit::Billion => write!(f, "{}B", self.value),
        }
    }
}

impl PartialOrd for ParameterSize {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ParameterSize {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.approximate_count().cmp(&other.approximate_count())
    }
}

/// Information about an available embedding model
///
/// This struct represents metadata about embedding models available from a provider.
/// It supports optional fields to accommodate different provider APIs (Ollama, OpenAI, etc.)
/// and is serializable for caching and configuration purposes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    /// Model identifier (e.g., "nomic-embed-text:latest", "text-embedding-3-small")
    pub name: String,

    /// Human-readable display name (optional)
    pub display_name: Option<String>,

    /// Model family or architecture (e.g., "bert", "gpt", "llama")
    pub family: Option<ModelFamily>,

    /// Embedding dimensionality
    pub dimensions: Option<usize>,

    /// Model size in bytes (on disk or in memory)
    pub size_bytes: Option<u64>,

    /// Parameter count (e.g., "137M", "7B")
    pub parameter_size: Option<ParameterSize>,

    /// Quantization level (e.g., "Q4_0", "Q8_0", "fp16")
    pub quantization: Option<String>,

    /// Model format (e.g., "gguf", "safetensors", "pytorch")
    pub format: Option<String>,

    /// When the model was last modified or pulled
    pub modified_at: Option<DateTime<Utc>>,

    /// Content digest or hash (e.g., "sha256:c1f958f8c3e8...")
    pub digest: Option<String>,

    /// Maximum context length (tokens)
    pub max_tokens: Option<usize>,

    /// Whether this model is recommended or featured by the provider
    pub recommended: bool,

    /// Additional provider-specific metadata
    pub metadata: Option<serde_json::Value>,
}

impl ModelInfo {
    /// Create a new ModelInfo with just a name (minimal constructor)
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: None,
            family: None,
            dimensions: None,
            size_bytes: None,
            parameter_size: None,
            quantization: None,
            format: None,
            modified_at: None,
            digest: None,
            max_tokens: None,
            recommended: false,
            metadata: None,
        }
    }

    /// Create a builder for constructing ModelInfo instances
    pub fn builder() -> ModelInfoBuilder {
        ModelInfoBuilder::default()
    }

    /// Get the display name, falling back to name if not set
    pub fn display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }

    /// Check if this model is compatible with a required dimension count
    pub fn is_compatible_dimensions(&self, required_dims: usize) -> bool {
        self.dimensions.map_or(true, |dims| dims == required_dims)
    }

    /// Format the model size as a human-readable string
    pub fn formatted_size(&self) -> Option<String> {
        self.size_bytes.map(|bytes| {
            const KB: u64 = 1024;
            const MB: u64 = KB * 1024;
            const GB: u64 = MB * 1024;

            if bytes >= GB {
                format!("{:.1} GB", bytes as f64 / GB as f64)
            } else if bytes >= MB {
                format!("{} MB", bytes / MB)
            } else if bytes >= KB {
                format!("{} KB", bytes / KB)
            } else {
                format!("{} B", bytes)
            }
        })
    }
}

/// Builder for ModelInfo instances
#[derive(Default)]
pub struct ModelInfoBuilder {
    name: Option<String>,
    display_name: Option<String>,
    family: Option<ModelFamily>,
    dimensions: Option<usize>,
    size_bytes: Option<u64>,
    parameter_size: Option<ParameterSize>,
    quantization: Option<String>,
    format: Option<String>,
    modified_at: Option<DateTime<Utc>>,
    digest: Option<String>,
    max_tokens: Option<usize>,
    recommended: bool,
    metadata: Option<serde_json::Value>,
}

impl ModelInfoBuilder {
    /// Set the model name (required)
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the display name
    pub fn display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    /// Set the model family
    pub fn family(mut self, family: ModelFamily) -> Self {
        self.family = Some(family);
        self
    }

    /// Set the embedding dimensions
    pub fn dimensions(mut self, dimensions: usize) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    /// Set the model size in bytes
    pub fn size_bytes(mut self, size_bytes: u64) -> Self {
        self.size_bytes = Some(size_bytes);
        self
    }

    /// Set the parameter size
    pub fn parameter_size(mut self, parameter_size: ParameterSize) -> Self {
        self.parameter_size = Some(parameter_size);
        self
    }

    /// Set the quantization level
    pub fn quantization(mut self, quantization: impl Into<String>) -> Self {
        self.quantization = Some(quantization.into());
        self
    }

    /// Set the model format
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Set the modification timestamp
    pub fn modified_at(mut self, modified_at: DateTime<Utc>) -> Self {
        self.modified_at = Some(modified_at);
        self
    }

    /// Set the digest/hash
    pub fn digest(mut self, digest: impl Into<String>) -> Self {
        self.digest = Some(digest.into());
        self
    }

    /// Set the maximum token count
    pub fn max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Mark as recommended
    pub fn recommended(mut self, recommended: bool) -> Self {
        self.recommended = recommended;
        self
    }

    /// Set additional metadata
    pub fn metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Build the ModelInfo instance
    ///
    /// # Panics
    ///
    /// Panics if `name` was not set
    pub fn build(self) -> ModelInfo {
        ModelInfo {
            name: self.name.expect("name is required"),
            display_name: self.display_name,
            family: self.family,
            dimensions: self.dimensions,
            size_bytes: self.size_bytes,
            parameter_size: self.parameter_size,
            quantization: self.quantization,
            format: self.format,
            modified_at: self.modified_at,
            digest: self.digest,
            max_tokens: self.max_tokens,
            recommended: self.recommended,
            metadata: self.metadata,
        }
    }
}

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
    /// use crucible_llm::embeddings::EmbeddingResponse;
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
    /// use crucible_llm::embeddings::EmbeddingResponse;
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
/// use crucible_llm::embeddings::{EmbeddingProvider, EmbeddingResponse, EmbeddingResult};
/// use crucible_llm::embeddings::provider::ModelInfo;
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
///
///     async fn list_models(&self) -> EmbeddingResult<Vec<ModelInfo>> {
///         // Return available models for this provider
///         Ok(vec![])
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
    /// # use crucible_llm::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
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
    /// # use crucible_llm::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EmbeddingConfig::default();
    /// let provider = create_provider(config).await?;
    ///
    /// let texts = vec![
    ///     "First note".to_string(),
    ///     "Second note".to_string(),
    ///     "Third note".to_string(),
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
    /// # use crucible_llm::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
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
    /// # use crucible_llm::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
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
    /// # use crucible_llm::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
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
    /// # use crucible_llm::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
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

    /// List available models from this provider
    ///
    /// Queries the provider's API to discover what embedding models are available.
    /// This is useful for:
    /// - Model selection UI
    /// - Validation of configured model names
    /// - Discovering model capabilities (dimensions, size, etc.)
    /// - Cache warming or pre-configuration
    ///
    /// # Returns
    ///
    /// A vector of `ModelInfo` structs describing available models
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The provider API is unavailable
    /// - Authentication fails
    /// - The response cannot be parsed
    /// - A timeout occurs
    /// - Model discovery is not supported by this provider
    ///
    /// # Provider Implementation Notes
    ///
    /// Providers should return as much metadata as available from their API:
    /// - **Required fields**: `name` (always required)
    /// - **Highly recommended**: `dimensions` (critical for embeddings)
    /// - **Optional but useful**: All other fields
    ///
    /// If a provider doesn't support model discovery, it should return
    /// `EmbeddingError::ModelDiscoveryNotSupported`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use crucible_llm::embeddings::{EmbeddingProvider, create_provider, EmbeddingConfig};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EmbeddingConfig::default();
    /// let provider = create_provider(config).await?;
    ///
    /// // List all available models
    /// let models = provider.list_models().await?;
    ///
    /// for model in models {
    ///     println!("Model: {} ({} dims)",
    ///         model.display_name(),
    ///         model.dimensions.map_or("unknown".to_string(), |d| d.to_string())
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Performance Considerations
    ///
    /// This method may make network requests and should be called infrequently.
    /// Consider caching the results if you need to access model information repeatedly.
    async fn list_models(&self) -> EmbeddingResult<Vec<ModelInfo>>;
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
        let response = EmbeddingResponse::new(embedding, "test-model".to_string()).with_tokens(10);

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
        assert!(
            (similarity - 1.0).abs() < 1e-6,
            "Expected 1.0, got {}",
            similarity
        );
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
        assert!(
            (similarity + 1.0).abs() < 1e-6,
            "Expected -1.0, got {}",
            similarity
        );
    }

    #[test]
    #[should_panic(
        expected = "Cannot calculate cosine similarity for embeddings with different dimensions"
    )]
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
