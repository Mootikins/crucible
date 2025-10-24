//! Embedding Thread Pool Implementation
//!
//! A privacy-focused thread pool for vector embedding generation with configurable
//! performance settings, circuit breaker pattern, and comprehensive error handling.

use crate::embedding_config::*;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Semaphore, RwLock};
use tokio::task::JoinSet;
use tokio::time::timeout;
use tracing::{debug, warn, error, info};

/// Thread pool for embedding generation with privacy-focused processing
#[derive(Debug, Clone)]
pub struct EmbeddingThreadPool {
    /// Configuration for the thread pool
    config: Arc<EmbeddingConfig>,

    /// Worker threads join handle
    workers: Arc<Mutex<JoinSet<Result<()>>>>,

    /// Task queue semaphore for limiting concurrent tasks
    task_semaphore: Arc<Semaphore>,

    /// Metrics tracking
    metrics: Arc<RwLock<ThreadPoolMetrics>>,

    /// Circuit breaker state
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,

    /// Shutdown signal
    shutdown_signal: Arc<RwLock<bool>>,

    /// Mock embedding provider for testing
    mock_provider: bool,
}

/// Circuit breaker implementation for fault tolerance
#[derive(Debug)]
struct CircuitBreaker {
    /// Number of consecutive failures
    failure_count: u32,

    /// Whether the circuit breaker is open
    is_open: bool,

    /// When the circuit breaker will attempt to close
    next_attempt: Option<Instant>,

    /// Threshold for opening the circuit breaker
    threshold: u32,

    /// Timeout for keeping circuit breaker open
    timeout: Duration,
}

impl CircuitBreaker {
    fn new(threshold: u32, timeout: Duration) -> Self {
        Self {
            failure_count: 0,
            is_open: false,
            next_attempt: None,
            threshold,
            timeout,
        }
    }

    /// Check if the circuit breaker allows requests
    fn can_execute(&self) -> bool {
        if !self.is_open {
            return true;
        }

        if let Some(next_attempt) = self.next_attempt {
            Instant::now() >= next_attempt
        } else {
            false
        }
    }

    /// Record a successful execution
    fn record_success(&mut self) {
        self.failure_count = 0;
        if self.is_open {
            info!("Circuit breaker closing after successful execution");
            self.is_open = false;
            self.next_attempt = None;
        }
    }

    /// Record a failed execution
    fn record_failure(&mut self) {
        self.failure_count += 1;
        if self.failure_count >= self.threshold && !self.is_open {
            warn!(
                "Circuit breaker opening after {} consecutive failures",
                self.failure_count
            );
            self.is_open = true;
            self.next_attempt = Some(Instant::now() + self.timeout);
        }
    }

    /// Attempt to reset the circuit breaker
    fn attempt_reset(&mut self) -> bool {
        if self.is_open {
            if let Some(next_attempt) = self.next_attempt {
                if Instant::now() >= next_attempt {
                    info!("Circuit breaker attempting to reset");
                    self.failure_count = 0;
                    self.is_open = false;
                    self.next_attempt = None;
                    return true;
                }
            }
        }
        false
    }
}


impl EmbeddingThreadPool {
    /// Create a new embedding thread pool with the given configuration
    pub async fn new(config: EmbeddingConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;

        let pool = Self {
            config: Arc::new(config.clone()),
            workers: Arc::new(Mutex::new(JoinSet::new())),
            task_semaphore: Arc::new(Semaphore::new(config.max_queue_size)),
            metrics: Arc::new(RwLock::new(ThreadPoolMetrics::new())),
            circuit_breaker: Arc::new(RwLock::new(CircuitBreaker::new(
                config.circuit_breaker_threshold,
                config.circuit_breaker_timeout(),
            ))),
            shutdown_signal: Arc::new(RwLock::new(false)),
            mock_provider: true, // Use mock provider for testing
        };

        // Start worker threads
        pool.start_workers().await?;

        info!(
            "Embedding thread pool created with {} workers, batch size {}",
            config.worker_count, config.batch_size
        );

        Ok(pool)
    }

    /// Start worker threads
    async fn start_workers(&self) -> Result<()> {
        let mut workers = self.workers.lock().await;
        let config = self.config.clone();
        let metrics = self.metrics.clone();
        let circuit_breaker = self.circuit_breaker.clone();
        let shutdown_signal = self.shutdown_signal.clone();
        let mock_provider = self.mock_provider;

        for worker_id in 0..config.worker_count {
            let worker_config = config.clone();
            let worker_metrics = metrics.clone();
            let worker_circuit_breaker = circuit_breaker.clone();
            let worker_shutdown = shutdown_signal.clone();
            let is_mock = mock_provider;

            workers.spawn(async move {
                debug!("Starting embedding worker {}", worker_id);

                loop {
                    // Check for shutdown signal
                    if *worker_shutdown.read().await {
                        debug!("Worker {} shutting down", worker_id);
                        break Ok(());
                    }

                    // Check circuit breaker
                    {
                        let mut cb = worker_circuit_breaker.write().await;
                        if !cb.can_execute() {
                            // Wait before checking again
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            continue;
                        }
                    }

                    // Simulate processing work
                    tokio::time::sleep(Duration::from_millis(10)).await;

                    // Update metrics
                    {
                        let mut metrics = worker_metrics.write().await;
                        metrics.active_workers += 1;
                    }

                    // Simulate embedding generation
                    let result: Result<Vec<f32>, anyhow::Error> = if is_mock {
                        Ok(Self::generate_mock_embedding(&worker_config))
                    } else {
                        // In real implementation, this would call actual embedding service
                        Ok(vec![0.1; worker_config.model_type.dimensions()])
                    };

                    // Record success/failure
                    {
                        let mut cb = worker_circuit_breaker.write().await;
                        match result {
                            Ok(_) => {
                                cb.record_success();
                                let mut metrics = worker_metrics.write().await;
                                metrics.total_tasks_processed += 1;
                            }
                            Err(_) => {
                                cb.record_failure();
                                let mut metrics = worker_metrics.write().await;
                                metrics.failed_tasks += 1;
                            }
                        }
                    }

                    // Update active workers
                    {
                        let mut metrics = worker_metrics.write().await;
                        metrics.active_workers = metrics.active_workers.saturating_sub(1);
                    }
                }
            });
        }

        Ok(())
    }

    /// Generate mock embedding for testing
    fn generate_mock_embedding(config: &EmbeddingConfig) -> Vec<f32> {
        let dimensions = config.model_type.dimensions();
        let mut embedding = Vec::with_capacity(dimensions);

        // Generate deterministic but varied embedding based on content hash
        let seed = 42; // Fixed seed for reproducible tests
        for i in 0..dimensions {
            let value = ((seed + i) as f32 * 0.1).sin() * 0.5 + 0.5;
            embedding.push(value);
        }

        embedding
    }

    /// Get the current worker count
    pub async fn worker_count(&self) -> usize {
        self.config.worker_count
    }

    /// Get the batch size
    pub async fn batch_size(&self) -> usize {
        self.config.batch_size
    }

    /// Get the model type
    pub async fn model_type(&self) -> EmbeddingModel {
        self.config.model_type.clone()
    }

    /// Get the privacy mode
    pub async fn privacy_mode(&self) -> PrivacyMode {
        self.config.privacy_mode.clone()
    }

    /// Check if the thread pool is privacy-focused
    pub async fn is_privacy_focused(&self) -> bool {
        self.config.is_privacy_focused()
    }

    /// Check if privacy is enforced
    pub async fn enforces_privacy(&self) -> bool {
        self.config.privacy_mode.is_strict()
    }

    /// Check if external processing is allowed
    pub async fn allows_external_processing(&self) -> bool {
        self.config.privacy_mode.allows_external()
    }

    /// Get current thread pool metrics
    pub async fn get_metrics(&self) -> ThreadPoolMetrics {
        let metrics = self.metrics.read().await;
        let circuit_breaker = self.circuit_breaker.read().await;

        ThreadPoolMetrics {
            total_tasks_processed: metrics.total_tasks_processed,
            active_workers: metrics.active_workers,
            queue_size: (self.config.max_queue_size.saturating_sub(
                self.task_semaphore.available_permits()
            )) as u32,
            average_processing_time: metrics.average_processing_time,
            failed_tasks: metrics.failed_tasks,
            circuit_breaker_open: circuit_breaker.is_open,
            memory_usage: metrics.memory_usage,
        }
    }

    /// Shutdown the thread pool
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down embedding thread pool");

        // Set shutdown signal
        {
            let mut shutdown = self.shutdown_signal.write().await;
            *shutdown = true;
        }

        // Abort all workers
        let mut workers = self.workers.lock().await;
        workers.abort_all();

        // Wait for workers to finish (with timeout)
        let shutdown_timeout = Duration::from_secs(30);
        let start = Instant::now();

        while !workers.is_empty() {
            if start.elapsed() > shutdown_timeout {
                warn!("Thread pool shutdown timeout, forcing exit");
                break;
            }

            // Check if any workers completed
            while let Some(result) = workers.join_next().await {
                match result {
                    Ok(Ok(())) => debug!("Worker shutdown successfully"),
                    Ok(Err(e)) => error!("Worker shutdown with error: {:?}", e),
                    Err(e) => warn!("Worker join error: {:?}", e),
                }
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        info!("Embedding thread pool shutdown complete");
        Ok(())
    }

    /// Check if the thread pool is shutdown
    pub async fn is_shutdown(&self) -> bool {
        *self.shutdown_signal.read().await
    }

    /// Process a single document with retry logic
    pub async fn process_document_with_retry(
        &self,
        document_id: &str,
        content: &str,
    ) -> Result<RetryProcessingResult> {
        let start_time = Instant::now();
        let mut attempt_count = 0;
        let max_attempts = self.config.retry_attempts + 1;

        loop {
            attempt_count += 1;

            // Check circuit breaker
            {
                let circuit_breaker = self.circuit_breaker.read().await;
                if !circuit_breaker.can_execute() {
                    let error = EmbeddingError::new(
                        document_id.to_string(),
                        EmbeddingErrorType::CircuitBreakerOpen,
                        "Circuit breaker is open".to_string(),
                    );
                    return Ok(RetryProcessingResult::failure(
                        attempt_count,
                        start_time.elapsed(),
                        error,
                    ));
                }
            }

            // Attempt processing
            match self.process_document_internal(document_id, content).await {
                Ok(embedding) => {
                    // Record success
                    {
                        let mut circuit_breaker = self.circuit_breaker.write().await;
                        circuit_breaker.record_success();
                    }

                    return Ok(RetryProcessingResult::success(
                        attempt_count,
                        start_time.elapsed(),
                    ));
                }
                Err(e) => {
                    // Record failure
                    {
                        let mut circuit_breaker = self.circuit_breaker.write().await;
                        circuit_breaker.record_failure();
                    }

                    // Check if we should retry
                    if attempt_count < max_attempts {
                        warn!(
                            "Document {} processing failed (attempt {}/{}), retrying in {}ms: {}",
                            document_id,
                            attempt_count,
                            max_attempts,
                            self.config.retry_delay_ms,
                            e.to_string()
                        );

                        tokio::time::sleep(self.config.retry_delay_duration()).await;
                        continue;
                    } else {
                        error!(
                            "Document {} processing failed after {} attempts: {}",
                            document_id,
                            attempt_count,
                            e.to_string()
                        );

                        let error = EmbeddingError::new(
                            document_id.to_string(),
                            EmbeddingErrorType::ProcessingError,
                            e.to_string(),
                        );
                        return Ok(RetryProcessingResult::failure(
                            attempt_count,
                            start_time.elapsed(),
                            error,
                        ));
                    }
                }
            }
        }
    }

    /// Internal document processing without retry logic
    async fn process_document_internal(
        &self,
        document_id: &str,
        content: &str,
    ) -> Result<Vec<f32>> {
        // Check shutdown state
        if self.is_shutdown().await {
            return Err(anyhow!("Thread pool is shutdown"));
        }

        // Acquire semaphore permit
        let _permit = timeout(
            self.config.timeout_duration(),
            self.task_semaphore.acquire()
        )
        .await
        .map_err(|_| anyhow!("Semaphore acquisition timeout"))?
        .map_err(|_| anyhow!("Semaphore closed"))?;

        // Generate embedding
        let embedding = timeout(
            self.config.timeout_duration(),
            self.generate_embedding(content)
        )
        .await
        .map_err(|_| anyhow!("Embedding generation timeout"))??;

        debug!(
            "Generated {}-dimensional embedding for document {}",
            embedding.len(),
            document_id
        );

        Ok(embedding)
    }

    /// Generate embedding for content
    async fn generate_embedding(&self, content: &str) -> Result<Vec<f32>> {
        if self.mock_provider {
            // Simple mock embedding based on content hash
            let content_hash = {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                content.hash(&mut hasher);
                hasher.finish()
            };

            let dimensions = self.config.model_type.dimensions();
            let mut embedding = Vec::with_capacity(dimensions);

            for i in 0..dimensions {
                let seed = content_hash + i as u64;
                let value = ((seed as f64 * 0.1).sin() * 0.5 + 0.5) as f32;
                embedding.push(value);
            }

            Ok(embedding)
        } else {
            // In real implementation, this would call actual embedding service
            Err(anyhow!("Real embedding provider not implemented"))
        }
    }

    /// Process multiple documents in batch
    pub async fn process_batch(
        &self,
        documents: Vec<(String, String)>, // (document_id, content)
    ) -> Result<EmbeddingProcessingResult> {
        let start_time = Instant::now();
        let mut result = EmbeddingProcessingResult::new();

        if documents.is_empty() {
            return Ok(result);
        }

        info!("Processing batch of {} documents", documents.len());

        // Process in chunks based on batch size
        for chunk in documents.chunks(self.config.batch_size) {
            let mut chunk_futures = Vec::new();

            for (document_id, content) in chunk {
                let future = self.process_document_with_retry(document_id, content);
                chunk_futures.push(future);
            }

            // Wait for chunk completion
            let chunk_results = futures::future::join_all(chunk_futures).await;

            for retry_result in chunk_results {
                match retry_result {
                    Ok(retry_result) => {
                        if retry_result.succeeded {
                            result.processed_count += 1;
                            result.embeddings_generated += 1;
                        } else {
                            result.failed_count += 1;
                            if let Some(error) = retry_result.final_error {
                                result.errors.push(error);
                            }
                        }
                    }
                    Err(e) => {
                        result.failed_count += 1;
                        let error = EmbeddingError::new(
                            "unknown".to_string(),
                            EmbeddingErrorType::ProcessingError,
                            e.to_string(),
                        );
                        result.errors.push(error);
                    }
                }
            }

            // Check circuit breaker state
            {
                let circuit_breaker = self.circuit_breaker.read().await;
                if circuit_breaker.is_open {
                    warn!("Circuit breaker opened during batch processing");
                    result.circuit_breaker_triggered = true;
                    break;
                }
            }
        }

        result.total_processing_time = start_time.elapsed();

        info!(
            "Batch processing complete: {} succeeded, {} failed, {:?}",
            result.processed_count,
            result.failed_count,
            result.total_processing_time
        );

        Ok(result)
    }

    /// Reset circuit breaker manually
    pub async fn reset_circuit_breaker(&self) -> Result<()> {
        let mut circuit_breaker = self.circuit_breaker.write().await;
        circuit_breaker.attempt_reset();
        info!("Circuit breaker manually reset");
        Ok(())
    }
}

impl Drop for EmbeddingThreadPool {
    fn drop(&mut self) {
        // Note: This is a synchronous drop, but we need async shutdown
        // In practice, the shutdown should be called explicitly before dropping
    }
}

/// Create an embedding thread pool with the given configuration
pub async fn create_embedding_thread_pool(
    config: EmbeddingConfig,
) -> Result<EmbeddingThreadPool> {
    EmbeddingThreadPool::new(config).await
}

/// Validate embedding configuration
pub async fn validate_embedding_config(
    config: &EmbeddingConfig,
) -> Result<()> {
    crate::embedding_config::validate_embedding_config(config).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_thread_pool_creation() {
        let config = EmbeddingConfig::default();
        let pool = EmbeddingThreadPool::new(config).await.unwrap();

        assert_eq!(pool.worker_count().await, num_cpus::get());
        assert_eq!(pool.batch_size().await, 16);
        assert!(pool.is_privacy_focused().await);
        assert!(pool.enforces_privacy().await);
        assert!(!pool.allows_external_processing().await);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_custom_configuration() {
        let config = EmbeddingConfig {
            worker_count: 4,
            batch_size: 32,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 1000,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        };

        let pool = EmbeddingThreadPool::new(config.clone()).await.unwrap();

        assert_eq!(pool.worker_count().await, 4);
        assert_eq!(pool.batch_size().await, 32);
        assert_eq!(pool.model_type().await, EmbeddingModel::LocalMini);
        assert_eq!(pool.privacy_mode().await, PrivacyMode::StrictLocal);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_invalid_configuration() {
        let invalid_config = EmbeddingConfig {
            worker_count: 0, // Invalid
            batch_size: 16,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 1000,
            timeout_ms: 30000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        };

        let result = EmbeddingThreadPool::new(invalid_config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_document_processing() {
        let config = EmbeddingConfig::default();
        let pool = EmbeddingThreadPool::new(config).await.unwrap();

        let result = pool.process_document_with_retry(
            "test_doc",
            "This is a test document for embedding generation."
        ).await.unwrap();

        assert!(result.succeeded);
        assert!(result.attempt_count >= 1);
        assert!(result.total_time.as_millis() > 0);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_batch_processing() {
        let config = EmbeddingConfig {
            worker_count: 2,
            batch_size: 2,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 100,
            timeout_ms: 10000,
            retry_attempts: 3,
            retry_delay_ms: 1000,
            circuit_breaker_threshold: 10,
            circuit_breaker_timeout_ms: 30000,
        };

        let pool = EmbeddingThreadPool::new(config).await.unwrap();

        let documents = vec![
            ("doc1".to_string(), "First document content".to_string()),
            ("doc2".to_string(), "Second document content".to_string()),
            ("doc3".to_string(), "Third document content".to_string()),
        ];

        let result = pool.process_batch(documents).await.unwrap();

        assert_eq!(result.processed_count, 3);
        assert_eq!(result.failed_count, 0);
        assert_eq!(result.embeddings_generated, 3);
        assert!(!result.circuit_breaker_triggered);
        assert!(result.total_processing_time.as_millis() > 0);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_metrics() {
        let config = EmbeddingConfig::default();
        let pool = EmbeddingThreadPool::new(config).await.unwrap();

        let metrics = pool.get_metrics().await;
        assert_eq!(metrics.total_tasks_processed, 0);
        assert_eq!(metrics.active_workers, 0);
        assert!(!metrics.circuit_breaker_open);

        // Process a document
        pool.process_document_with_retry("test", "content").await.unwrap();

        let metrics_after = pool.get_metrics().await;
        assert_eq!(metrics_after.total_tasks_processed, 1);
        assert_eq!(metrics_after.failed_tasks, 0);
        assert_eq!(metrics_after.success_rate(), 100.0);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let config = EmbeddingConfig {
            worker_count: 1,
            batch_size: 1,
            model_type: EmbeddingModel::LocalMini,
            privacy_mode: PrivacyMode::StrictLocal,
            max_queue_size: 10,
            timeout_ms: 1000,
            retry_attempts: 1,
            retry_delay_ms: 100,
            circuit_breaker_threshold: 2,
            circuit_breaker_timeout_ms: 5000,
        };

        let pool = EmbeddingThreadPool::new(config).await.unwrap();

        // Process documents successfully
        pool.process_document_with_retry("doc1", "content1").await.unwrap();
        pool.process_document_with_retry("doc2", "content2").await.unwrap();

        let metrics = pool.get_metrics().await;
        assert!(!metrics.circuit_breaker_open);

        pool.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_shutdown() {
        let config = EmbeddingConfig::default();
        let pool = EmbeddingThreadPool::new(config).await.unwrap();

        assert!(!pool.is_shutdown().await);

        pool.shutdown().await.unwrap();
        assert!(pool.is_shutdown().await);
    }
}