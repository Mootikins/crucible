// crates/crucible-mcp/src/embeddings/openai.rs

//! OpenAI embedding provider implementation

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::config::EmbeddingConfig;
use super::error::{EmbeddingError, EmbeddingResult};
use super::provider::{EmbeddingProvider, EmbeddingResponse};

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
    /// Model used
    model: String,
    /// Usage statistics
    usage: OpenAIUsage,
}

/// Individual embedding in OpenAI response
#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingData {
    /// The embedding vector
    embedding: Vec<f32>,
    /// Index in the batch
    index: usize,
}

/// Token usage information
#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    /// Number of prompt tokens
    prompt_tokens: usize,
    /// Total tokens used
    #[allow(dead_code)]
    total_tokens: usize,
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
    #[serde(rename = "type")]
    #[allow(dead_code)]
    error_type: String,
    #[serde(default)]
    #[allow(dead_code)]
    code: Option<String>,
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
        // Validate configuration
        config.validate()?;

        // Get API key
        let api_key = config.api_key.clone().ok_or_else(|| {
            EmbeddingError::ConfigError("OpenAI requires an API key".to_string())
        })?;

        // Build HTTP client with timeout
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| EmbeddingError::ConfigError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            endpoint: config.endpoint.clone(),
            config,
            api_key,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        // Build request
        let request = OpenAIEmbeddingRequest {
            model: self.config.model.clone(),
            input: EmbeddingInput::Single(text.to_string()),
            encoding_format: None,
        };

        // Send request
        let response = self
            .client
            .post(format!("{}/embeddings", self.endpoint))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        // Handle error status codes
        let status = response.status();
        if !status.is_success() {
            return self.handle_error_response(response).await;
        }

        // Parse successful response
        let api_response: OpenAIEmbeddingResponse = response.json().await.map_err(|e| {
            EmbeddingError::InvalidResponse(format!("Failed to parse OpenAI response: {}", e))
        })?;

        // Extract first embedding from data array
        let data = api_response.data.into_iter().next().ok_or_else(|| {
            EmbeddingError::InvalidResponse("No embedding data in response".to_string())
        })?;

        // Validate dimensions
        let embedding_dims = data.embedding.len();
        let expected_dims = self.dimensions();
        if embedding_dims != expected_dims {
            return Err(EmbeddingError::InvalidDimensions {
                expected: expected_dims,
                actual: embedding_dims,
            });
        }

        // Build response
        Ok(EmbeddingResponse::new(data.embedding, api_response.model)
            .with_tokens(api_response.usage.prompt_tokens))
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Build request
        let request = OpenAIEmbeddingRequest {
            model: self.config.model.clone(),
            input: EmbeddingInput::Batch(texts),
            encoding_format: None,
        };

        // Send request
        let response = self
            .client
            .post(format!("{}/embeddings", self.endpoint))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        // Handle error status codes
        let status = response.status();
        if !status.is_success() {
            return self.handle_error_response(response).await;
        }

        // Parse successful response
        let api_response: OpenAIEmbeddingResponse = response.json().await.map_err(|e| {
            EmbeddingError::InvalidResponse(format!("Failed to parse OpenAI response: {}", e))
        })?;

        // Sort by index to maintain input order
        let mut data = api_response.data;
        data.sort_by_key(|d| d.index);

        // Validate dimensions and build responses
        let expected_dims = self.dimensions();
        let mut results = Vec::with_capacity(data.len());

        for embedding_data in data {
            let embedding_dims = embedding_data.embedding.len();
            if embedding_dims != expected_dims {
                return Err(EmbeddingError::InvalidDimensions {
                    expected: expected_dims,
                    actual: embedding_dims,
                });
            }

            results.push(
                EmbeddingResponse::new(embedding_data.embedding, api_response.model.clone())
                    .with_tokens(api_response.usage.prompt_tokens / results.len().max(1)),
            );
        }

        Ok(results)
    }

    fn provider_name(&self) -> &str {
        "OpenAI"
    }

    fn model_name(&self) -> &str {
        &self.config.model
    }

    fn dimensions(&self) -> usize {
        self.config.expected_dimensions()
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
        let error_detail = if let Ok(error_response) = response.json::<OpenAIErrorResponse>().await {
            error_response.error.message
        } else {
            format!("HTTP {}", status)
        };

        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(EmbeddingError::AuthenticationError(error_detail))
            }
            StatusCode::TOO_MANY_REQUESTS => {
                Err(EmbeddingError::RateLimitExceeded {
                    retry_after_secs: retry_after,
                })
            }
            StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND => {
                Err(EmbeddingError::InvalidResponse(error_detail))
            }
            _ if status.is_server_error() => {
                Err(EmbeddingError::ProviderError {
                    provider: "OpenAI".to_string(),
                    message: error_detail,
                })
            }
            _ => Err(EmbeddingError::Other(format!(
                "OpenAI API error: {}",
                error_detail
            ))),
        }
    }
}
