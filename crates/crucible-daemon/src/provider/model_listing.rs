pub mod anthropic {
    use super::{ModelListingError, ModelListingResult};
    use serde_json::Value;
    use std::time::Duration;

    pub async fn list_models(endpoint: &str, api_key: &str) -> ModelListingResult<Vec<String>> {
        let endpoint = endpoint.trim_end_matches('/');
        let url = format!("{}/v1/models", endpoint);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        let mut request = client.get(&url);
        if !api_key.is_empty() {
            request = request.header("x-api-key", api_key);
        }

        let response = request
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

    pub fn parse_models_response(body: &str) -> ModelListingResult<Vec<String>> {
        let payload: Value = serde_json::from_str(body)?;
        let data = payload
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ModelListingError::Api("expected 'data' key with array value in response".into())
            })?;

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
}

pub mod ollama {
    use super::{ModelListingError, ModelListingResult};
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    #[derive(Debug, Serialize, Deserialize)]
    struct TagsResponse {
        models: Vec<ModelTag>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct ModelTag {
        name: String,
    }

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

    pub fn parse_tags_response(body: &str) -> ModelListingResult<Vec<String>> {
        let response: TagsResponse = serde_json::from_str(body)?;
        Ok(response.models.into_iter().map(|m| m.name).collect())
    }
}

pub mod openai_compat {
    use super::{ModelListingError, ModelListingResult};
    use serde_json::Value;
    use std::time::Duration;

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
            return Err(ModelListingError::Api(format!("HTTP {}: {}", status, text)));
        }

        let body = response.text().await?;
        parse_models_response(&body)
    }

    pub fn parse_models_response(body: &str) -> ModelListingResult<Vec<String>> {
        let payload: Value = serde_json::from_str(body)?;

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

        if let Some(data) = payload.get("data") {
            if let Some(data_array) = data.as_array() {
                return Ok(model_names_from_array(data_array));
            }
        }

        if let Some(models) = payload.get("models") {
            if let Some(models_array) = models.as_array() {
                return Ok(model_names_from_array(models_array));
            }
        }

        Err(ModelListingError::Api(
            "expected 'data' or 'models' key in response".into(),
        ))
    }
}

use crate::provider::copilot::CopilotClient;
use crucible_config::BackendType;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelListingError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("API error: {0}")]
    Api(String),
}

pub type ModelListingResult<T> = Result<T, ModelListingError>;

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
                let client = CopilotClient::new(token.to_string());
                match client.list_models().await {
                    Ok(models) => Ok(models.into_iter().map(|m| m.id).collect()),
                    Err(_) => Ok(vec![]),
                }
            } else {
                Ok(vec![])
            }
        }
        BackendType::Cohere
        | BackendType::VertexAI
        | BackendType::FastEmbed
        | BackendType::Burn
        | BackendType::Custom
        | BackendType::Mock => Ok(vec![]),
    }
}
