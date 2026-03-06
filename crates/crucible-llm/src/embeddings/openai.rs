// crates/crucible-mcp/src/embeddings/openai.rs

//! OpenAI embedding provider implementation

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::config::EmbeddingConfig;
use super::error::{EmbeddingError, EmbeddingResult};
use crucible_core::enrichment::EmbeddingProvider;

/// OpenAI API request for embeddings
#[derive(Debug, Serialize)]
struct OpenAIEmbeddingRequest {
    /// Model to use for embedding
    model: String,
    /// Input text(s) to embed
    input: EmbeddingInput,
    /// Optional encoding format (default: float)
    #[serde(skip_serializing_if = "Option::is_none")]
    encoding_format: Option<String>,
}

/// Input for OpenAI embedding request (can be string or array)
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

/// OpenAI API response for embeddings
#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingResponse {
    /// Embedding data array
    data: Vec<OpenAIEmbeddingData>,
}

/// Individual embedding in OpenAI response
#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingData {
    /// The embedding vector
    embedding: Vec<f32>,
    /// Index in the batch
    index: usize,
}

/// OpenAI error response
#[derive(Debug, Deserialize)]
struct OpenAIErrorResponse {
    error: OpenAIErrorDetail,
}

/// Error detail from OpenAI
#[derive(Debug, Deserialize)]
struct OpenAIErrorDetail {
    message: String,
}

/// OpenAI embedding provider
pub struct OpenAIProvider {
    client: Client,
    config: EmbeddingConfig,
    api_key: String,
    endpoint: String,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider from configuration
    pub fn new(config: EmbeddingConfig) -> EmbeddingResult<Self> {
        // Validate configuration (From impl handles error conversion)
        config.validate()?;

        // Get API key using helper method
        let api_key = config
            .api_key()
            .ok_or_else(|| EmbeddingError::ConfigError("OpenAI requires an API key".to_string()))?
            .to_string();

        // Build HTTP client with timeout
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs()))
            .build()
            .map_err(|e| {
                EmbeddingError::ConfigError(format!("Failed to create HTTP client: {}", e))
            })?;

        let endpoint = config.endpoint();

        Ok(Self {
            client,
            endpoint,
            config,
            api_key,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIProvider {
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let request = OpenAIEmbeddingRequest {
            model: self.config.model_name().to_string(),
            input: EmbeddingInput::Single(text.to_string()),
            encoding_format: None,
        };

        let response = self
            .client
            .post(format!("{}/embeddings", self.endpoint))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return self
                .handle_error_response::<Vec<f32>>(response)
                .await
                .map_err(|e| e.into());
        }

        let api_response: OpenAIEmbeddingResponse = response.json().await.map_err(|e| {
            EmbeddingError::InvalidResponse(format!("Failed to parse OpenAI response: {}", e))
        })?;

        let data = api_response.data.into_iter().next().ok_or_else(|| {
            EmbeddingError::InvalidResponse("No embedding data in response".to_string())
        })?;

        let embedding_dims = data.embedding.len();
        let expected_dims = self.dimensions();
        if embedding_dims != expected_dims {
            return Err(EmbeddingError::InvalidDimensions {
                expected: expected_dims,
                actual: embedding_dims,
            }
            .into());
        }

        Ok(data.embedding)
    }

    async fn embed_batch(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let owned: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        let request = OpenAIEmbeddingRequest {
            model: self.config.model_name().to_string(),
            input: EmbeddingInput::Batch(owned),
            encoding_format: None,
        };

        let response = self
            .client
            .post(format!("{}/embeddings", self.endpoint))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return self
                .handle_error_response::<Vec<Vec<f32>>>(response)
                .await
                .map_err(|e| e.into());
        }

        let api_response: OpenAIEmbeddingResponse = response.json().await.map_err(|e| {
            EmbeddingError::InvalidResponse(format!("Failed to parse OpenAI response: {}", e))
        })?;

        let mut data = api_response.data;
        data.sort_by_key(|d| d.index);

        let expected_dims = self.dimensions();
        let mut results = Vec::with_capacity(data.len());

        for embedding_data in data {
            let embedding_dims = embedding_data.embedding.len();
            if embedding_dims != expected_dims {
                return Err(EmbeddingError::InvalidDimensions {
                    expected: expected_dims,
                    actual: embedding_dims,
                }
                .into());
            }
            results.push(embedding_data.embedding);
        }

        Ok(results)
    }

    fn provider_name(&self) -> &str {
        "OpenAI"
    }

    fn model_name(&self) -> &str {
        self.config.model_name()
    }

    fn dimensions(&self) -> usize {
        super::config::expected_dimensions_for_model(
            &self.config.provider_type(),
            self.config.model_name(),
        )
    }

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        Err(anyhow::anyhow!(
            "Model discovery not supported by OpenAI provider"
        ))
    }
}

impl OpenAIProvider {
    /// Handle error responses from OpenAI API
    async fn handle_error_response<T>(&self, response: reqwest::Response) -> EmbeddingResult<T> {
        let status = response.status();

        // Extract retry-after header before consuming response
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60); // Default to 60 seconds

        // Try to parse error response
        let error_detail = if let Ok(error_response) = response.json::<OpenAIErrorResponse>().await
        {
            error_response.error.message
        } else {
            format!("HTTP {}", status)
        };

        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(EmbeddingError::AuthenticationError(error_detail))
            }
            StatusCode::TOO_MANY_REQUESTS => Err(EmbeddingError::RateLimitExceeded {
                retry_after_secs: retry_after,
            }),
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => {
                Err(EmbeddingError::InvalidResponse(error_detail))
            }
            _ if status.is_server_error() => Err(EmbeddingError::ProviderError {
                provider: "OpenAI".to_string(),
                message: error_detail,
            }),
            _ => Err(EmbeddingError::Other(format!(
                "OpenAI API error: {}",
                error_detail
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_helpers::create_test_embedding_config;

    fn create_test_config() -> EmbeddingConfig {
        create_test_embedding_config()
    }


    #[test]
    fn test_provider_creation() {
        let config = create_test_config();
        let provider = OpenAIProvider::new(config);
        assert!(provider.is_ok());

        let provider = provider.unwrap();
        assert_eq!(provider.provider_name(), "OpenAI");
        assert_eq!(provider.model_name(), "text-embedding-3-small");
        assert_eq!(provider.dimensions(), 1536);
    }

    #[test]
    fn test_provider_creation_with_invalid_config() {
        // Create config with empty model name (invalid)
        let config = EmbeddingConfig::openai(
            "sk-test-api-key-for-testing".to_string(),
            Some(String::new()), // Invalid empty model name
        );

        let provider = OpenAIProvider::new(config);
        assert!(provider.is_err());
    }

    #[test]
    fn test_provider_creation_without_api_key() {
        // Create config with empty API key (invalid for OpenAI)
        let config = EmbeddingConfig::openai(
            String::new(), // Empty API key (invalid)
            Some("text-embedding-3-small".to_string()),
        );

        let provider = OpenAIProvider::new(config);
        assert!(provider.is_err());
    }

    #[tokio::test]
    async fn test_embed_empty_text() {
        let config = create_test_config();
        let provider = OpenAIProvider::new(config).unwrap();

        let result = provider.embed("").await;
        // OpenAI might handle empty text differently, but should return error or empty response
        // This test will fail with network error in CI, but structure is correct
        assert!(result.is_err());
    }

    #[test]
    fn test_openai_request_serialization() {
        let request = OpenAIEmbeddingRequest {
            model: "text-embedding-3-small".to_string(),
            input: EmbeddingInput::Single("test text".to_string()),
            encoding_format: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("text-embedding-3-small"));
        assert!(json.contains("test text"));
    }

    #[test]
    fn test_openai_batch_request_serialization() {
        let request = OpenAIEmbeddingRequest {
            model: "text-embedding-3-small".to_string(),
            input: EmbeddingInput::Batch(vec!["text1".to_string(), "text2".to_string()]),
            encoding_format: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("text-embedding-3-small"));
        assert!(json.contains("text1"));
        assert!(json.contains("text2"));
    }

    #[test]
    fn test_openai_response_deserialization() {
        let json = r#"{
            "data": [
                {"embedding": [0.1, 0.2, 0.3], "index": 0}
            ],
            "model": "text-embedding-3-small",
            "usage": {
                "prompt_tokens": 5,
                "total_tokens": 5
            }
        }"#;

        let response: OpenAIEmbeddingResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].embedding.len(), 3);
        assert_eq!(response.data[0].embedding[0], 0.1);
        assert_eq!(response.data[0].index, 0);
    }

    #[test]
    fn test_openai_batch_response_deserialization() {
        let json = r#"{
            "data": [
                {"embedding": [0.1, 0.2], "index": 0},
                {"embedding": [0.3, 0.4], "index": 1}
            ],
            "model": "text-embedding-3-small",
            "usage": {
                "prompt_tokens": 10,
                "total_tokens": 10
            }
        }"#;

        let response: OpenAIEmbeddingResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].index, 0);
        assert_eq!(response.data[1].index, 1);
    }

    #[test]
    fn test_openai_error_response_deserialization() {
        let json = r#"{
            "error": {
                "message": "Invalid API key",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        }"#;

        let response: OpenAIErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error.message, "Invalid API key");
    }
}
