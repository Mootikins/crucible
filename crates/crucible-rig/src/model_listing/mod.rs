//! Model listing dispatch and provider wrappers
//!
//! This module provides a unified interface for discovering available models
//! across different LLM providers. Each provider has a thin wrapper that handles
//! the provider-specific API details.

pub mod anthropic;
pub mod ollama;
pub mod openai_compat;

use crucible_config::BackendType;
use thiserror::Error;

/// Errors from model listing operations
#[derive(Debug, Error)]
pub enum ModelListingError {
    /// HTTP request failed
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON parsing failed
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    /// API returned an error
    #[error("API error: {0}")]
    Api(String),
}

/// Result type for model listing operations
pub type ModelListingResult<T> = Result<T, ModelListingError>;

/// List available models for a given backend
///
/// Routes to the appropriate provider wrapper based on `backend_type`.
/// Returns a list of model identifiers (e.g., "llama3.2:latest", "gpt-4", etc.).
///
/// # Arguments
///
/// * `backend_type` - The LLM provider backend
/// * `endpoint` - The API endpoint URL
/// * `api_key` - Optional API key (required for some providers)
///
/// # Returns
///
/// A list of available model identifiers, or an error if discovery fails.
/// For providers that don't support model discovery, returns an empty list.
pub async fn list_models(
    backend_type: BackendType,
    endpoint: &str,
    api_key: Option<&str>,
) -> ModelListingResult<Vec<String>> {
    match backend_type {
        BackendType::Ollama => ollama::list_models(endpoint).await,

        BackendType::OpenAI | BackendType::ZAI | BackendType::OpenRouter => {
            openai_compat::list_models(endpoint, api_key.unwrap_or("")).await
        }

        BackendType::Anthropic => anthropic::list_models(endpoint, api_key.unwrap_or("")).await,

        BackendType::GitHubCopilot => {
            if let Some(token) = api_key {
                match github_copilot_list_models(token).await {
                    Ok(models) => Ok(models),
                    Err(_) => Ok(vec![]),
                }
            } else {
                Ok(vec![])
            }
        }

        // Providers that don't support model discovery
        BackendType::Cohere
        | BackendType::VertexAI
        | BackendType::FastEmbed
        | BackendType::Burn
        | BackendType::Custom
        | BackendType::Mock => Ok(vec![]),
    }
}

/// Helper to list GitHub Copilot models using an OAuth token
async fn github_copilot_list_models(token: &str) -> ModelListingResult<Vec<String>> {
    use crate::github_copilot::CopilotClient;

    let client = CopilotClient::new(token.to_string());
    match client.list_models().await {
        Ok(models) => Ok(models.into_iter().map(|m| m.id).collect()),
        Err(_) => Ok(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_config::BackendType;

    #[tokio::test]
    async fn test_dispatch_cohere_returns_empty() {
        let result = list_models(BackendType::Cohere, "http://example.com", None).await;
        assert_eq!(result.unwrap(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_dispatch_vertexai_returns_empty() {
        let result = list_models(BackendType::VertexAI, "http://example.com", None).await;
        assert_eq!(result.unwrap(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_dispatch_fastembed_returns_empty() {
        let result = list_models(BackendType::FastEmbed, "", None).await;
        assert_eq!(result.unwrap(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_dispatch_burn_returns_empty() {
        let result = list_models(BackendType::Burn, "", None).await;
        assert_eq!(result.unwrap(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_dispatch_custom_returns_empty() {
        let result = list_models(BackendType::Custom, "", None).await;
        assert_eq!(result.unwrap(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn test_dispatch_mock_returns_empty() {
        let result = list_models(BackendType::Mock, "", None).await;
        assert_eq!(result.unwrap(), Vec::<String>::new());
    }

    #[test]
    fn test_model_listing_error_api_variant() {
        let err = ModelListingError::Api("test error".to_string());
        assert!(err.to_string().contains("test error"));
    }

    #[test]
    fn test_model_listing_error_json_variant() {
        let json_err = serde_json::from_str::<serde_json::Value>("{invalid").unwrap_err();
        let err = ModelListingError::Json(json_err);
        assert!(err.to_string().contains("JSON") || err.to_string().contains("parse"));
    }

    // --- Dispatch routing tests (mockito HTTP server) ---

    #[tokio::test]
    async fn test_dispatch_ollama_routes_to_ollama_wrapper() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/api/tags")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"models": [{"name": "llama3.2:latest"}]}"#)
            .create_async()
            .await;

        let result = list_models(BackendType::Ollama, &server.url(), None).await;
        mock.assert_async().await;
        assert_eq!(result.unwrap(), vec!["llama3.2:latest"]);
    }

    #[tokio::test]
    async fn test_dispatch_openai_routes_to_openai_compat_wrapper() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [{"id": "gpt-4o"}]}"#)
            .create_async()
            .await;

        let result = list_models(BackendType::OpenAI, &server.url(), Some("test-key")).await;
        mock.assert_async().await;
        assert_eq!(result.unwrap(), vec!["gpt-4o"]);
    }

    #[tokio::test]
    async fn test_dispatch_zai_routes_to_openai_compat_wrapper() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [{"id": "glm-4-flash"}]}"#)
            .create_async()
            .await;

        let result = list_models(BackendType::ZAI, &server.url(), Some("test-key")).await;
        mock.assert_async().await;
        assert_eq!(result.unwrap(), vec!["glm-4-flash"]);
    }

    #[tokio::test]
    async fn test_dispatch_openrouter_routes_to_openai_compat_wrapper() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [{"id": "anthropic/claude-3-haiku"}]}"#)
            .create_async()
            .await;

        let result = list_models(BackendType::OpenRouter, &server.url(), Some("test-key")).await;
        mock.assert_async().await;
        assert_eq!(result.unwrap(), vec!["anthropic/claude-3-haiku"]);
    }

    #[tokio::test]
    async fn test_dispatch_anthropic_routes_to_anthropic_wrapper() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data": [{"id": "claude-sonnet-4-20250514"}]}"#)
            .create_async()
            .await;

        let result = list_models(BackendType::Anthropic, &server.url(), Some("test-key")).await;
        mock.assert_async().await;
        assert_eq!(result.unwrap(), vec!["claude-sonnet-4-20250514"]);
    }
}
