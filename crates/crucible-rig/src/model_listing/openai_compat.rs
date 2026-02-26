//! OpenAI-compatible model listing wrapper
//!
//! Discovers available models from OpenAI-compatible endpoints
//! (OpenAI, OpenRouter, ZAI, etc.) via the `/models` endpoint.
//!
//! Handles both standard OpenAI format (`data[].id`) and fallback formats (`models[].name`).

use super::{ModelListingError, ModelListingResult};
use serde_json::Value;
use std::time::Duration;

/// List available models from an OpenAI-compatible endpoint
///
/// Queries the `/models` endpoint with a 10-second timeout.
/// Extracts model IDs from the response.
///
/// # Arguments
///
/// * `endpoint` - API endpoint (e.g., "https://api.openai.com/v1")
/// * `api_key` - API key for authentication (optional, but required for some endpoints)
///
/// # Returns
///
/// A list of model identifiers, or an error if the request fails.
pub async fn list_models(endpoint: &str, api_key: &str) -> ModelListingResult<Vec<String>> {
    let endpoint = endpoint.trim_end_matches('/');
    let url = format!("{}/models", endpoint);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let mut request = client.get(&url);
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }

    let response = request.send().await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let text = response.text().await.unwrap_or_default();
        return Err(ModelListingError::Api(format!(
            "HTTP {}: {}",
            status, text
        )));
    }

    let body = response.text().await?;
    parse_models_response(&body)
}

/// Parse OpenAI-compatible models response
///
/// Handles both standard OpenAI format (`data[].id`) and fallback formats (`models[].name`).
/// Falls back to `name` field if `id` is not present.
///
/// # Arguments
///
/// * `body` - JSON response body
///
/// # Returns
///
/// A list of model identifiers, or an error if parsing fails.
pub fn parse_models_response(body: &str) -> ModelListingResult<Vec<String>> {
    let payload: Value = serde_json::from_str(body)?;

    // Helper to extract model names from an array
    fn model_names_from_array(models: &[Value]) -> Vec<String> {
        models
            .iter()
            .filter_map(|model| {
                let obj = model.as_object()?;
                obj.get("id")
                    .and_then(Value::as_str)
                    .or_else(|| obj.get("name").and_then(Value::as_str))
                    .map(ToString::to_string)
            })
            .collect()
    }

    // Try 'data' key first (standard OpenAI format)
    if let Some(data) = payload.get("data") {
        if let Some(data_array) = data.as_array() {
            return Ok(model_names_from_array(data_array));
        }
    }

    // Try 'models' key as fallback
    if let Some(models) = payload.get("models") {
        if let Some(models_array) = models.as_array() {
            return Ok(model_names_from_array(models_array));
        }
    }

    // Neither key found
    Err(ModelListingError::Api(
        "expected 'data' or 'models' key in response".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_models_response_standard_openai_format() {
        let json = r#"{
            "data": [
                {"id": "gpt-4o", "object": "model"},
                {"id": "gpt-4-turbo", "object": "model"},
                {"id": "gpt-3.5-turbo", "object": "model"}
            ]
        }"#;
        let result = parse_models_response(json).unwrap();
        assert_eq!(result, vec!["gpt-4o", "gpt-4-turbo", "gpt-3.5-turbo"]);
    }

    #[test]
    fn test_parse_models_response_with_dall_e_and_embedding() {
        let json = r#"{
            "data": [
                {"id": "gpt-4o"},
                {"id": "dall-e-3"},
                {"id": "text-embedding-3-small"}
            ]
        }"#;
        let result = parse_models_response(json).unwrap();
        // All models returned, no filtering
        assert_eq!(result.len(), 3);
        assert!(result.contains(&"gpt-4o".to_string()));
        assert!(result.contains(&"dall-e-3".to_string()));
        assert!(result.contains(&"text-embedding-3-small".to_string()));
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
    fn test_parse_models_response_models_key_fallback() {
        let json = r#"{
            "models": [
                {"name": "model-a"},
                {"name": "model-b"}
            ]
        }"#;
        let result = parse_models_response(json).unwrap();
        assert_eq!(result, vec!["model-a", "model-b"]);
    }

    #[test]
    fn test_parse_models_response_missing_both_keys() {
        let json = r#"{"other_key": []}"#;
        let result = parse_models_response(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_models_response_id_fallback_to_name() {
        let json = r#"{
            "data": [
                {"id": "model-with-id"},
                {"name": "model-with-name-only"}
            ]
        }"#;
        let result = parse_models_response(json).unwrap();
        assert_eq!(result, vec!["model-with-id", "model-with-name-only"]);
    }
}

