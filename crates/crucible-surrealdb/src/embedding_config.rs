//! Embedding Thread Pool Configuration
//!
//! Configuration types and validation for the vector embedding thread pool system.
//! Provides privacy-focused local processing with configurable performance settings.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for embedding thread pool operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Number of worker threads for parallel processing
    pub worker_count: usize,

    /// Number of documents to process in each batch
    pub batch_size: usize,

    /// Type of embedding model to use
    pub model_type: EmbeddingModel,

    /// Privacy mode for processing
    pub privacy_mode: PrivacyMode,

    /// Maximum number of tasks in the processing queue
    pub max_queue_size: usize,

    /// Timeout for individual embedding operations (milliseconds)
    pub timeout_ms: u64,

    /// Number of retry attempts for failed operations
    pub retry_attempts: u32,

    /// Delay between retry attempts (milliseconds)
    pub retry_delay_ms: u64,

    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: u32,

    /// Circuit breaker timeout (milliseconds)
    pub circuit_breaker_timeout_ms: u64,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            worker_count: num_cpus::get(),
            batch_size: 16,
            model_type: EmbeddingModel::LocalStandard,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 1000,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        }
    }
}

impl EmbeddingConfig {
    /// Create configuration optimized for high throughput
    pub fn optimize_for_throughput() -> Self {
        Self {
            worker_count: num_cpus::get(),
            batch_size: 64,
            model_type: EmbeddingModel::LocalStandard,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 2000,
            timeout_ms: 60000,
            retry_attempts: 2,
            retry_delay_ms: 500,
            circuit_breaker_threshold: 20,
            circuit_breaker_timeout_ms: 60000,
        }
    }

    /// Create configuration optimized for low latency
    pub fn optimize_for_latency() -> Self {
        Self {
            worker_count: num_cpus::get(),
            batch_size: 4,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 100,
            timeout_ms: 5000,
            retry_attempts: 1,
            retry_delay_ms: 100,
            circuit_breaker_threshold: 5,
            circuit_breaker_timeout_ms: 10000,
        }
    }

    /// Create configuration optimized for resource efficiency
    pub fn optimize_for_resources() -> Self {
        Self {
            worker_count: 1,
            batch_size: 8,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 50,
            timeout_ms: 15000,
            retry_attempts: 1,
            retry_delay_ms: 200,
            circuit_breaker_threshold: 3,
            circuit_breaker_timeout_ms: 15000,
        }
    }

    /// Create configuration based on machine specifications
    pub fn for_machine_specs(cpu_cores: usize, memory_bytes: usize) -> Self {
        let worker_count = cpu_cores.min(8); // Cap at 8 workers
        let batch_size = match memory_bytes {
            0..=2_147_483_648 => 8,              // <= 2GB
            2_147_483_649..=8_589_934_592 => 16, // 2GB - 8GB
            _ => 32,                             // > 8GB
        };

        Self {
            worker_count,
            batch_size,
            model_type: if memory_bytes < 4_294_967_296 {
                EmbeddingModel::LocalMini
            } else {
                EmbeddingModel::LocalStandard
            },
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: worker_count * 100,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        }
    }

    /// Get timeout as Duration
    pub fn timeout_duration(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    /// Get retry delay as Duration
    pub fn retry_delay_duration(&self) -> Duration {
        Duration::from_millis(self.retry_delay_ms)
    }

    /// Get circuit breaker timeout as Duration
    pub fn circuit_breaker_timeout(&self) -> Duration {
        Duration::from_millis(self.circuit_breaker_timeout_ms)
    }

    /// Check if configuration is privacy-focused
    pub fn is_privacy_focused(&self) -> bool {
        matches!(self.privacy_mode, PrivacyMode::StrictLocal)
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<()> {
        if self.worker_count == 0 {
            return Err(anyhow::anyhow!("Worker count must be greater than 0"));
        }

        if self.batch_size == 0 {
            return Err(anyhow::anyhow!("Batch size must be greater than 0"));
        }

        if self.batch_size > 10000 {
            return Err(anyhow::anyhow!("Batch size too large (max 10000)"));
        }

        if self.timeout_ms == 0 {
            return Err(anyhow::anyhow!("Timeout must be greater than 0"));
        }

        if self.max_queue_size == 0 {
            return Err(anyhow::anyhow!("Max queue size must be greater than 0"));
        }

        if self.circuit_breaker_threshold == 0 {
            return Err(anyhow::anyhow!(
                "Circuit breaker threshold must be greater than 0"
            ));
        }

        Ok(())
    }
}

/// Types of embedding models available
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EmbeddingModel {
    /// Minimal model for resource-constrained environments
    LocalMini,
    /// Standard model for general use
    LocalStandard,
    /// Large model for maximum accuracy
    LocalLarge,
}

impl EmbeddingModel {
    /// Get embedding dimensions for this model
    pub fn dimensions(&self) -> usize {
        match self {
            EmbeddingModel::LocalMini => 256,
            EmbeddingModel::LocalStandard => 768,
            EmbeddingModel::LocalLarge => 1536,
        }
    }

    /// Get model name string
    pub fn model_name(&self) -> &'static str {
        match self {
            EmbeddingModel::LocalMini => "local-mini",
            EmbeddingModel::LocalStandard => "local-standard",
            EmbeddingModel::LocalLarge => "local-large",
        }
    }

    /// Check if this is a lightweight model
    pub fn is_lightweight(&self) -> bool {
        matches!(self, EmbeddingModel::LocalMini)
    }
}

/// Privacy modes for embedding processing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PrivacyMode {
    /// All processing done locally, no external calls
    StrictLocal,
    /// Allow external fallback only if local fails
    AllowExternalFallback,
    /// Hybrid mode with local caching
    HybridMode,
}

impl PrivacyMode {
    /// Check if external processing is allowed
    pub fn allows_external(&self) -> bool {
        !matches!(self, PrivacyMode::StrictLocal)
    }

    /// Check if this is strict privacy mode
    pub fn is_strict(&self) -> bool {
        matches!(self, PrivacyMode::StrictLocal)
    }
}

/// Metrics for thread pool performance monitoring
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadPoolMetrics {
    /// Total number of tasks processed
    pub total_tasks_processed: u64,

    /// Number of currently active workers
    pub active_workers: u32,

    /// Current queue size
    pub queue_size: u32,

    /// Average processing time per task
    pub average_processing_time: Duration,

    /// Number of failed tasks
    pub failed_tasks: u64,

    /// Circuit breaker state
    pub circuit_breaker_open: bool,

    /// Memory usage in bytes (if available)
    pub memory_usage: Option<u64>,
}

impl ThreadPoolMetrics {
    /// Create new metrics with default values
    pub fn new() -> Self {
        Self {
            total_tasks_processed: 0,
            active_workers: 0,
            queue_size: 0,
            average_processing_time: Duration::ZERO,
            failed_tasks: 0,
            circuit_breaker_open: false,
            memory_usage: None,
        }
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_tasks_processed == 0 {
            100.0
        } else {
            let successful = self.total_tasks_processed.saturating_sub(self.failed_tasks);
            (successful as f64 / self.total_tasks_processed as f64) * 100.0
        }
    }

    /// Check if the thread pool is under load
    pub fn is_under_load(&self) -> bool {
        self.queue_size > 100 || self.active_workers >= num_cpus::get() as u32
    }
}

impl Default for ThreadPoolMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Error types for embedding operations
#[derive(Debug, Clone, PartialEq)]
pub enum EmbeddingErrorType {
    /// General processing error
    ProcessingError,
    /// Operation timed out
    TimeoutError,
    /// Resource exhaustion (memory, CPU, etc.)
    ResourceError,
    /// Invalid configuration
    ConfigurationError,
    /// Circuit breaker is open
    CircuitBreakerOpen,
    /// Network or external service error
    ExternalServiceError,
    /// Document parsing error
    DocumentParsingError,
    /// Database operation error
    DatabaseError,
}

impl EmbeddingErrorType {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            EmbeddingErrorType::ProcessingError
                | EmbeddingErrorType::TimeoutError
                | EmbeddingErrorType::ResourceError
                | EmbeddingErrorType::ExternalServiceError
        )
    }

    /// Get error description
    pub fn description(&self) -> &'static str {
        match self {
            EmbeddingErrorType::ProcessingError => "Document processing failed",
            EmbeddingErrorType::TimeoutError => "Operation timed out",
            EmbeddingErrorType::ResourceError => "Insufficient resources",
            EmbeddingErrorType::ConfigurationError => "Invalid configuration",
            EmbeddingErrorType::CircuitBreakerOpen => "Circuit breaker is open",
            EmbeddingErrorType::ExternalServiceError => "External service error",
            EmbeddingErrorType::DocumentParsingError => "Document parsing failed",
            EmbeddingErrorType::DatabaseError => "Database operation failed",
        }
    }
}

/// Detailed error information for embedding operations
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingError {
    /// ID of the document that failed
    pub document_id: String,

    /// Type of error that occurred
    pub error_type: EmbeddingErrorType,

    /// Human-readable error message
    pub error_message: String,

    /// When the error occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Number of retry attempts
    pub retry_count: u32,

    /// Processing time before failure
    pub processing_time: Duration,
}

impl EmbeddingError {
    /// Create a new embedding error
    pub fn new(document_id: String, error_type: EmbeddingErrorType, error_message: String) -> Self {
        Self {
            document_id,
            error_type,
            error_message,
            timestamp: chrono::Utc::now(),
            retry_count: 0,
            processing_time: Duration::ZERO,
        }
    }

    /// Create error with retry information
    pub fn with_retry_info(mut self, retry_count: u32, processing_time: Duration) -> Self {
        self.retry_count = retry_count;
        self.processing_time = processing_time;
        self
    }

    /// Check if this error should trigger circuit breaker
    pub fn should_trigger_circuit_breaker(&self) -> bool {
        matches!(
            self.error_type,
            EmbeddingErrorType::ResourceError
                | EmbeddingErrorType::ExternalServiceError
                | EmbeddingErrorType::DatabaseError
        )
    }
}

/// Result of embedding processing operations
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingProcessingResult {
    /// Number of successfully processed documents
    pub processed_count: usize,

    /// Number of failed documents
    pub failed_count: usize,

    /// Total time taken for processing
    pub total_processing_time: Duration,

    /// Collection of errors that occurred
    pub errors: Vec<EmbeddingError>,

    /// Whether circuit breaker was triggered
    pub circuit_breaker_triggered: bool,

    /// Number of embeddings generated
    pub embeddings_generated: usize,
}

impl EmbeddingProcessingResult {
    /// Create new processing result
    pub fn new() -> Self {
        Self {
            processed_count: 0,
            failed_count: 0,
            total_processing_time: Duration::ZERO,
            errors: Vec::new(),
            circuit_breaker_triggered: false,
            embeddings_generated: 0,
        }
    }

    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        let total = self.processed_count + self.failed_count;
        if total == 0 {
            100.0
        } else {
            (self.processed_count as f64 / total as f64) * 100.0
        }
    }

    /// Check if processing was completely successful
    pub fn is_success(&self) -> bool {
        self.failed_count == 0 && !self.circuit_breaker_triggered
    }

    /// Check if processing was partially successful
    pub fn is_partial_success(&self) -> bool {
        self.processed_count > 0 && (self.failed_count > 0 || self.circuit_breaker_triggered)
    }
}

impl Default for EmbeddingProcessingResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of incremental document processing
#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalProcessingResult {
    /// Whether the document was processed
    pub processed: bool,

    /// Number of new embeddings created
    pub embeddings_created: usize,

    /// Number of existing embeddings updated
    pub embeddings_updated: usize,

    /// Content hash of the processed document
    pub content_hash: String,

    /// Time taken for processing
    pub processing_time: Duration,

    /// Whether the document was skipped (no changes)
    pub skipped: bool,
}

impl IncrementalProcessingResult {
    /// Create result for skipped document
    pub fn skipped(content_hash: String) -> Self {
        Self {
            processed: false,
            embeddings_created: 0,
            embeddings_updated: 0,
            content_hash,
            processing_time: Duration::ZERO,
            skipped: true,
        }
    }

    /// Create result for processed document
    pub fn processed(
        embeddings_created: usize,
        embeddings_updated: usize,
        content_hash: String,
        processing_time: Duration,
    ) -> Self {
        Self {
            processed: true,
            embeddings_created,
            embeddings_updated,
            content_hash,
            processing_time,
            skipped: false,
        }
    }

    /// Get total number of embeddings affected
    pub fn total_embeddings_affected(&self) -> usize {
        self.embeddings_created + self.embeddings_updated
    }
}

/// Result of batch incremental processing
#[derive(Debug, Clone, PartialEq)]
pub struct BatchIncrementalResult {
    /// Number of documents processed
    pub processed_count: usize,

    /// Number of documents skipped (no changes)
    pub skipped_count: usize,

    /// Total processing time
    pub total_processing_time: Duration,

    /// Total embeddings created across all documents
    pub total_embeddings_created: usize,

    /// Total embeddings updated across all documents
    pub total_embeddings_updated: usize,
}

impl BatchIncrementalResult {
    /// Create new batch incremental result
    pub fn new() -> Self {
        Self {
            processed_count: 0,
            skipped_count: 0,
            total_processing_time: Duration::ZERO,
            total_embeddings_created: 0,
            total_embeddings_updated: 0,
        }
    }

    /// Get total number of documents handled
    pub fn total_documents(&self) -> usize {
        self.processed_count + self.skipped_count
    }

    /// Check if any documents were processed
    pub fn has_changes(&self) -> bool {
        self.processed_count > 0
    }
}

impl Default for BatchIncrementalResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of retry processing
#[derive(Debug, Clone, PartialEq)]
pub struct RetryProcessingResult {
    /// Whether processing eventually succeeded
    pub succeeded: bool,

    /// Number of attempts made
    pub attempt_count: u32,

    /// Total time spent across all attempts
    pub total_time: Duration,

    /// Final error if all retries failed
    pub final_error: Option<EmbeddingError>,

    /// The generated embedding vector (if successful)
    pub embedding: Option<Vec<f32>>,
}

impl RetryProcessingResult {
    /// Create successful retry result
    pub fn success(attempt_count: u32, total_time: Duration, embedding: Vec<f32>) -> Self {
        Self {
            succeeded: true,
            attempt_count,
            total_time,
            final_error: None,
            embedding: Some(embedding),
        }
    }

    /// Create failed retry result
    pub fn failure(attempt_count: u32, total_time: Duration, final_error: EmbeddingError) -> Self {
        Self {
            succeeded: false,
            attempt_count,
            total_time,
            final_error: Some(final_error),
            embedding: None,
        }
    }

    /// Check if retries were exhausted
    pub fn retries_exhausted(&self) -> bool {
        !self.succeeded && self.final_error.is_some()
    }
}

/// Document embedding representation
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentEmbedding {
    /// ID of the document
    pub document_id: String,

    /// ID of the chunk (if chunked)
    pub chunk_id: Option<String>,

    /// Vector embedding values
    pub vector: Vec<f32>,

    /// Name of the embedding model used
    pub embedding_model: String,

    /// When the embedding was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Size of the original text chunk
    pub chunk_size: usize,

    /// Position of the chunk in the document
    pub chunk_position: Option<usize>,
}

impl DocumentEmbedding {
    /// Create a new document embedding
    pub fn new(document_id: String, vector: Vec<f32>, embedding_model: String) -> Self {
        Self {
            document_id,
            chunk_id: None,
            vector,
            embedding_model,
            created_at: chrono::Utc::now(),
            chunk_size: 0,
            chunk_position: None,
        }
    }

    /// Create embedding with chunk information
    pub fn with_chunk_info(
        mut self,
        chunk_id: String,
        chunk_size: usize,
        chunk_position: usize,
    ) -> Self {
        self.chunk_id = Some(chunk_id);
        self.chunk_size = chunk_size;
        self.chunk_position = Some(chunk_position);
        self
    }

    /// Get embedding dimensions
    pub fn dimensions(&self) -> usize {
        self.vector.len()
    }

    /// Check if this is a chunked embedding
    pub fn is_chunked(&self) -> bool {
        self.chunk_id.is_some()
    }

    /// Calculate vector magnitude (L2 norm)
    pub fn magnitude(&self) -> f32 {
        self.vector.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    /// Normalize the vector to unit length
    pub fn normalize(&self) -> Vec<f32> {
        let mag = self.magnitude();
        if mag == 0.0 {
            self.vector.clone()
        } else {
            self.vector.iter().map(|x| x / mag).collect()
        }
    }
}

/// Validate embedding configuration
pub async fn validate_embedding_config(config: &EmbeddingConfig) -> Result<()> {
    config.validate()?;

    // Additional async validation if needed
    if config.worker_count > num_cpus::get() * 4 {
        tracing::warn!(
            "Worker count ({}) significantly exceeds CPU cores ({})",
            config.worker_count,
            num_cpus::get()
        );
    }

    if config.batch_size > config.max_queue_size {
        return Err(anyhow::anyhow!("Batch size cannot exceed max queue size"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.worker_count, num_cpus::get());
        assert_eq!(config.batch_size, 16);
        assert_eq!(config.model_type, EmbeddingModel::LocalStandard);
        assert_eq!(config.privacy_mode, PrivacyMode::StrictLocal);
    }

    #[test]
    fn test_throughput_optimization() {
        let config = EmbeddingConfig::optimize_for_throughput();
        assert!(config.batch_size >= 32);
        assert!(config.max_queue_size >= 1000);
        assert_eq!(config.privacy_mode, PrivacyMode::StrictLocal);
    }

    #[test]
    fn test_latency_optimization() {
        let config = EmbeddingConfig::optimize_for_latency();
        assert!(config.batch_size <= 8);
        assert!(config.timeout_ms <= 10000);
        assert_eq!(config.model_type, EmbeddingModel::LocalMini);
    }

    #[test]
    fn test_resource_optimization() {
        let config = EmbeddingConfig::optimize_for_resources();
        assert_eq!(config.worker_count, 1);
        assert!(config.batch_size <= 16);
        assert_eq!(config.model_type, EmbeddingModel::LocalMini);
    }

    #[test]
    fn test_machine_specs() {
        let config = EmbeddingConfig::for_machine_specs(4, 8_589_934_592); // 4 cores, 8GB
        assert_eq!(config.worker_count, 4);
        assert_eq!(config.batch_size, 16);
        assert_eq!(config.model_type, EmbeddingModel::LocalStandard);
    }

    #[test]
    fn test_model_dimensions() {
        assert_eq!(EmbeddingModel::LocalMini.dimensions(), 256);
        assert_eq!(EmbeddingModel::LocalStandard.dimensions(), 768);
        assert_eq!(EmbeddingModel::LocalLarge.dimensions(), 1536);
    }

    #[test]
    fn test_privacy_modes() {
        assert!(!PrivacyMode::StrictLocal.allows_external());
        assert!(PrivacyMode::AllowExternalFallback.allows_external());
        assert!(PrivacyMode::HybridMode.allows_external());

        assert!(PrivacyMode::StrictLocal.is_strict());
        assert!(!PrivacyMode::AllowExternalFallback.is_strict());
    }

    #[test]
    fn test_config_validation() {
        let mut config = EmbeddingConfig::default();
        assert!(config.validate().is_ok());

        config.worker_count = 0;
        assert!(config.validate().is_err());

        config.worker_count = 2;
        config.batch_size = 0;
        assert!(config.validate().is_err());

        config.batch_size = 16;
        config.timeout_ms = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_metrics() {
        let metrics = ThreadPoolMetrics::new();
        assert_eq!(metrics.success_rate(), 100.0);
        assert!(!metrics.is_under_load());

        let mut loaded_metrics = metrics.clone();
        loaded_metrics.queue_size = 200;
        loaded_metrics.active_workers = num_cpus::get() as u32;
        assert!(loaded_metrics.is_under_load());
    }

    #[test]
    fn test_document_embedding() {
        let embedding = DocumentEmbedding::new(
            "doc1".to_string(),
            vec![0.1, 0.2, 0.3],
            "test-model".to_string(),
        );

        assert_eq!(embedding.dimensions(), 3);
        assert!(!embedding.is_chunked());
        assert!(embedding.magnitude() > 0.0);
    }

    #[test]
    fn test_processing_result() {
        let result = EmbeddingProcessingResult::new();
        assert_eq!(result.success_rate(), 100.0);
        assert!(result.is_success());

        let mut partial_result = result.clone();
        partial_result.processed_count = 5;
        partial_result.failed_count = 2;
        assert_eq!(partial_result.success_rate(), 5.0 / 7.0 * 100.0);
        assert!(partial_result.is_partial_success());
    }

    #[tokio::test]
    async fn test_async_validation() {
        let config = EmbeddingConfig::default();
        assert!(validate_embedding_config(&config).await.is_ok());

        let mut invalid_config = config.clone();
        invalid_config.batch_size = invalid_config.max_queue_size + 1;
        assert!(validate_embedding_config(&invalid_config).await.is_err());
    }
}
