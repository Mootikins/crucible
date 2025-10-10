// crates/crucible-mcp/src/embeddings/ollama.rs

//\! Ollama embedding provider implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::config::EmbeddingConfig;
use super::error::{EmbeddingError, EmbeddingResult};
use super::provider::{EmbeddingProvider, EmbeddingResponse};

/// Request structure for Ollama embedding API
#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

/// Response structure from Ollama embedding API
#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

/// Ollama embedding provider
///
/// Connects to Ollama API (local or remote) to generate embeddings.
/// Supports retry logic with exponential backoff and dimension validation.
pub struct OllamaProvider {
    client: Client,
    endpoint: String,
    model: String,
    expected_dimensions: usize,
    timeout_secs: u64,
    max_retries: u32,
    batch_size: usize,
}

impl OllamaProvider {
    /// Create a new Ollama provider from configuration
    pub fn new(config: EmbeddingConfig) -> EmbeddingResult<Self> {
        // Validate configuration
        config.validate()?;

        // Build HTTP client with timeout
        let mut client_builder = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs));

        // Accept self-signed certificates for local development servers
        if config.endpoint.contains("localhost") ||
           config.endpoint.contains("127.0.0.1") ||
           config.endpoint.contains(".terminal.") {
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }

        let client = client_builder
            .build()
            .map_err(|e| EmbeddingError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;

        let expected_dimensions = config.expected_dimensions();

        Ok(Self {
            client,
            endpoint: config.endpoint,
            model: config.model,
            expected_dimensions,
            timeout_secs: config.timeout_secs,
            max_retries: config.max_retries,
            batch_size: config.batch_size,
        })
    }

    /// Make a single embedding request with retry logic and exponential backoff
    async fn embed_with_retry(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match self.embed_single(text).await {
                Ok(response) => {
                    // Validate dimensions match expected
                    response.validate_dimensions(self.expected_dimensions)?;
                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e);

                    // Check if we should retry
                    if let Some(ref err) = last_error {
                        if !err.is_retryable() || attempt >= self.max_retries {
                            break;
                        }

                        // Calculate exponential backoff delay
                        let base_delay = err.retry_delay_secs().unwrap_or(1);
                        let delay_secs = base_delay * 2_u64.pow(attempt);

                        tracing::warn!(
                            "Embedding request failed (attempt {}/{}), retrying in {}s: {}",
                            attempt + 1,
                            self.max_retries + 1,
                            delay_secs,
                            err
                        );

                        tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                    }
                }
            }
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            EmbeddingError::Other("All retry attempts failed".to_string())
        }))
    }

    /// Make a single embedding request without retry
    async fn embed_single(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        let url = format!("{}/api/embeddings", self.endpoint);

        let request = OllamaEmbeddingRequest {
            model: self.model.clone(),
            prompt: text.to_string(),
        };

        tracing::debug!(
            "Sending embedding request to {} for {} chars",
            url,
            text.len()
        );

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    EmbeddingError::Timeout { timeout_secs: self.timeout_secs }
                } else {
                    EmbeddingError::HttpError(e)
                }
            })?;

        // Check response status
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(EmbeddingError::ProviderError {
                provider: "Ollama".to_string(),
                message: format!("HTTP {}: {}", status, error_text),
            });
        }

        // Parse response
        let ollama_response: OllamaEmbeddingResponse = response.json().await.map_err(|e| {
            EmbeddingError::InvalidResponse(format!("Failed to parse Ollama response: {}", e))
        })?;

        tracing::debug!(
            "Received embedding with {} dimensions",
            ollama_response.embedding.len()
        );

        Ok(EmbeddingResponse::new(
            ollama_response.embedding,
            self.model.clone(),
        ))
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        if text.is_empty() {
            return Err(EmbeddingError::InvalidResponse(
                "Cannot embed empty text".to_string()
            ));
        }

        self.embed_with_retry(text).await
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = Vec::with_capacity(texts.len());

        // Process in batches to avoid overwhelming the API
        for chunk in texts.chunks(self.batch_size) {
            // Process each item in chunk sequentially
            for text in chunk {
                let result = self.embed(text).await?;
                results.push(result);
            }

            // Small delay between batches to be nice to the API
            if texts.len() > self.batch_size {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        Ok(results)
    }

    fn provider_name(&self) -> &str {
        "Ollama"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn dimensions(&self) -> usize {
        self.expected_dimensions
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        // Try to embed a simple test string
        match self.embed_single("health check").await {
            Ok(response) => {
                // Verify dimensions are correct
                Ok(response.dimensions == self.expected_dimensions)
            }
            Err(e) => {
                tracing::warn!("Ollama health check failed: {}", e);
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> EmbeddingConfig {
        EmbeddingConfig::ollama(
            Some("https://llama.terminal.krohnos.io".to_string()),
            Some("nomic-embed-text".to_string()),
        )
    }

    #[test]
    fn test_provider_creation() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "Ollama");
        assert_eq!(provider.model_name(), "nomic-embed-text");
        assert_eq!(provider.dimensions(), 768);
    }

    #[test]
    fn test_provider_creation_with_invalid_config() {
        let mut config = create_test_config();
        config.timeout_secs = 0; // Invalid timeout

        let provider = OllamaProvider::new(config);
        assert!(provider.is_err());
    }

    #[tokio::test]
    async fn test_embed_empty_text() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config).unwrap();

        let result = provider.embed("").await;
        assert!(result.is_err());

        if let Err(e) = result {
            assert!(matches!(e, EmbeddingError::InvalidResponse(_)));
        }
    }

    #[test]
    fn test_ollama_request_serialization() {
        let request = OllamaEmbeddingRequest {
            model: "nomic-embed-text".to_string(),
            prompt: "test text".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("nomic-embed-text"));
        assert!(json.contains("test text"));
    }

    #[test]
    fn test_ollama_response_deserialization() {
        let json = r#"{"embedding": [0.1, 0.2, 0.3]}"#;
        let response: OllamaEmbeddingResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.embedding.len(), 3);
        assert_eq!(response.embedding[0], 0.1);
        assert_eq!(response.embedding[1], 0.2);
        assert_eq!(response.embedding[2], 0.3);
    }

    // Integration tests - these require a live Ollama instance
    // Run with: cargo test --package crucible-mcp --lib embeddings::ollama -- --ignored

    #[tokio::test]
    #[ignore]
    async fn test_embed_integration() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config).unwrap();

        let result = provider.embed("Hello, world!").await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.dimensions, 768);
        assert_eq!(response.model, "nomic-embed-text");
        assert!(!response.embedding.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_embed_batch_integration() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config).unwrap();

        let texts = vec![
            "First sentence".to_string(),
            "Second sentence".to_string(),
            "Third sentence".to_string(),
        ];

        let result = provider.embed_batch(texts).await;
        assert!(result.is_ok());

        let responses = result.unwrap();
        assert_eq!(responses.len(), 3);

        for response in &responses {
            assert_eq!(response.dimensions, 768);
            assert_eq!(response.model, "nomic-embed-text");
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_health_check_integration() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config).unwrap();

        let result = provider.health_check().await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
