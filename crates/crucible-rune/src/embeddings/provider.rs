//! Trait definition for embedding providers

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::error::{EmbeddingError, EmbeddingResult};

/// Trait for embedding providers
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embeddings for a single text
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse>;

    /// Generate embeddings for multiple texts (batch processing)
    async fn embed_batch(&self, texts: &[String]) -> EmbeddingResult<Vec<EmbeddingResponse>>;

    /// Get provider information
    fn provider_info(&self) -> ProviderInfo;

    /// Check if the provider is healthy
    async fn health_check(&self) -> EmbeddingResult<bool>;

    /// Get available models
    async fn list_models(&self) -> EmbeddingResult<Vec<String>>;

    /// Check if a specific model is available
    async fn model_available(&self, model: &str) -> EmbeddingResult<bool>;

    /// Get token count for text (approximate)
    fn estimate_tokens(&self, text: &str) -> usize;

    /// Get maximum tokens allowed per request
    fn max_tokens(&self) -> usize;

    /// Get expected embedding dimensions for current model
    fn dimensions(&self) -> usize;

    /// Get the current model name
    fn model(&self) -> &str;
}

/// Response from embedding generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResponse {
    /// The embedding vector
    pub embedding: Vec<f32>,

    /// Number of tokens processed
    pub token_count: usize,

    /// Processing time in milliseconds
    pub processing_time_ms: u64,

    /// Model used for generation
    pub model: String,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl EmbeddingResponse {
    /// Create a new embedding response
    pub fn new(
        embedding: Vec<f32>,
        token_count: usize,
        processing_time_ms: u64,
        model: String,
    ) -> Self {
        Self {
            embedding,
            token_count,
            processing_time_ms,
            model,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Get the length of the embedding vector
    pub fn dimensions(&self) -> usize {
        self.embedding.len()
    }

    /// Check if the embedding is valid (non-empty, finite values)
    pub fn is_valid(&self) -> bool {
        if self.embedding.is_empty() {
            return false;
        }

        // Check for NaN or infinite values
        self.embedding.iter().all(|&v| v.is_finite())
    }

    /// Normalize the embedding vector
    pub fn normalize(mut self) -> Self {
        let norm: f32 = self.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm != 0.0 {
            for value in self.embedding.iter_mut() {
                *value /= norm;
            }
        }
        self
    }

    /// Get a normalized copy of the embedding
    pub fn normalized(&self) -> Vec<f32> {
        let mut result = self.embedding.clone();
        let norm: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm != 0.0 {
            for value in result.iter_mut() {
                *value /= norm;
            }
        }
        result
    }
}

/// Information about an embedding provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Provider name
    pub name: String,

    /// Provider version
    pub version: String,

    /// Current model
    pub model: String,

    /// Available models
    pub available_models: Vec<String>,

    /// Maximum batch size
    pub max_batch_size: usize,

    /// Maximum tokens per request
    pub max_tokens: usize,

    /// Default embedding dimensions
    pub dimensions: usize,

    /// Provider capabilities
    pub capabilities: ProviderCapabilities,

    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Capabilities of an embedding provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Whether batch processing is supported
    pub supports_batch: bool,

    /// Whether streaming is supported
    pub supports_streaming: bool,

    /// Whether custom models are supported
    pub supports_custom_models: bool,

    /// Whether token counting is accurate
    pub accurate_token_count: bool,

    /// Whether the provider supports dimensionality control
    pub supports_dimension_control: bool,

    /// Whether the provider supports async processing
    pub supports_async: bool,
}

/// Statistics for embedding operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmbeddingStats {
    /// Total number of requests
    pub total_requests: u64,

    /// Total number of texts processed
    pub total_texts: u64,

    /// Total number of tokens processed
    pub total_tokens: u64,

    /// Average processing time in milliseconds
    pub avg_processing_time_ms: f64,

    /// Minimum processing time in milliseconds
    pub min_processing_time_ms: u64,

    /// Maximum processing time in milliseconds
    pub max_processing_time_ms: u64,

    /// Number of successful requests
    pub successful_requests: u64,

    /// Number of failed requests
    pub failed_requests: u64,

    /// Error breakdown by type
    pub errors_by_type: HashMap<String, u64>,

    /// Last request timestamp
    pub last_request: Option<chrono::DateTime<chrono::Utc>>,
}

impl EmbeddingStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful request
    pub fn record_success(&mut self, text_count: usize, token_count: usize, processing_time_ms: u64) {
        self.total_requests += 1;
        self.total_texts += text_count as u64;
        self.total_tokens += token_count as u64;
        self.successful_requests += 1;

        // Update processing time stats
        self.min_processing_time_ms = self.min_processing_time_ms.min(processing_time_ms);
        self.max_processing_time_ms = self.max_processing_time_ms.max(processing_time_ms);

        if self.successful_requests > 0 {
            self.avg_processing_time_ms =
                (self.avg_processing_time_ms * (self.successful_requests - 1) as f64 + processing_time_ms as f64)
                / self.successful_requests as f64;
        }

        self.last_request = Some(chrono::Utc::now());
    }

    /// Record a failed request
    pub fn record_failure(&mut self, error_type: &str) {
        self.total_requests += 1;
        self.failed_requests += 1;

        *self.errors_by_type.entry(error_type.to_string()).or_insert(0) += 1;

        self.last_request = Some(chrono::Utc::now());
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_requests > 0 {
            self.successful_requests as f64 / self.total_requests as f64
        } else {
            1.0
        }
    }

    /// Get failure rate
    pub fn failure_rate(&self) -> f64 {
        1.0 - self.success_rate()
    }

    /// Get average texts per request
    pub fn avg_texts_per_request(&self) -> f64 {
        if self.total_requests > 0 {
            self.total_texts as f64 / self.total_requests as f64
        } else {
            0.0
        }
    }

    /// Get average tokens per request
    pub fn avg_tokens_per_request(&self) -> f64 {
        if self.total_requests > 0 {
            self.total_tokens as f64 / self.total_requests as f64
        } else {
            0.0
        }
    }

    /// Get requests per minute (based on last request time)
    pub fn requests_per_minute(&self) -> f64 {
        if let Some(last_request) = self.last_request {
            let now = chrono::Utc::now();
            let minutes_elapsed = (now - last_request).num_minutes().max(1) as f64;
            self.total_requests as f64 / minutes_elapsed
        } else {
            0.0
        }
    }

    /// Reset statistics
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Configuration for embedding requests
#[derive(Debug, Clone)]
pub struct EmbeddingRequestConfig {
    /// Whether to normalize embeddings
    pub normalize: bool,

    /// Custom model override
    pub model: Option<String>,

    /// Request timeout in seconds
    pub timeout_secs: Option<u64>,

    /// Custom headers
    pub headers: HashMap<String, String>,

    /// Whether to include token count in response
    pub include_token_count: bool,

    /// Whether to include processing time in response
    pub include_processing_time: bool,
}

impl Default for EmbeddingRequestConfig {
    fn default() -> Self {
        Self {
            normalize: false,
            model: None,
            timeout_secs: None,
            headers: HashMap::new(),
            include_token_count: true,
            include_processing_time: true,
        }
    }
}

/// Utility functions for embedding providers
pub mod utils {
    use super::*;

    /// Split text into chunks that fit within token limits
    pub fn chunk_text(text: &str, max_tokens: usize, chunk_overlap: usize) -> Vec<String> {
        if text.is_empty() {
            return vec![];
        }

        // Simple splitting by words - in a real implementation, you'd use a proper tokenizer
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut chunks = Vec::new();

        if words.len() <= max_tokens {
            chunks.push(text.to_string());
            return chunks;
        }

        let mut i = 0;
        while i < words.len() {
            let end = (i + max_tokens).min(words.len());
            let chunk = words[i..end].join(" ");
            chunks.push(chunk);

            // Move to next chunk with overlap
            i = if i + max_tokens >= words.len() {
                words.len()
            } else {
                i + max_tokens - chunk_overlap
            };
        }

        chunks
    }

    /// Estimate token count using simple heuristic (words / 0.75)
    pub fn estimate_tokens_simple(text: &str) -> usize {
        let word_count = text.split_whitespace().count();
        (word_count as f64 / 0.75).ceil() as usize
    }

    /// Validate embedding vector
    pub fn validate_embedding(embedding: &[f32]) -> EmbeddingResult<()> {
        if embedding.is_empty() {
            return Err(EmbeddingError::InvalidResponse(
                "Empty embedding vector".to_string()
            ));
        }

        if embedding.len() > 10000 {
            return Err(EmbeddingError::InvalidResponse(
                "Embedding vector too large".to_string()
            ));
        }

        // Check for NaN or infinite values
        for (i, &value) in embedding.iter().enumerate() {
            if !value.is_finite() {
                return Err(EmbeddingError::InvalidResponse(
                    format!("Non-finite value at index {}: {}", i, value)
                ));
            }
        }

        Ok(())
    }

    /// Normalize embedding vector in place
    pub fn normalize_embedding(embedding: &mut [f32]) {
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm != 0.0 {
            for value in embedding.iter_mut() {
                *value /= norm;
            }
        }
    }

    /// Calculate cosine similarity between two embeddings
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f64 = a.iter()
            .zip(b.iter())
            .map(|(x, y)| (*x as f64) * (*y as f64))
            .sum();

        let norm_a: f64 = a.iter()
            .map(|x| (*x as f64).powi(2))
            .sum::<f64>()
            .sqrt();

        let norm_b: f64 = b.iter()
            .map(|x| (*x as f64).powi(2))
            .sum::<f64>()
            .sqrt();

        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot_product / (norm_a * norm_b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_response() {
        let embedding = vec![0.1, 0.2, 0.3];
        let response = EmbeddingResponse::new(
            embedding.clone(),
            10,
            100,
            "test-model".to_string(),
        );

        assert_eq!(response.dimensions(), 3);
        assert!(response.is_valid());
        assert_eq!(response.token_count, 10);
        assert_eq!(response.model, "test-model");
    }

    #[test]
    fn test_embedding_response_normalize() {
        let embedding = vec![3.0, 4.0]; // Should normalize to [0.6, 0.8]
        let response = EmbeddingResponse::new(
            embedding,
            10,
            100,
            "test-model".to_string(),
        );

        let normalized = response.normalize();
        assert!((normalized.embedding[0] - 0.6).abs() < 0.001);
        assert!((normalized.embedding[1] - 0.8).abs() < 0.001);

        // Check that the normalized vector has unit length
        let norm: f32 = normalized.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_embedding_stats() {
        let mut stats = EmbeddingStats::new();

        stats.record_success(1, 10, 100);
        stats.record_success(2, 20, 200);

        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.successful_requests, 2);
        assert_eq!(stats.total_texts, 3);
        assert_eq!(stats.total_tokens, 30);
        assert_eq!(stats.success_rate(), 1.0);
        assert_eq!(stats.avg_texts_per_request(), 1.5);
        assert_eq!(stats.avg_tokens_per_request(), 15.0);
        assert_eq!(stats.min_processing_time_ms, 100);
        assert_eq!(stats.max_processing_time_ms, 200);

        stats.record_failure("timeout");
        assert_eq!(stats.total_requests, 3);
        assert_eq!(stats.failed_requests, 1);
        assert_eq!(stats.success_rate(), 2.0 / 3.0);
        assert_eq!(stats.failure_rate(), 1.0 / 3.0);
    }

    #[test]
    fn test_chunk_text() {
        let text = "word1 word2 word3 word4 word5 word6 word7 word8";
        let chunks = chunk_text(text, 3, 1);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "word1 word2 word3");
        assert_eq!(chunks[1], "word3 word4 word5");
        assert_eq!(chunks[2], "word5 word6 word7 word8");
    }

    #[test]
    fn test_estimate_tokens_simple() {
        let text = "This is a test text with ten words total";
        let tokens = estimate_tokens_simple(text);
        assert_eq!(tokens, 13); // 10 words / 0.75 = 13.33, rounded up
    }

    #[test]
    fn test_validate_embedding() {
        let valid_embedding = vec![0.1, 0.2, 0.3];
        assert!(utils::validate_embedding(&valid_embedding).is_ok());

        let empty_embedding = vec![];
        assert!(utils::validate_embedding(&empty_embedding).is_err());

        let invalid_embedding = vec![f32::NAN, 0.2, 0.3];
        assert!(utils::validate_embedding(&invalid_embedding).is_err());

        let infinite_embedding = vec![f32::INFINITY, 0.2, 0.3];
        assert!(utils::validate_embedding(&infinite_embedding).is_err());
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let c = vec![1.0, 0.0, 0.0];

        assert_eq!(utils::cosine_similarity(&a, &b), 0.0);
        assert_eq!(utils::cosine_similarity(&a, &c), 1.0);

        let d = vec![1.0, 1.0, 0.0];
        let e = vec![1.0, 0.0, 1.0];
        let similarity = utils::cosine_similarity(&d, &e);
        assert!((similarity - 0.5).abs() < 0.001);
    }
}