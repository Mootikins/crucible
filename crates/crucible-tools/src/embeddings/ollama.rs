//! Ollama embedding provider implementation
//!
//! This module provides an implementation of the EmbeddingProvider trait
//! for Ollama, which allows running embedding models locally.

use crate::embeddings::{EmbeddingConfig, EmbeddingProvider, EmbeddingResponse};
use crucible_llm::embeddings::{EmbeddingError, EmbeddingResult};
use crucible_llm::embeddings::provider::{ModelInfo, ModelFamily};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info};

/// Ollama embedding provider
pub struct OllamaProvider {
    client: Client,
    config: EmbeddingConfig,
}

impl OllamaProvider {
    /// Create a new Ollama provider
    pub fn new(config: EmbeddingConfig) -> Result<Self, EmbeddingError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| EmbeddingError::Configuration(format!("Failed to create HTTP client: {}", e)))?;

        info!("Created Ollama provider for model: {}", config.model);

        Ok(Self { client, config })
    }

    /// Check if the Ollama server is accessible
    pub async fn health_check(&self) -> Result<(), EmbeddingError> {
        let url = format!("{}/api/tags", self.config.endpoint);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| EmbeddingError::Network(format!("Failed to connect to Ollama: {}", e)))?;

        if response.status().is_success() {
            debug!("Ollama server is accessible at {}", self.config.endpoint);
            Ok(())
        } else {
            Err(EmbeddingError::Network(format!(
                "Ollama server returned status: {}",
                response.status()
            )))
        }
    }

    /// Get available models from Ollama
    pub async fn list_models(&self) -> Result<Vec<OllamaModel>, EmbeddingError> {
        let url = format!("{}/api/tags", self.config.endpoint);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| EmbeddingError::Network(format!("Failed to fetch models: {}", e)))?;

        if response.status().is_success() {
            let models_response: OllamaModelsResponse = response
                .json()
                .await
                .map_err(EmbeddingError::SerializationError)?;

            debug!("Found {} models in Ollama", models_response.models.len());
            Ok(models_response.models)
        } else {
            Err(EmbeddingError::Network(format!(
                "Failed to fetch models, status: {}",
                response.status()
            )))
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        let url = format!("{}/api/embeddings", self.config.endpoint);

        let request = OllamaEmbeddingRequest {
            model: self.config.model.clone(),
            prompt: text.to_string(),
        };

        debug!("Generating embedding for text of length: {}", text.len());

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(EmbeddingError::HttpError)?;

        if response.status().is_success() {
            let embedding_response: OllamaEmbeddingResponse = response
                .json()
                .await
                .map_err(EmbeddingError::SerializationError)?;

            debug!("Generated embedding with {} dimensions", embedding_response.embedding.len());

            let embedding = EmbeddingResponse::new(
                embedding_response.embedding,
                self.config.model.clone()
            );

            Ok(embedding)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            error!("Ollama embedding error: {}", error_text);
            Err(EmbeddingError::ProviderError {
                provider: "Ollama".to_string(),
                message: error_text,
            })
        }
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        debug!("Generating batch embeddings for {} texts", texts.len());

        // For Ollama, we'll generate embeddings one by one since it doesn't have a built-in batch API
        let mut embeddings = Vec::with_capacity(texts.len());

        for (i, text) in texts.iter().enumerate() {
            debug!("Processing text {}/{}", i + 1, texts.len());
            let embedding = self.embed(text).await?;
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn dimensions(&self) -> usize {
        // Ollama embeddings typically have 768 dimensions for nomic-embed-text
        // This could be made configurable in the future
        768
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<String>> {
        let url = format!("{}/api/tags", self.config.endpoint);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(EmbeddingError::HttpError)?;

        if response.status().is_success() {
            let models_response: OllamaModelsResponse = response
                .json()
                .await
                .map_err(EmbeddingError::SerializationError)?;

            let mut model_names = Vec::new();
            for model in models_response.models {
                model_names.push(model.name);
            }

            Ok(model_names)
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(EmbeddingError::ProviderError {
                provider: "Ollama".to_string(),
                message: format!("Failed to list models: {}", error_text),
            })
        }
    }
}

/// Ollama embedding request
#[derive(Debug, Serialize)]
struct OllamaEmbeddingRequest {
    model: String,
    prompt: String,
}

/// Ollama embedding response
#[derive(Debug, Deserialize)]
struct OllamaEmbeddingResponse {
    embedding: Vec<f32>,
}

/// Ollama model information
#[derive(Debug, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    pub size: Option<u64>,
    pub digest: Option<String>,
    pub modified_at: Option<String>,
    pub details: Option<OllamaModelDetails>,
}

/// Ollama model details
#[derive(Debug, Deserialize)]
pub struct OllamaModelDetails {
    pub format: Option<String>,
    pub family: Option<String>,
    pub families: Option<Vec<String>>,
    pub parameter_size: Option<String>,
    pub quantization_level: Option<String>,
}

/// Ollama models response
#[derive(Debug, Deserialize)]
struct OllamaModelsResponse {
    models: Vec<OllamaModel>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embeddings::{ProviderType, default_models};

    #[tokio::test]
    async fn test_ollama_provider_creation() {
        let config = default_models::ollama_default();
        let provider = OllamaProvider::new(config);
        assert!(provider.is_ok());
    }

    #[tokio::test]
    async fn test_ollama_provider_requires_valid_endpoint() {
        let config = EmbeddingConfig {
            provider: ProviderType::Ollama,
            endpoint: "http://invalid:12345".to_string(),
            api_key: None,
            model: "test-model".to_string(),
            timeout_secs: 1,
            max_retries: 0,
            batch_size: 1,
        };

        let provider = OllamaProvider::new(config).unwrap();
        let result = provider.health_check().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_ollama_embedding_request_serialization() {
        let request = OllamaEmbeddingRequest {
            model: "nomic-embed-text".to_string(),
            prompt: "test text".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("nomic-embed-text"));
        assert!(json.contains("test text"));
    }

    #[test]
    fn test_ollama_embedding_response_deserialization() {
        let json = r#"
        {
            "embedding": [0.1, 0.2, 0.3, 0.4]
        }
        "#;

        let response: OllamaEmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.embedding, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn test_ollama_models_response_deserialization() {
        let json = r#"
        {
            "models": [
                {
                    "name": "nomic-embed-text:latest",
                    "size": 134217728,
                    "digest": "sha256:abc123",
                    "modified_at": "2024-01-01T00:00:00Z",
                    "details": {
                        "format": "gguf",
                        "family": "nomic-embed",
                        "parameter_size": "137M",
                        "quantization_level": "Q4_0"
                    }
                }
            ]
        }
        "#;

        let response: OllamaModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.models.len(), 1);
        assert_eq!(response.models[0].name, "nomic-embed-text:latest");
        assert_eq!(response.models[0].size, Some(134217728));
    }
}