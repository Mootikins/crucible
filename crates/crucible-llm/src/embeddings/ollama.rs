// crates/crucible-mcp/src/embeddings/ollama.rs

//\! Ollama embedding provider implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::config::EmbeddingConfig;
use super::error::{EmbeddingError, EmbeddingResult};
use super::provider::EmbeddingResponse;
use crucible_core::enrichment::EmbeddingProvider;

/// Request structure for Ollama legacy embedding API (/api/embeddings)
#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

/// Response structure from Ollama legacy embedding API
#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

/// Request structure for Ollama batch embedding API (/api/embed)
#[derive(Debug, Serialize)]
struct OllamaBatchEmbeddingRequest {
    model: String,
    input: Vec<String>,
}

/// Response structure from Ollama batch embedding API
#[derive(Debug, Deserialize)]
struct OllamaBatchEmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

/// Response structure from Ollama /api/tags endpoint
#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelInfo>,
}

/// Model information from Ollama /api/tags
#[derive(Debug, Deserialize)]
struct OllamaModelInfo {
    name: String,
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
        // Validate configuration (From impl handles error conversion)
        config.validate()?;

        let timeout_secs = config.timeout_secs();

        // Build HTTP client with timeout
        let mut client_builder = Client::builder().timeout(Duration::from_secs(timeout_secs));

        let endpoint = config.endpoint();

        // Accept self-signed certificates for local development servers
        if endpoint.contains("localhost")
            || endpoint.contains("127.0.0.1")
            || endpoint.contains(".terminal.")
        {
            client_builder = client_builder.danger_accept_invalid_certs(true);
        }

        let client = client_builder.build().map_err(|e| {
            EmbeddingError::ConfigError(format!("Failed to create HTTP client: {}", e))
        })?;

        // Get expected dimensions based on provider and model
        let expected_dimensions = super::config::expected_dimensions_for_model(
            &config.provider_type(),
            config.model_name(),
        );

        // Get batch size from config (default 50 for ~7x speedup)
        let batch_size = config.batch_size();

        tracing::debug!(
            "OllamaProvider initialized: endpoint={}, model={}, batch_size={}",
            endpoint,
            config.model_name(),
            batch_size
        );

        Ok(Self {
            client,
            endpoint,
            model: config.model_name().to_string(),
            expected_dimensions,
            timeout_secs,
            max_retries: config.retry_attempts(),
            batch_size,
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
        Err(last_error
            .unwrap_or_else(|| EmbeddingError::Other("All retry attempts failed".to_string())))
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

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    EmbeddingError::Timeout {
                        timeout_secs: self.timeout_secs,
                    }
                } else {
                    EmbeddingError::HttpError(e)
                }
            })?;

        // Check response status
        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
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

    /// Make a batch embedding request using /api/embed endpoint
    ///
    /// This is ~7x faster than individual requests due to reduced HTTP overhead.
    /// Falls back to sequential requests if batch fails.
    async fn embed_batch_native(
        &self,
        texts: Vec<String>,
    ) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let url = format!("{}/api/embed", self.endpoint);

        let request = OllamaBatchEmbeddingRequest {
            model: self.model.clone(),
            input: texts.clone(),
        };

        tracing::debug!(
            "Sending batch embedding request to {} for {} texts",
            url,
            texts.len()
        );

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    EmbeddingError::Timeout {
                        timeout_secs: self.timeout_secs,
                    }
                } else {
                    EmbeddingError::HttpError(e)
                }
            })?;

        // Check response status
        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(EmbeddingError::ProviderError {
                provider: "Ollama".to_string(),
                message: format!("HTTP {}: {}", status, error_text),
            });
        }

        // Parse batch response
        let batch_response: OllamaBatchEmbeddingResponse = response.json().await.map_err(|e| {
            EmbeddingError::InvalidResponse(format!("Failed to parse Ollama batch response: {}", e))
        })?;

        // Verify we got the right number of embeddings
        if batch_response.embeddings.len() != texts.len() {
            return Err(EmbeddingError::InvalidResponse(format!(
                "Expected {} embeddings, got {}",
                texts.len(),
                batch_response.embeddings.len()
            )));
        }

        tracing::debug!(
            "Received {} embeddings with {} dimensions each",
            batch_response.embeddings.len(),
            batch_response
                .embeddings
                .first()
                .map(|e| e.len())
                .unwrap_or(0)
        );

        // Convert to EmbeddingResponse objects
        let results: Vec<EmbeddingResponse> = batch_response
            .embeddings
            .into_iter()
            .map(|embedding| EmbeddingResponse::new(embedding, self.model.clone()))
            .collect();

        // Validate dimensions on first embedding
        if let Some(first) = results.first() {
            first.validate_dimensions(self.expected_dimensions)?;
        }

        Ok(results)
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        if text.is_empty() {
            return Err(
                EmbeddingError::InvalidResponse("Cannot embed empty text".to_string()).into(),
            );
        }

        let response = self.embed_with_retry(text).await?;
        Ok(response.embedding)
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Filter out empty texts, converting to owned strings for internal API
        let non_empty: Vec<String> = texts
            .iter()
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect();
        if non_empty.is_empty() {
            return Ok(Vec::new());
        }

        // Use legacy single-request mode if batch_size is 1
        if self.batch_size <= 1 {
            let mut results = Vec::with_capacity(non_empty.len());
            for text in &non_empty {
                let response = self.embed_with_retry(text).await?;
                results.push(response.embedding);
            }
            return Ok(results);
        }

        // Use native batch endpoint (/api/embed) for maximum throughput
        const MAX_SINGLE_BATCH: usize = 256;

        if non_empty.len() <= MAX_SINGLE_BATCH {
            match self.embed_batch_native(non_empty.clone()).await {
                Ok(batch_results) => {
                    return Ok(batch_results.into_iter().map(|r| r.embedding).collect())
                }
                Err(e) => {
                    tracing::warn!("Batch embedding failed, falling back to chunked: {}", e);
                }
            }
        }

        // Chunked processing for very large batches or fallback
        let mut results = Vec::with_capacity(non_empty.len());

        for chunk in non_empty.chunks(self.batch_size) {
            let chunk_texts: Vec<String> = chunk.to_vec();

            match self.embed_batch_native(chunk_texts.clone()).await {
                Ok(batch_results) => {
                    results.extend(batch_results.into_iter().map(|r| r.embedding));
                }
                Err(e) => {
                    tracing::warn!("Batch embedding failed, falling back to sequential: {}", e);
                    for text in &chunk_texts {
                        let response = self.embed_with_retry(text).await?;
                        results.push(response.embedding);
                    }
                }
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

    async fn health_check(&self) -> anyhow::Result<bool> {
        match self.embed_single("health check").await {
            Ok(response) => Ok(response.dimensions == self.expected_dimensions),
            Err(e) => {
                tracing::warn!("Ollama health check failed: {}", e);
                Ok(false)
            }
        }
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        let url = format!("{}/api/tags", self.endpoint);

        let response = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(self.timeout_secs))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list models: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to list models: {}",
                response.status()
            ));
        }

        let tags_response: OllamaTagsResponse = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse model list: {}", e))?;

        Ok(tags_response.models.into_iter().map(|m| m.name).collect())
    }
}

// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> EmbeddingConfig {
        EmbeddingConfig::ollama(
            Some("https://llama.krohnos.io".to_string()),
            Some("nomic-embed-text-v1.5-q8_0".to_string()),
        )
    }

    #[test]
    fn test_provider_creation() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "Ollama");
        assert_eq!(provider.model_name(), "nomic-embed-text-v1.5-q8_0");
        assert_eq!(provider.dimensions(), 768);
    }

    #[test]
    fn test_provider_creation_with_invalid_config() {
        // Create config with empty model name (invalid)
        let config = EmbeddingConfig::ollama(
            Some("https://llama.krohnos.io".to_string()),
            Some(String::new()), // Invalid empty model name
        );

        let provider = OllamaProvider::new(config);
        assert!(provider.is_err());
    }

    #[tokio::test]
    async fn test_embed_empty_text() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config).unwrap();

        let result = EmbeddingProvider::embed(&provider, "").await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Cannot embed empty text"),
            "Expected empty text error, got: {}",
            err
        );
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

    #[tokio::test]
    async fn test_list_models_response_deserialization() {
        use crate::embeddings::provider::{ModelFamily, ModelInfo, ParameterSize};

        // Test Ollama /api/tags response parsing
        let json = r#"{
            "models": [
                {
                    "name": "nomic-embed-text:latest",
                    "model": "nomic-embed-text:latest",
                    "modified_at": "2023-11-04T14:56:49.277302595-07:00",
                    "size": 274301056,
                    "digest": "sha256:c1f958f8c3e8",
                    "details": {
                        "parent_model": "",
                        "format": "gguf",
                        "family": "bert",
                        "families": ["bert"],
                        "parameter_size": "137M",
                        "quantization_level": "Q4_0"
                    }
                },
                {
                    "name": "mxbai-embed-large:latest",
                    "model": "mxbai-embed-large:latest",
                    "modified_at": "2024-01-15T10:30:00Z",
                    "size": 669000000,
                    "digest": "sha256:abc123def456",
                    "details": {
                        "format": "gguf",
                        "family": "bert",
                        "parameter_size": "334M",
                        "quantization_level": "Q8_0"
                    }
                }
            ]
        }"#;

        // This test verifies we can parse the Ollama API response
        // The actual deserialization will be implemented in OllamaTagsResponse
        let response: serde_json::Value = serde_json::from_str(json).unwrap();
        assert!(response["models"].is_array());
        assert_eq!(response["models"].as_array().unwrap().len(), 2);

        // Verify we can build ModelInfo from this data
        let first_model = &response["models"][0];
        let model_info = ModelInfo::builder()
            .name(first_model["name"].as_str().unwrap())
            .size_bytes(first_model["size"].as_u64().unwrap())
            .digest(first_model["digest"].as_str().unwrap())
            .family(ModelFamily::from_str(
                first_model["details"]["family"].as_str().unwrap(),
            ))
            .parameter_size(
                ParameterSize::from_str(first_model["details"]["parameter_size"].as_str().unwrap())
                    .unwrap(),
            )
            .quantization(
                first_model["details"]["quantization_level"]
                    .as_str()
                    .unwrap(),
            )
            .format(first_model["details"]["format"].as_str().unwrap())
            .build();

        assert_eq!(model_info.name, "nomic-embed-text:latest");
        assert_eq!(model_info.size_bytes, Some(274301056));
        assert_eq!(model_info.family, Some(ModelFamily::Bert));
        assert!(model_info.parameter_size.is_some());
        assert_eq!(model_info.parameter_size.unwrap().to_string(), "137M");
        assert_eq!(model_info.quantization, Some("Q4_0".to_string()));
    }

    #[test]
    fn test_parameter_size_parsing() {
        use crate::embeddings::provider::ParameterSize;

        let size_m = ParameterSize::from_str("137M").unwrap();
        assert_eq!(size_m.to_string(), "137M");
        assert_eq!(size_m.approximate_count(), 137_000_000);

        let size_b = ParameterSize::from_str("7B").unwrap();
        assert_eq!(size_b.to_string(), "7B");
        assert_eq!(size_b.approximate_count(), 7_000_000_000);

        let size_decimal = ParameterSize::from_str("1.5B").unwrap();
        assert_eq!(size_decimal.to_string(), "2B"); // Rounds to 2
        assert_eq!(size_decimal.approximate_count(), 2_000_000_000);
    }

    #[test]
    fn test_model_family_parsing() {
        use crate::embeddings::provider::ModelFamily;

        assert_eq!(ModelFamily::from_str("bert"), ModelFamily::Bert);
        assert_eq!(ModelFamily::from_str("BERT"), ModelFamily::Bert);
        assert_eq!(ModelFamily::from_str("gpt"), ModelFamily::Gpt);
        assert!(matches!(
            ModelFamily::from_str("custom"),
            ModelFamily::Other(_)
        ));

        assert_eq!(ModelFamily::Bert.as_str(), "bert");
    }

    #[test]
    fn test_model_info_builder() {
        use crate::embeddings::provider::{ModelFamily, ModelInfo};

        let model = ModelInfo::builder()
            .name("test-model")
            .dimensions(768)
            .family(ModelFamily::Bert)
            .recommended(true)
            .build();

        assert_eq!(model.name, "test-model");
        assert_eq!(model.dimensions, Some(768));
        assert_eq!(model.family, Some(ModelFamily::Bert));
        assert!(model.recommended);
        assert_eq!(model.display_name(), "test-model");
    }

    #[test]
    fn test_model_info_compatibility() {
        use crate::embeddings::provider::ModelInfo;

        let model = ModelInfo::builder().name("test").dimensions(768).build();

        assert!(model.is_compatible_dimensions(768));
        assert!(!model.is_compatible_dimensions(1536));

        // Model without dimensions is compatible with any requirement
        let model_no_dims = ModelInfo::new("test2");
        assert!(model_no_dims.is_compatible_dimensions(768));
        assert!(model_no_dims.is_compatible_dimensions(1536));
    }

    #[test]
    fn test_batch_request_serialization() {
        let request = OllamaBatchEmbeddingRequest {
            model: "nomic-embed-text".to_string(),
            input: vec![
                "first text".to_string(),
                "second text".to_string(),
                "third text".to_string(),
            ],
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("nomic-embed-text"));
        assert!(json.contains("first text"));
        assert!(json.contains("second text"));
        assert!(json.contains("third text"));
        assert!(json.contains("\"input\""));
    }

    #[test]
    fn test_batch_response_deserialization() {
        let json = r#"{"embeddings": [[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]}"#;
        let response: OllamaBatchEmbeddingResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.embeddings.len(), 2);
        assert_eq!(response.embeddings[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(response.embeddings[1], vec![0.4, 0.5, 0.6]);
    }

    #[tokio::test]
    async fn test_embed_batch_empty() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config).unwrap();

        let empty: &[&str] = &[];
        let result = EmbeddingProvider::embed_batch(&provider, empty).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_embed_batch_filters_empty_strings() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config).unwrap();

        // All empty strings should result in empty vec
        let result = EmbeddingProvider::embed_batch(&provider, &["", ""]).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_provider_batch_size_from_config() {
        let config = create_test_config();
        let provider = OllamaProvider::new(config).unwrap();

        // Default batch size should be 50
        assert_eq!(provider.batch_size, 50);
    }

    // =========================================================================
}
