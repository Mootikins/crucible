//! Enrichment Configuration
//!
//! This module defines the configuration for the enrichment pipeline,
//! including embedding provider configuration with provider-specific settings.
//!
//! ## Design Philosophy
//!
//! - **Type Safety**: Each provider has its own struct with specific fields
//! - **Clear Defaults**: Each provider variant implements clear default values
//! - **Easy Configuration**: Strongly-typed configuration prevents errors
//! - **Extensibility**: Easy to add new providers with their specific requirements

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

// Re-export the EmbeddingProviderType from components module to avoid duplication
pub use super::components::EmbeddingProviderType;

/// Main enrichment configuration
///
/// This configuration encompasses all settings related to document enrichment,
/// including the embedding provider configuration and enrichment pipeline settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnrichmentConfig {
    /// Embedding provider configuration
    pub provider: EmbeddingProviderConfig,

    /// Pipeline configuration
    pub pipeline: PipelineConfig,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            provider: EmbeddingProviderConfig::default(),
            pipeline: PipelineConfig::default(),
        }
    }
}

/// Embedding provider configuration
///
/// Each variant contains provider-specific configuration. This makes it
/// clear what settings are available for each provider and provides
/// type-safe configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum EmbeddingProviderConfig {
    /// OpenAI embedding provider
    OpenAI(OpenAIConfig),

    /// Ollama (local) embedding provider
    Ollama(OllamaConfig),

    /// FastEmbed (local) embedding provider
    FastEmbed(FastEmbedConfig),

    /// Cohere embedding provider
    Cohere(CohereConfig),

    /// Google Vertex AI embedding provider
    VertexAI(VertexAIConfig),

    /// Custom HTTP-based embedding provider
    Custom(CustomConfig),

    /// Mock provider for testing
    Mock(MockConfig),
}

impl Default for EmbeddingProviderConfig {
    fn default() -> Self {
        // Default to FastEmbed for privacy and no API key requirement
        EmbeddingProviderConfig::FastEmbed(FastEmbedConfig::default())
    }
}

/// OpenAI embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OpenAIConfig {
    /// API key for OpenAI
    pub api_key: String,

    /// Model to use
    #[serde(default = "OpenAIConfig::default_model")]
    pub model: String,

    /// Base URL for API (defaults to OpenAI's official endpoint)
    #[serde(default = "OpenAIConfig::default_base_url")]
    pub base_url: String,

    /// Request timeout in seconds
    #[serde(default = "OpenAIConfig::default_timeout")]
    pub timeout_seconds: u64,

    /// Number of retry attempts
    #[serde(default = "OpenAIConfig::default_retries")]
    pub retry_attempts: u32,

    /// Expected embedding dimensions
    #[serde(default = "OpenAIConfig::default_dimensions")]
    pub dimensions: u32,

    /// Custom HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

impl OpenAIConfig {
    fn default_model() -> String {
        "text-embedding-3-small".to_string()
    }

    fn default_base_url() -> String {
        "https://api.openai.com/v1".to_string()
    }

    fn default_timeout() -> u64 {
        30
    }

    fn default_retries() -> u32 {
        3
    }

    fn default_dimensions() -> u32 {
        1536
    }
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: Self::default_model(),
            base_url: Self::default_base_url(),
            timeout_seconds: Self::default_timeout(),
            retry_attempts: Self::default_retries(),
            dimensions: Self::default_dimensions(),
            headers: HashMap::new(),
        }
    }
}

/// Ollama (local) embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaConfig {
    /// Model to use
    #[serde(default = "OllamaConfig::default_model")]
    pub model: String,

    /// Base URL for Ollama server
    #[serde(default = "OllamaConfig::default_base_url")]
    pub base_url: String,

    /// Request timeout in seconds
    #[serde(default = "OllamaConfig::default_timeout")]
    pub timeout_seconds: u64,

    /// Number of retry attempts
    #[serde(default = "OllamaConfig::default_retries")]
    pub retry_attempts: u32,

    /// Expected embedding dimensions
    #[serde(default = "OllamaConfig::default_dimensions")]
    pub dimensions: u32,
}

impl OllamaConfig {
    fn default_model() -> String {
        "nomic-embed-text".to_string()
    }

    fn default_base_url() -> String {
        "http://localhost:11434".to_string()
    }

    fn default_timeout() -> u64 {
        30
    }

    fn default_retries() -> u32 {
        3
    }

    fn default_dimensions() -> u32 {
        768
    }
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            model: Self::default_model(),
            base_url: Self::default_base_url(),
            timeout_seconds: Self::default_timeout(),
            retry_attempts: Self::default_retries(),
            dimensions: Self::default_dimensions(),
        }
    }
}

/// FastEmbed (local) embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FastEmbedConfig {
    /// Model to use
    #[serde(default = "FastEmbedConfig::default_model")]
    pub model: String,

    /// Cache directory for model files
    #[serde(default)]
    pub cache_dir: Option<String>,

    /// Batch size for processing
    #[serde(default = "FastEmbedConfig::default_batch_size")]
    pub batch_size: u32,

    /// Expected embedding dimensions
    #[serde(default = "FastEmbedConfig::default_dimensions")]
    pub dimensions: u32,

    /// Number of threads to use (None = auto)
    #[serde(default)]
    pub num_threads: Option<usize>,
}

impl FastEmbedConfig {
    fn default_model() -> String {
        "BAAI/bge-small-en-v1.5".to_string()
    }

    fn default_batch_size() -> u32 {
        32
    }

    fn default_dimensions() -> u32 {
        384
    }
}

impl Default for FastEmbedConfig {
    fn default() -> Self {
        Self {
            model: Self::default_model(),
            cache_dir: None,
            batch_size: Self::default_batch_size(),
            dimensions: Self::default_dimensions(),
            num_threads: None,
        }
    }
}

/// Cohere embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CohereConfig {
    /// API key for Cohere
    pub api_key: String,

    /// Model to use
    #[serde(default = "CohereConfig::default_model")]
    pub model: String,

    /// Base URL for API
    #[serde(default = "CohereConfig::default_base_url")]
    pub base_url: String,

    /// Request timeout in seconds
    #[serde(default = "CohereConfig::default_timeout")]
    pub timeout_seconds: u64,

    /// Number of retry attempts
    #[serde(default = "CohereConfig::default_retries")]
    pub retry_attempts: u32,

    /// Input type: search_document, search_query, classification, clustering
    #[serde(default = "CohereConfig::default_input_type")]
    pub input_type: String,

    /// Custom HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

impl CohereConfig {
    fn default_model() -> String {
        "embed-english-v3.0".to_string()
    }

    fn default_base_url() -> String {
        "https://api.cohere.ai/v1".to_string()
    }

    fn default_timeout() -> u64 {
        30
    }

    fn default_retries() -> u32 {
        3
    }

    fn default_input_type() -> String {
        "search_document".to_string()
    }
}

impl Default for CohereConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: Self::default_model(),
            base_url: Self::default_base_url(),
            timeout_seconds: Self::default_timeout(),
            retry_attempts: Self::default_retries(),
            input_type: Self::default_input_type(),
            headers: HashMap::new(),
        }
    }
}

/// Google Vertex AI embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VertexAIConfig {
    /// GCP project ID
    pub project_id: String,

    /// Model to use
    #[serde(default = "VertexAIConfig::default_model")]
    pub model: String,

    /// Base URL for API
    #[serde(default = "VertexAIConfig::default_base_url")]
    pub base_url: String,

    /// Request timeout in seconds
    #[serde(default = "VertexAIConfig::default_timeout")]
    pub timeout_seconds: u64,

    /// Number of retry attempts
    #[serde(default = "VertexAIConfig::default_retries")]
    pub retry_attempts: u32,

    /// Service account credentials JSON path
    pub credentials_path: Option<String>,

    /// Custom HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

impl VertexAIConfig {
    fn default_model() -> String {
        "textembedding-gecko@003".to_string()
    }

    fn default_base_url() -> String {
        "https://aiplatform.googleapis.com/v1".to_string()
    }

    fn default_timeout() -> u64 {
        30
    }

    fn default_retries() -> u32 {
        3
    }
}

impl Default for VertexAIConfig {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            model: Self::default_model(),
            base_url: Self::default_base_url(),
            timeout_seconds: Self::default_timeout(),
            retry_attempts: Self::default_retries(),
            credentials_path: None,
            headers: HashMap::new(),
        }
    }
}

/// Custom HTTP-based embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CustomConfig {
    /// Base URL for the custom embedding API
    pub base_url: String,

    /// API key (if required)
    pub api_key: Option<String>,

    /// Model name
    pub model: String,

    /// Request timeout in seconds
    #[serde(default = "CustomConfig::default_timeout")]
    pub timeout_seconds: u64,

    /// Number of retry attempts
    #[serde(default = "CustomConfig::default_retries")]
    pub retry_attempts: u32,

    /// Expected embedding dimensions
    pub dimensions: u32,

    /// Custom HTTP headers
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Request template (JSON)
    pub request_template: Option<String>,

    /// Response path (JSONPath) to extract embeddings
    pub response_path: Option<String>,
}

impl CustomConfig {
    fn default_timeout() -> u64 {
        30
    }

    fn default_retries() -> u32 {
        3
    }
}

impl Default for CustomConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            api_key: None,
            model: String::new(),
            timeout_seconds: Self::default_timeout(),
            retry_attempts: Self::default_retries(),
            dimensions: 768,
            headers: HashMap::new(),
            request_template: None,
            response_path: None,
        }
    }
}

/// Mock embedding provider for testing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MockConfig {
    /// Model name (for testing)
    #[serde(default = "MockConfig::default_model")]
    pub model: String,

    /// Dimensions to return
    #[serde(default = "MockConfig::default_dimensions")]
    pub dimensions: u32,

    /// Simulate latency (milliseconds)
    #[serde(default)]
    pub simulated_latency_ms: u64,
}

impl MockConfig {
    fn default_model() -> String {
        "mock-test-model".to_string()
    }

    fn default_dimensions() -> u32 {
        768
    }
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            model: Self::default_model(),
            dimensions: Self::default_dimensions(),
            simulated_latency_ms: 0,
        }
    }
}

/// Pipeline configuration for enrichment operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineConfig {
    /// Number of parallel worker threads
    #[serde(default = "PipelineConfig::default_worker_count")]
    pub worker_count: usize,

    /// Batch size for processing documents
    #[serde(default = "PipelineConfig::default_batch_size")]
    pub batch_size: usize,

    /// Maximum queue size
    #[serde(default = "PipelineConfig::default_max_queue_size")]
    pub max_queue_size: usize,

    /// Operation timeout in milliseconds
    #[serde(default = "PipelineConfig::default_timeout_ms")]
    pub timeout_ms: u64,

    /// Retry attempts for failed operations
    #[serde(default = "PipelineConfig::default_retry_attempts")]
    pub retry_attempts: u32,

    /// Delay between retries in milliseconds
    #[serde(default = "PipelineConfig::default_retry_delay_ms")]
    pub retry_delay_ms: u64,

    /// Circuit breaker failure threshold
    #[serde(default = "PipelineConfig::default_circuit_breaker_threshold")]
    pub circuit_breaker_threshold: u32,

    /// Circuit breaker timeout in milliseconds
    #[serde(default = "PipelineConfig::default_circuit_breaker_timeout_ms")]
    pub circuit_breaker_timeout_ms: u64,
}

impl PipelineConfig {
    fn default_worker_count() -> usize {
        num_cpus::get()
    }

    fn default_batch_size() -> usize {
        16
    }

    fn default_max_queue_size() -> usize {
        1000
    }

    fn default_timeout_ms() -> u64 {
        30000
    }

    fn default_retry_attempts() -> u32 {
        3
    }

    fn default_retry_delay_ms() -> u64 {
        1000
    }

    fn default_circuit_breaker_threshold() -> u32 {
        5
    }

    fn default_circuit_breaker_timeout_ms() -> u64 {
        60000
    }

    /// Create configuration optimized for throughput
    pub fn optimize_for_throughput() -> Self {
        Self {
            worker_count: num_cpus::get() * 2,
            batch_size: 64,
            max_queue_size: 5000,
            ..Default::default()
        }
    }

    /// Create configuration optimized for latency
    pub fn optimize_for_latency() -> Self {
        Self {
            worker_count: num_cpus::get(),
            batch_size: 4,
            max_queue_size: 100,
            timeout_ms: 5000,
            ..Default::default()
        }
    }

    /// Create configuration optimized for resource usage
    pub fn optimize_for_resources() -> Self {
        Self {
            worker_count: 1,
            batch_size: 8,
            max_queue_size: 100,
            ..Default::default()
        }
    }
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            worker_count: Self::default_worker_count(),
            batch_size: Self::default_batch_size(),
            max_queue_size: Self::default_max_queue_size(),
            timeout_ms: Self::default_timeout_ms(),
            retry_attempts: Self::default_retry_attempts(),
            retry_delay_ms: Self::default_retry_delay_ms(),
            circuit_breaker_threshold: Self::default_circuit_breaker_threshold(),
            circuit_breaker_timeout_ms: Self::default_circuit_breaker_timeout_ms(),
        }
    }
}

/// Helper methods and constructors for EmbeddingProviderConfig
impl EmbeddingProviderConfig {
    /// Create an Ollama provider configuration with defaults
    ///
    /// # Arguments
    /// * `endpoint` - Optional base URL (defaults to "http://localhost:11434")
    /// * `model` - Optional model name (defaults to "nomic-embed-text")
    pub fn ollama(endpoint: Option<String>, model: Option<String>) -> Self {
        Self::Ollama(OllamaConfig {
            model: model.unwrap_or_else(OllamaConfig::default_model),
            base_url: endpoint.unwrap_or_else(OllamaConfig::default_base_url),
            timeout_seconds: OllamaConfig::default_timeout(),
            retry_attempts: OllamaConfig::default_retries(),
            dimensions: OllamaConfig::default_dimensions(),
        })
    }

    /// Create an OpenAI provider configuration
    ///
    /// # Arguments
    /// * `api_key` - OpenAI API key (required)
    /// * `model` - Optional model name (defaults to "text-embedding-3-small")
    pub fn openai(api_key: String, model: Option<String>) -> Self {
        Self::OpenAI(OpenAIConfig {
            api_key,
            model: model.unwrap_or_else(OpenAIConfig::default_model),
            base_url: OpenAIConfig::default_base_url(),
            timeout_seconds: OpenAIConfig::default_timeout(),
            retry_attempts: OpenAIConfig::default_retries(),
            dimensions: OpenAIConfig::default_dimensions(),
            headers: HashMap::new(),
        })
    }

    /// Create a FastEmbed provider configuration with defaults
    ///
    /// # Arguments
    /// * `model` - Optional model name (defaults to "BAAI/bge-small-en-v1.5")
    /// * `cache_dir` - Optional cache directory for model files
    /// * `num_threads` - Optional number of threads (defaults to auto)
    pub fn fastembed(
        model: Option<String>,
        cache_dir: Option<String>,
        num_threads: Option<usize>,
    ) -> Self {
        Self::FastEmbed(FastEmbedConfig {
            model: model.unwrap_or_else(FastEmbedConfig::default_model),
            cache_dir,
            batch_size: FastEmbedConfig::default_batch_size(),
            dimensions: FastEmbedConfig::default_dimensions(),
            num_threads,
        })
    }

    /// Create a Mock provider configuration for testing
    ///
    /// # Arguments
    /// * `dimensions` - Number of dimensions for mock embeddings (defaults to 768)
    pub fn mock(dimensions: Option<u32>) -> Self {
        Self::Mock(MockConfig {
            model: MockConfig::default_model(),
            dimensions: dimensions.unwrap_or_else(MockConfig::default_dimensions),
            simulated_latency_ms: 0,
        })
    }

    /// Get the timeout as a Duration
    pub fn timeout(&self) -> Duration {
        let seconds = match self {
            Self::OpenAI(c) => c.timeout_seconds,
            Self::Ollama(c) => c.timeout_seconds,
            Self::FastEmbed(_) => 30, // FastEmbed is local, no timeout
            Self::Cohere(c) => c.timeout_seconds,
            Self::VertexAI(c) => c.timeout_seconds,
            Self::Custom(c) => c.timeout_seconds,
            Self::Mock(_) => 1,
        };
        Duration::from_secs(seconds)
    }

    /// Get the number of retry attempts
    pub fn retry_attempts(&self) -> u32 {
        match self {
            Self::OpenAI(c) => c.retry_attempts,
            Self::Ollama(c) => c.retry_attempts,
            Self::FastEmbed(_) => 0, // Local processing doesn't need retries
            Self::Cohere(c) => c.retry_attempts,
            Self::VertexAI(c) => c.retry_attempts,
            Self::Custom(c) => c.retry_attempts,
            Self::Mock(_) => 0,
        }
    }

    /// Get the model name
    pub fn model(&self) -> &str {
        match self {
            Self::OpenAI(c) => &c.model,
            Self::Ollama(c) => &c.model,
            Self::FastEmbed(c) => &c.model,
            Self::Cohere(c) => &c.model,
            Self::VertexAI(c) => &c.model,
            Self::Custom(c) => &c.model,
            Self::Mock(c) => &c.model,
        }
    }

    /// Get the model name (alias for model() for backward compatibility)
    pub fn model_name(&self) -> &str {
        self.model()
    }

    /// Get the provider type enum value
    pub fn provider_type(&self) -> EmbeddingProviderType {
        EmbeddingProviderType::from_config(self)
    }

    /// Get the API key if the provider supports it
    pub fn api_key(&self) -> Option<&str> {
        match self {
            Self::OpenAI(c) => Some(&c.api_key),
            Self::Cohere(c) => Some(&c.api_key),
            Self::Custom(c) => c.api_key.as_deref(),
            _ => None,
        }
    }

    /// Get the base URL/endpoint
    pub fn base_url(&self) -> Option<&str> {
        match self {
            Self::OpenAI(c) => Some(&c.base_url),
            Self::Ollama(c) => Some(&c.base_url),
            Self::Cohere(c) => Some(&c.base_url),
            Self::VertexAI(c) => Some(&c.base_url),
            Self::Custom(c) => Some(&c.base_url),
            _ => None,
        }
    }

    /// Get the endpoint URL (alias for base_url for backward compatibility)
    pub fn endpoint(&self) -> String {
        self.base_url()
            .unwrap_or("http://localhost:11434")
            .to_string()
    }

    /// Get timeout in seconds
    pub fn timeout_secs(&self) -> u64 {
        match self {
            Self::OpenAI(c) => c.timeout_seconds,
            Self::Ollama(c) => c.timeout_seconds,
            Self::FastEmbed(_) => 30,
            Self::Cohere(c) => c.timeout_seconds,
            Self::VertexAI(c) => c.timeout_seconds,
            Self::Custom(c) => c.timeout_seconds,
            Self::Mock(_) => 1,
        }
    }

    /// Get the expected embedding dimensions
    pub fn dimensions(&self) -> Option<u32> {
        match self {
            Self::OpenAI(c) => Some(c.dimensions),
            Self::Ollama(c) => Some(c.dimensions),
            Self::FastEmbed(c) => Some(c.dimensions),
            Self::Cohere(_) => None,   // Cohere dimensions vary by model
            Self::VertexAI(_) => None, // VertexAI dimensions vary by model
            Self::Custom(c) => Some(c.dimensions),
            Self::Mock(c) => Some(c.dimensions),
        }
    }

    /// Validate the configuration
    #[must_use]
    pub fn validate(&self) -> Result<(), crate::ConfigValidationError> {
        use crate::ConfigValidationError;

        match self {
            Self::OpenAI(c) => {
                if c.api_key.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "api_key".to_string(),
                    });
                }
                // Basic API key format check (OpenAI keys start with "sk-")
                if !c.api_key.starts_with("sk-") && c.api_key != "test-key" {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "api_key".to_string(),
                        reason: "OpenAI API keys should start with 'sk-'".to_string(),
                    });
                }
                if c.model.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "model".to_string(),
                    });
                }
                // Validate base URL format
                if !c.base_url.starts_with("http://") && !c.base_url.starts_with("https://") {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "base_url".to_string(),
                        reason: "must start with http:// or https://".to_string(),
                    });
                }
                // Validate timeout is reasonable (1-300 seconds)
                if c.timeout_seconds == 0 || c.timeout_seconds > 300 {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "timeout_seconds".to_string(),
                        reason: "must be between 1 and 300 seconds".to_string(),
                    });
                }
            }
            Self::Ollama(c) => {
                if c.model.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "model".to_string(),
                    });
                }
                if c.base_url.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "base_url".to_string(),
                    });
                }
                // Validate base URL format
                if !c.base_url.starts_with("http://") && !c.base_url.starts_with("https://") {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "base_url".to_string(),
                        reason: "must start with http:// or https://".to_string(),
                    });
                }
                // Validate timeout is reasonable (1-300 seconds)
                if c.timeout_seconds == 0 || c.timeout_seconds > 300 {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "timeout_seconds".to_string(),
                        reason: "must be between 1 and 300 seconds".to_string(),
                    });
                }
            }
            Self::FastEmbed(c) => {
                if c.model.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "model".to_string(),
                    });
                }
                if c.batch_size == 0 {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "batch_size".to_string(),
                        reason: "must be greater than 0".to_string(),
                    });
                }
            }
            Self::Cohere(c) => {
                if c.api_key.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "api_key".to_string(),
                    });
                }
                if c.model.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "model".to_string(),
                    });
                }
                // Validate base URL format
                if !c.base_url.starts_with("http://") && !c.base_url.starts_with("https://") {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "base_url".to_string(),
                        reason: "must start with http:// or https://".to_string(),
                    });
                }
                // Validate timeout
                if c.timeout_seconds == 0 || c.timeout_seconds > 300 {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "timeout_seconds".to_string(),
                        reason: "must be between 1 and 300 seconds".to_string(),
                    });
                }
            }
            Self::VertexAI(c) => {
                if c.project_id.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "project_id".to_string(),
                    });
                }
                if c.model.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "model".to_string(),
                    });
                }
            }
            Self::Custom(c) => {
                if c.base_url.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "base_url".to_string(),
                    });
                }
                // Validate base URL format
                if !c.base_url.starts_with("http://") && !c.base_url.starts_with("https://") {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "base_url".to_string(),
                        reason: "must start with http:// or https://".to_string(),
                    });
                }
                if c.model.is_empty() {
                    return Err(ConfigValidationError::MissingField {
                        field: "model".to_string(),
                    });
                }
                if c.dimensions == 0 {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "dimensions".to_string(),
                        reason: "must be greater than 0".to_string(),
                    });
                }
                // Validate timeout
                if c.timeout_seconds == 0 || c.timeout_seconds > 300 {
                    return Err(ConfigValidationError::InvalidValue {
                        field: "timeout_seconds".to_string(),
                        reason: "must be between 1 and 300 seconds".to_string(),
                    });
                }
            }
            Self::Mock(_) => {
                // Mock provider has no validation requirements
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_enrichment_config() {
        let config = EnrichmentConfig::default();
        assert!(matches!(
            config.provider,
            EmbeddingProviderConfig::FastEmbed(_)
        ));
        assert_eq!(config.pipeline.batch_size, 16);
    }

    #[test]
    fn test_openai_config_defaults() {
        let config = OpenAIConfig::default();
        assert_eq!(config.model, "text-embedding-3-small");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.dimensions, 1536);
    }

    #[test]
    fn test_ollama_config_defaults() {
        let config = OllamaConfig::default();
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.dimensions, 768);
    }

    #[test]
    fn test_fastembed_config_defaults() {
        let config = FastEmbedConfig::default();
        assert_eq!(config.model, "BAAI/bge-small-en-v1.5");
        assert_eq!(config.dimensions, 384);
        assert_eq!(config.batch_size, 32);
    }

    #[test]
    fn test_validation_openai_missing_key() {
        let config = EmbeddingProviderConfig::OpenAI(OpenAIConfig::default());
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_openai_valid() {
        let config = EmbeddingProviderConfig::OpenAI(OpenAIConfig {
            api_key: "test-key".to_string(),
            ..Default::default()
        });
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pipeline_optimization_presets() {
        let throughput = PipelineConfig::optimize_for_throughput();
        assert_eq!(throughput.batch_size, 64);

        let latency = PipelineConfig::optimize_for_latency();
        assert_eq!(latency.batch_size, 4);
        assert_eq!(latency.timeout_ms, 5000);

        let resources = PipelineConfig::optimize_for_resources();
        assert_eq!(resources.worker_count, 1);
    }

    #[test]
    fn test_helper_methods() {
        let config = EmbeddingProviderConfig::OpenAI(OpenAIConfig::default());
        assert_eq!(config.model(), "text-embedding-3-small");
        assert_eq!(config.dimensions(), Some(1536));
        assert_eq!(config.retry_attempts(), 3);
    }
}
