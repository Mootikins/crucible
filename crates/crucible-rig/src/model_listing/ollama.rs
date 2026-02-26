//! Ollama model listing wrapper
//!
//! Discovers available models from a local or remote Ollama instance
//! via the `/api/tags` endpoint.

use super::{ModelListingError, ModelListingResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Response from Ollama `/api/tags` endpoint
#[derive(Debug, Serialize, Deserialize)]
struct TagsResponse {
    /// List of available models
    models: Vec<ModelTag>,
}

/// Individual model tag from Ollama
#[derive(Debug, Serialize, Deserialize)]
struct ModelTag {
    /// Model name/identifier
    name: String,
}

/// List available models from Ollama
///
/// Queries the `/api/tags` endpoint with a 10-second timeout.
/// Extracts model names from the response.
///
/// # Arguments
///
/// * `endpoint` - Ollama API endpoint (e.g., "http://localhost:11434")
///
/// # Returns
///
/// A list of model names, or an error if the request fails.
pub async fn list_models(endpoint: &str) -> ModelListingResult<Vec<String>> {
    let endpoint = endpoint.trim_end_matches('/');
    let url = format!("{}/api/tags", endpoint);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        return Err(ModelListingError::Api(format!(
            "Ollama API error {}: {}",
            status, text
        )));
    }

    let body = response.text().await?;
    parse_tags_response(&body)
}

/// Parse Ollama tags response JSON
///
/// Extracts model names from the JSON response.
/// This is a separate function to enable unit testing without HTTP.
pub fn parse_tags_response(body: &str) -> ModelListingResult<Vec<String>> {
    let response: TagsResponse = serde_json::from_str(body)?;
    Ok(response.models.into_iter().map(|m| m.name).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tags_response_extracts_names() {
        let json = r#"{"models":[{"name":"llama3.2:latest"},{"name":"mistral:latest"}]}"#;
        let result = parse_tags_response(json).unwrap();
        assert_eq!(result, vec!["llama3.2:latest", "mistral:latest"]);
    }

    #[test]
    fn test_parse_tags_response_empty_models() {
        let json = r#"{"models":[]}"#;
        let result = parse_tags_response(json).unwrap();
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn test_parse_tags_response_malformed_returns_error() {
        let json = r#"{"invalid": "json"}"#;
        let result = parse_tags_response(json);
        assert!(result.is_err());
    }
}
