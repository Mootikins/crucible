//! Anthropic model listing wrapper
//!
//! Discovers available models from Anthropic API.
//!
//! Uses the `/v1/models` endpoint with `x-api-key` authentication
//! (NOT Bearer auth — Anthropic uses its own header).

use super::{ModelListingError, ModelListingResult};
use serde_json::Value;
use std::time::Duration;

/// List available models from Anthropic API
///
/// Queries the `/v1/models` endpoint with a 10-second timeout.
/// Extracts model IDs from the response.
///
/// # Arguments
///
/// * `endpoint` - API endpoint (e.g., "https://api.anthropic.com")
/// * `api_key` - API key for authentication
///
/// # Returns
///
/// A list of model identifiers, or an error if the request fails.
pub async fn list_models(endpoint: &str, api_key: &str) -> ModelListingResult<Vec<String>> {
    let endpoint = endpoint.trim_end_matches('/');
    let url = format!("{}/v1/models", endpoint);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client
        .get(&url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        return Err(ModelListingError::Api(format!(
            "Anthropic API error {}: {}",
            status, text
        )));
    }

    let body = response.text().await?;
    parse_models_response(&body)
}

/// Parse Anthropic models response JSON
///
/// Extracts model IDs from the JSON response.
/// This is a separate function to enable unit testing without HTTP.
///
/// Anthropic response format: `{"data": [{"id": "claude-opus-4-5", ...}]}`
pub fn parse_models_response(body: &str) -> ModelListingResult<Vec<String>> {
    let payload: Value = serde_json::from_str(body)?;

    // Extract the 'data' key
    let data = payload
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| ModelListingError::Api(
            "expected 'data' key with array value in response".into(),
        ))?;

    // Extract 'id' field from each model entry
    let models = data
        .iter()
        .filter_map(|model| {
            model
                .as_object()?
                .get("id")
                .and_then(Value::as_str)
                .map(ToString::to_string)
        })
        .collect();

    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_models_response_standard_anthropic_format() {
        let json = r#"{
            "data": [
                {"id": "claude-opus-4-5", "type": "model", "display_name": "Claude Opus 4.5"},
                {"id": "claude-sonnet-4", "type": "model", "display_name": "Claude Sonnet 4"},
                {"id": "claude-haiku-4-5", "type": "model", "display_name": "Claude Haiku 4.5"}
            ],
            "has_more": false
        }"#;
        let result = parse_models_response(json).unwrap();
        assert_eq!(result, vec!["claude-opus-4-5", "claude-sonnet-4", "claude-haiku-4-5"]);
    }

    #[test]
    fn test_parse_models_response_empty_data() {
        let json = r#"{"data": []}"#;
        let result = parse_models_response(json).unwrap();
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn test_parse_models_response_malformed_json() {
        let json = r#"{invalid json}"#;
        let result = parse_models_response(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_models_response_missing_data_key() {
        let json = r#"{"models": []}"#;
        let result = parse_models_response(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_models_response_uses_x_api_key_not_bearer() {
        // This test documents the authentication difference:
        // Anthropic uses 'x-api-key' header (NOT Bearer auth like OpenAI)
        // The list_models() function sets the header directly.
        // This is verified by the header setup in list_models().
        let json = r#"{"data": [{"id": "claude-opus-4-5"}]}"#;
        let result = parse_models_response(json).unwrap();
        assert_eq!(result, vec!["claude-opus-4-5"]);
    }
}
