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

        BackendType::Anthropic => {
            anthropic::list_models(endpoint, api_key.unwrap_or("")).await
        }

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
