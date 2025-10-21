//! OpenAI embedding provider implementation
//!
//! This module provides an implementation of the EmbeddingProvider trait
//! for OpenAI's embedding API.

use crate::embeddings::{EmbeddingConfig, EmbeddingProvider, EmbeddingResponse};
use crucible_llm::embeddings::{EmbeddingError, EmbeddingResult};
use crucible_llm::embeddings::provider::{ModelInfo, ModelFamily};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info};

/// OpenAI embedding provider
pub struct OpenAIProvider {
    client: Client,
    config: EmbeddingConfig,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider
    pub fn new(config: EmbeddingConfig) -> Result<Self, EmbeddingError> {
        if config.api_key.is_none() {
            return Err(EmbeddingError::Configuration(
                "OpenAI API key is required".to_string(),
            ));
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {}", config.api_key.as_ref().unwrap())
                        .parse()
                        .unwrap(),
                );
                headers.insert(
                    reqwest::header::CONTENT_TYPE,
                    "application/json".parse().unwrap(),
                );
                headers
            })
            .build()
            .map_err(|e| EmbeddingError::Configuration(format!("Failed to create HTTP client: {}", e)))?;

        info!("Created OpenAI provider for model: {}", config.model);

        Ok(Self { client, config })
    }

    /// Check if the OpenAI API is accessible
    pub async fn health_check(&self) -> Result<(), EmbeddingError> {
        let url = format!("{}/models", self.config.endpoint);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| EmbeddingError::Network(format!("Failed to connect to OpenAI: {}", e)))?;

        if response.status().is_success() {
            debug!("OpenAI API is accessible at {}", self.config.endpoint);
            Ok(())
        } else {
            Err(EmbeddingError::Network(format!(
                "OpenAI API returned status: {}",
                response.status()
            )))
        }
    }

    /// Get available models from OpenAI
    pub async fn list_models(&self) -> Result<Vec<OpenAIModel>, EmbeddingError> {
        let url = format!("{}/models", self.config.endpoint);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| EmbeddingError::Network(format!("Failed to fetch models: {}", e)))?;

        if response.status().is_success() {
            let models_response: OpenAIModelsResponse = response
                .json()
                .await
                .map_err(|e| EmbeddingError::Parsing(format!("Failed to parse models response: {}", e)))?;

            debug!("Found {} models in OpenAI", models_response.data.len());
            Ok(models_response.data)
        } else {
            Err(EmbeddingError::Network(format!(
                "Failed to fetch models, status: {}",
                response.status()
            )))
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        let url = format!("{}/embeddings", self.config.endpoint);

        let request = OpenAIEmbeddingRequest {
            model: self.config.model.clone(),
            input: text.to_string(),
            encoding_format: "float".to_string(),
        };

        debug!("Generating embedding for text of length: {}", text.len());

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| EmbeddingError::NetworkError(e))?;

        if response.status().is_success() {
            let embedding_response: OpenAIEmbeddingResponse = response
                .json()
                .await
                .map_err(EmbeddingError::SerializationError)?;

            debug!("Generated embedding with {} dimensions", embedding_response.data.embedding.len());

            let embedding = EmbeddingResponse::new(
                embedding_response.data.embedding,
                embedding_response.model
            ).with_tokens(embedding_response.usage.prompt_tokens as usize);

            Ok(embedding)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            error!("OpenAI embedding error: {}", error_text);
            Err(EmbeddingError::ApiError {
                message: error_text,
                status: response.status().as_u16(),
            })
        }
    }

    async fn embed_batch(&self, texts: &[String]) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        debug!("Generating batch embeddings for {} texts", texts.len());

        // OpenAI supports batch embedding requests
        let url = format!("{}/embeddings", self.config.endpoint);

        let request = OpenAIBatchEmbeddingRequest {
            model: self.config.model.clone(),
            input: texts.clone(),
            encoding_format: "float".to_string(),
        };

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| EmbeddingError::NetworkError(e))?;

        if response.status().is_success() {
            let batch_response: OpenAIBatchEmbeddingResponse = response
                .json()
                .await
                .map_err(EmbeddingError::SerializationError)?;

            debug!("Generated {} embeddings", batch_response.data.len());

            // Sort by index to maintain order
            let mut sorted_data = batch_response.data;
            sorted_data.sort_by_key(|item| item.index);

            let embeddings: Vec<EmbeddingResponse> = sorted_data
                .into_iter()
                .map(|item| EmbeddingResponse::new(item.embedding, batch_response.model.clone())
                    .with_tokens(batch_response.usage.prompt_tokens as usize))
                .collect();

            Ok(embeddings)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            error!("OpenAI batch embedding error: {}", error_text);
            Err(EmbeddingError::ApiError {
                message: error_text,
                status: response.status().as_u16(),
            })
        }
    }

    fn dimensions(&self) -> usize {
        // OpenAI text-embedding-3-small: 1536 dimensions
        // OpenAI text-embedding-ada-002: 1536 dimensions
        // OpenAI text-embedding-3-large: 3072 dimensions
        // This could be made configurable based on the model name
        if self.config.model.contains("3-large") {
            3072
        } else {
            1536
        }
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<String>> {
        // For OpenAI, we'll return hardcoded known embedding models
        Ok(vec![
            "text-embedding-3-small".to_string(),
            "text-embedding-3-large".to_string(),
            "text-embedding-ada-002".to_string(),
        ])
    }

    fn provider_info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "OpenAI".to_string(),
            version: "1.0.0".to_string(),
            model: self.config.model.clone(),
            available_models: vec![
                "text-embedding-3-small".to_string(),
                "text-embedding-3-large".to_string(),
                "text-embedding-ada-002".to_string(),
            ],
            max_batch_size: 2048,
            max_tokens: 8192,
            dimensions: self.dimensions(),
            capabilities: ProviderCapabilities {
                supports_batch: true,
                supports_streaming: false,
                supports_custom_models: false,
                accurate_token_count: true,
                supports_dimension_control: false,
                supports_async: false,
            },
            metadata: HashMap::new(),
        }
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        // Simple health check - try to embed a short text
        match self.embed("health check").await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn model_available(&self, model: &str) -> EmbeddingResult<bool> {
        Ok(["text-embedding-3-small", "text-embedding-3-large", "text-embedding-ada-002"]
            .contains(&model))
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        // Simple heuristic: roughly 4 characters per token for English
        (text.len() + 3) / 4
    }

    fn max_tokens(&self) -> usize {
        8192
    }

    fn model(&self) -> &str {
        &self.config.model
    }
}

/// OpenAI embedding request
#[derive(Debug, Serialize)]
struct OpenAIEmbeddingRequest {
    model: String,
    input: String,
    encoding_format: String,
}

/// OpenAI batch embedding request
#[derive(Debug, Serialize)]
struct OpenAIBatchEmbeddingRequest {
    model: String,
    input: Vec<String>,
    encoding_format: String,
}

/// OpenAI embedding data
#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingData {
    object: String,
    embedding: Vec<f32>,
    index: i32,
}

/// OpenAI embedding response
#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingResponse {
    object: String,
    data: OpenAIEmbeddingData,
    model: String,
    usage: OpenAIUsage,
}

/// OpenAI batch embedding response
#[derive(Debug, Deserialize)]
struct OpenAIBatchEmbeddingResponse {
    object: String,
    data: Vec<OpenAIEmbeddingData>,
    model: String,
    usage: OpenAIUsage,
}

/// OpenAI usage information
#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: i32,
    total_tokens: i32,
}

/// OpenAI model information
#[derive(Debug, Deserialize)]
pub struct OpenAIModel {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
}

/// OpenAI models response
#[derive(Debug, Deserialize)]
struct OpenAIModelsResponse {
    object: String,
    data: Vec<OpenAIModel>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::{ProviderType, default_models};

    #[test]
    fn test_openai_provider_creation_requires_api_key() {
        let config = EmbeddingConfig {
            provider: ProviderType::OpenAI,
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: None,
            model: "text-embedding-ada-002".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 10,
        };

        let provider = OpenAIProvider::new(config);
        assert!(provider.is_err());
    }

    #[test]
    fn test_openai_provider_creation_with_api_key() {
        let config = EmbeddingConfig {
            provider: ProviderType::OpenAI,
            endpoint: "https://api.openai.com/v1".to_string(),
            api_key: Some("test-key".to_string()),
            model: "text-embedding-ada-002".to_string(),
            timeout_secs: 30,
            max_retries: 3,
            batch_size: 10,
        };

        let provider = OpenAIProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn test_openai_embedding_request_serialization() {
        let request = OpenAIEmbeddingRequest {
            model: "text-embedding-ada-002".to_string(),
            input: "test text".to_string(),
            encoding_format: "float".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("text-embedding-ada-002"));
        assert!(json.contains("test text"));
        assert!(json.contains("float"));
    }

    #[test]
    fn test_openai_embedding_response_deserialization() {
        let json = r#"
        {
            "object": "list",
            "data": {
                "object": "embedding",
                "embedding": [0.1, 0.2, 0.3, 0.4],
                "index": 0
            },
            "model": "text-embedding-ada-002",
            "usage": {
                "prompt_tokens": 4,
                "total_tokens": 4
            }
        }
        "#;

        let response: OpenAIEmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.embedding, vec![0.1, 0.2, 0.3, 0.4]);
        assert_eq!(response.data.index, 0);
        assert_eq!(response.usage.prompt_tokens, 4);
    }

    #[test]
    fn test_openai_batch_embedding_response_deserialization() {
        let json = r#"
        {
            "object": "list",
            "data": [
                {
                    "object": "embedding",
                    "embedding": [0.1, 0.2, 0.3, 0.4],
                    "index": 0
                },
                {
                    "object": "embedding",
                    "embedding": [0.5, 0.6, 0.7, 0.8],
                    "index": 1
                }
            ],
            "model": "text-embedding-ada-002",
            "usage": {
                "prompt_tokens": 8,
                "total_tokens": 8
            }
        }
        "#;

        let response: OpenAIBatchEmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 2);
        assert_eq!(response.data[0].embedding, vec![0.1, 0.2, 0.3, 0.4]);
        assert_eq!(response.data[1].embedding, vec![0.5, 0.6, 0.7, 0.8]);
    }

    #[test]
    fn test_openai_models_response_deserialization() {
        let json = r#"
        {
            "object": "list",
            "data": [
                {
                    "id": "text-embedding-ada-002",
                    "object": "model",
                    "created": 1671217299,
                    "owned_by": "openai"
                }
            ]
        }
        "#;

        let response: OpenAIModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].id, "text-embedding-ada-002");
        assert_eq!(response.data[0].owned_by, "openai");
    }
}