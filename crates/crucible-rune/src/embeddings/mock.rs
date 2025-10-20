//! Mock embedding provider for testing

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use super::error::{EmbeddingError, EmbeddingResult};
use super::provider::{
    EmbeddingProvider, EmbeddingResponse, EmbeddingStats, ProviderCapabilities, ProviderInfo,
    utils,
};

/// Mock embedding provider for testing
pub struct MockEmbeddingProvider {
    model: String,
    dimensions: usize,
    max_tokens: usize,
    max_batch_size: usize,
    stats: Arc<tokio::sync::RwLock<EmbeddingStats>>,
    delay_ms: u64,
    failure_rate: f64,
    deterministic: bool,
}

impl MockEmbeddingProvider {
    /// Create a new mock provider
    pub fn new() -> Self {
        Self {
            model: "mock-model".to_string(),
            dimensions: 768,
            max_tokens: 8192,
            max_batch_size: 10,
            stats: Arc::new(tokio::sync::RwLock::new(EmbeddingStats::new())),
            delay_ms: 100,
            failure_rate: 0.0,
            deterministic: false,
        }
    }

    /// Create a mock provider with specific dimensions
    pub fn with_dimensions(dimensions: usize) -> Self {
        Self {
            dimensions,
            ..Self::new()
        }
    }

    /// Create a mock provider with custom settings
    pub fn with_settings(
        model: String,
        dimensions: usize,
        max_tokens: usize,
        max_batch_size: usize,
    ) -> Self {
        Self {
            model,
            dimensions,
            max_tokens,
            max_batch_size,
            stats: Arc::new(tokio::sync::RwLock::new(EmbeddingStats::new())),
            delay_ms: 100,
            failure_rate: 0.0,
            deterministic: false,
        }
    }

    /// Set artificial delay for requests
    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Set failure rate (0.0 to 1.0)
    pub fn with_failure_rate(mut self, failure_rate: f64) -> Self {
        self.failure_rate = failure_rate.clamp(0.0, 1.0);
        self
    }

    /// Set whether to generate deterministic embeddings
    pub fn deterministic(mut self, deterministic: bool) -> Self {
        self.deterministic = deterministic;
        self
    }

    /// Generate mock embedding vector
    fn generate_embedding(&self, text: &str) -> Vec<f32> {
        if self.deterministic {
            // Generate deterministic embedding based on text hash
            let hash = self.hash_text(text);
            (0..self.dimensions)
                .map(|i| {
                    let seed = hash.wrapping_mul(i as u64);
                    ((seed as f64) / (u64::MAX as f64) * 2.0 - 1.0) as f32
                })
                .collect()
        } else {
            // Generate random embedding
            (0..self.dimensions)
                .map(|_| rand::random::<f32>() * 2.0 - 1.0)
                .collect()
        }
    }

    /// Simple hash function for deterministic embeddings
    fn hash_text(&self, text: &str) -> u64 {
        let mut hash = 0u64;
        for byte in text.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }

    /// Estimate token count
    fn estimate_tokens(&self, text: &str) -> usize {
        utils::estimate_tokens_simple(text)
    }

    /// Simulate potential failure
    async fn maybe_fail(&self) -> EmbeddingResult<()> {
        if self.failure_rate > 0.0 && rand::random::<f64>() < self.failure_rate {
            let error_types = vec![
                EmbeddingError::NetworkError(
                    reqwest::Error::from(reqwest::ErrorKind::Request)
                ),
                EmbeddingError::TimeoutError { timeout_secs: 30 },
                EmbeddingError::ApiError {
                    message: "Simulated API error".to_string(),
                    status: 500,
                },
                EmbeddingError::ServiceUnavailable("Service temporarily unavailable".to_string()),
            ];

            let error = error_types[rand::random::<usize>() % error_types.len()];

            // Record failure in stats
            let mut stats = self.stats.write().await;
            stats.record_failure(error.category());

            return Err(error);
        }
        Ok(())
    }

    /// Get current statistics
    pub async fn get_stats(&self) -> EmbeddingStats {
        self.stats.read().await.clone()
    }

    /// Reset statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        stats.reset();
    }
}

impl Default for MockEmbeddingProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        let start_time = std::time::Instant::now();

        // Check for potential failure
        self.maybe_fail().await?;

        // Validate input
        if text.is_empty() {
            return Err(EmbeddingError::InvalidInput("Text cannot be empty".to_string()));
        }

        let token_count = self.estimate_tokens(text);
        if token_count > self.max_tokens {
            return Err(EmbeddingError::TooManyTokens {
                token_count,
                max_tokens: self.max_tokens,
            });
        }

        // Simulate processing delay
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }

        // Generate embedding
        let embedding = self.generate_embedding(text);
        let processing_time_ms = start_time.elapsed().as_millis() as u64;

        // Create response
        let response = EmbeddingResponse::new(
            embedding,
            token_count,
            processing_time_ms,
            self.model.clone(),
        );

        // Record success in stats
        let mut stats = self.stats.write().await;
        stats.record_success(1, token_count, processing_time_ms);

        Ok(response)
    }

    async fn embed_batch(&self, texts: &[String]) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        if texts.len() > self.max_batch_size {
            return Err(EmbeddingError::InvalidInput(format!(
                "Batch size {} exceeds maximum {}",
                texts.len(),
                self.max_batch_size
            )));
        }

        let mut responses = Vec::with_capacity(texts.len());

        for text in texts {
            let response = self.embed(text).await?;
            responses.push(response);
        }

        Ok(responses)
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "Mock Embedding Provider".to_string(),
            version: "1.0.0".to_string(),
            model: self.model.clone(),
            available_models: vec![self.model.clone()],
            max_batch_size: self.max_batch_size,
            max_tokens: self.max_tokens,
            dimensions: self.dimensions,
            capabilities: ProviderCapabilities {
                supports_batch: true,
                supports_streaming: false,
                supports_custom_models: false,
                accurate_token_count: false,
                supports_dimension_control: false,
                supports_async: true,
            },
            metadata: {
                let mut meta = HashMap::new();
                meta.insert("delay_ms".to_string(), serde_json::Value::Number(self.delay_ms.into()));
                meta.insert("failure_rate".to_string(), serde_json::Value::Number(self.failure_rate.into()));
                meta.insert("deterministic".to_string(), serde_json::Value::Bool(self.deterministic));
                meta
            },
        }
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        // Simulate health check with potential failure
        if rand::random::<f64>() < self.failure_rate {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<String>> {
        Ok(vec![self.model.clone()])
    }

    async fn model_available(&self, model: &str) -> EmbeddingResult<bool> {
        Ok(model == self.model)
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        self.estimate_tokens(text)
    }

    fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// Mock embedding provider that always fails
pub struct FailingMockProvider {
    model: String,
    error_type: EmbeddingError,
}

impl FailingMockProvider {
    /// Create a new failing mock provider
    pub fn new(error_type: EmbeddingError) -> Self {
        Self {
            model: "failing-model".to_string(),
            error_type,
        }
    }

    /// Create a provider that fails with network errors
    pub fn network_error() -> Self {
        Self::new(EmbeddingError::NetworkError(
            reqwest::Error::from(reqwest::ErrorKind::Request)
        ))
    }

    /// Create a provider that fails with timeout errors
    pub fn timeout_error() -> Self {
        Self::new(EmbeddingError::TimeoutError { timeout_secs: 30 })
    }

    /// Create a provider that fails with API errors
    pub fn api_error(status: u16) -> Self {
        Self::new(EmbeddingError::ApiError {
            message: "Simulated API error".to_string(),
            status,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for FailingMockProvider {
    async fn embed(&self, _text: &str) -> EmbeddingResult<EmbeddingResponse> {
        Err(self.error_type.clone())
    }

    async fn embed_batch(&self, _texts: &[String]) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        Err(self.error_type.clone())
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "Failing Mock Provider".to_string(),
            version: "1.0.0".to_string(),
            model: self.model.clone(),
            available_models: vec![self.model.clone()],
            max_batch_size: 0,
            max_tokens: 0,
            dimensions: 0,
            capabilities: ProviderCapabilities {
                supports_batch: false,
                supports_streaming: false,
                supports_custom_models: false,
                accurate_token_count: false,
                supports_dimension_control: false,
                supports_async: false,
            },
            metadata: HashMap::new(),
        }
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        Ok(false)
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<String>> {
        Ok(vec![self.model.clone()])
    }

    async fn model_available(&self, model: &str) -> EmbeddingResult<bool> {
        Ok(model == self.model)
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        utils::estimate_tokens_simple(text)
    }

    fn max_tokens(&self) -> usize {
        0
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_basic() {
        let provider = MockEmbeddingProvider::new();
        let result = provider.embed("Hello, world!").await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.dimensions(), 768);
        assert!(response.is_valid());
        assert_eq!(response.model, "mock-model");
    }

    #[tokio::test]
    async fn test_mock_provider_custom_dimensions() {
        let provider = MockEmbeddingProvider::with_dimensions(1536);
        let result = provider.embed("Hello, world!").await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.dimensions(), 1536);
        assert_eq!(provider.dimensions(), 1536);
    }

    #[tokio::test]
    async fn test_mock_provider_deterministic() {
        let provider = MockEmbeddingProvider::new().deterministic(true);
        let text = "Hello, world!";

        let result1 = provider.embed(text).await.unwrap();
        let result2 = provider.embed(text).await.unwrap();

        assert_eq!(result1.embedding, result2.embedding);
    }

    #[tokio::test]
    async fn test_mock_provider_batch() {
        let provider = MockEmbeddingProvider::with_settings(
            "test-model".to_string(),
            512,
            1000,
            5,
        );

        let texts = vec![
            "Hello".to_string(),
            "World".to_string(),
            "Test".to_string(),
        ];

        let results = provider.embed_batch(&texts).await.unwrap();
        assert_eq!(results.len(), 3);

        for response in results {
            assert_eq!(response.dimensions(), 512);
            assert!(response.is_valid());
        }
    }

    #[tokio::test]
    async fn test_mock_provider_failure_rate() {
        let provider = MockEmbeddingProvider::new()
            .with_failure_rate(1.0) // Always fail
            .with_delay(0);

        let result = provider.embed("Hello, world!").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_delay() {
        let provider = MockEmbeddingProvider::new().with_delay(100);
        let start = std::time::Instant::now();

        let result = provider.embed("Hello, world!").await.unwrap();

        let elapsed = start.elapsed();
        assert!(elapsed >= std::time::Duration::from_millis(100));
        assert!(result.processing_time_ms >= 100);
    }

    #[tokio::test]
    async fn test_mock_provider_too_many_tokens() {
        let provider = MockEmbeddingProvider::new().with_max_tokens(5);
        let text = "This is a very long text that should exceed the token limit";

        let result = provider.embed(text).await;
        assert!(result.is_err());

        if let Err(EmbeddingError::TooManyTokens { token_count, max_tokens }) = result {
            assert!(token_count > max_tokens);
            assert_eq!(max_tokens, 5);
        } else {
            panic!("Expected TooManyTokens error");
        }
    }

    #[tokio::test]
    async fn test_mock_provider_stats() {
        let provider = MockEmbeddingProvider::new();

        // Make some requests
        for _ in 0..5 {
            let _ = provider.embed("test").await;
        }

        let stats = provider.get_stats().await;
        assert_eq!(stats.total_requests, 5);
        assert_eq!(stats.successful_requests, 5);
        assert_eq!(stats.failed_requests, 0);
        assert_eq!(stats.success_rate(), 1.0);

        // Reset stats
        provider.reset_stats().await;
        let stats = provider.get_stats().await;
        assert_eq!(stats.total_requests, 0);
    }

    #[tokio::test]
    async fn test_mock_provider_health_check() {
        let provider = MockEmbeddingProvider::new();
        let healthy = provider.health_check().await.unwrap();
        assert!(healthy);

        let failing_provider = MockEmbeddingProvider::new()
            .with_failure_rate(1.0);
        let healthy = failing_provider.health_check().await.unwrap();
        assert!(!healthy);
    }

    #[tokio::test]
    async fn test_failing_mock_provider() {
        let provider = FailingMockProvider::network_error();
        let result = provider.embed("test").await;
        assert!(result.is_err());

        if let Err(EmbeddingError::NetworkError(_)) = result {
            // Expected error type
        } else {
            panic!("Expected NetworkError");
        }
    }

    #[tokio::test]
    async fn test_provider_info() {
        let provider = MockEmbeddingProvider::with_settings(
            "test-model".to_string(),
            1024,
            5000,
            20,
        );

        let info = provider.provider_info();
        assert_eq!(info.name, "Mock Embedding Provider");
        assert_eq!(info.model, "test-model");
        assert_eq!(info.dimensions, 1024);
        assert_eq!(info.max_tokens, 5000);
        assert_eq!(info.max_batch_size, 20);
        assert!(info.capabilities.supports_batch);
        assert!(!info.capabilities.supports_streaming);
    }

    #[tokio::test]
    async fn test_empty_text() {
        let provider = MockEmbeddingProvider::new();
        let result = provider.embed("").await;
        assert!(result.is_err());

        if let Err(EmbeddingError::InvalidInput(msg)) = result {
            assert!(msg.contains("cannot be empty"));
        } else {
            panic!("Expected InvalidInput error");
        }
    }
}