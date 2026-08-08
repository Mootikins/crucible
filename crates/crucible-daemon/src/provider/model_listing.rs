/// The HTTP client every model-listing probe uses.
///
/// `redirect::Policy::none()` is the load-bearing part. The endpoint a probe is
/// given has already been checked against the internal-address deny list
/// (`crucible-web`'s `validate_endpoint`), but that check applies to the URL we
/// were handed — not to wherever it points us next. Following redirects makes
/// the check meaningless: one `302 Location: http://169.254.169.254/` from a
/// validated public endpoint is a full SSRF, needing no DNS control at all,
/// just an HTTP server. No provider's model-listing API has any reason to
/// redirect, so a redirect is simply returned to the caller as the non-success
/// status it is.
fn http_client(timeout: std::time::Duration) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

/// How long a model-listing probe waits. Listing is interactive (a UI populates
/// a model picker from it), so it fails fast rather than hanging the caller.
const LIST_MODELS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub mod anthropic {
    use super::{http_client, ModelListingError, ModelListingResult, LIST_MODELS_TIMEOUT};
    use serde_json::Value;

    pub async fn list_models(endpoint: &str, api_key: &str) -> ModelListingResult<Vec<String>> {
        let endpoint = endpoint.trim_end_matches('/');
        let url = format!("{}/v1/models", endpoint);

        let client = http_client(LIST_MODELS_TIMEOUT)?;

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
    use super::{http_client, ModelListingError, ModelListingResult, LIST_MODELS_TIMEOUT};
    use serde::{Deserialize, Serialize};

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

        let client = http_client(LIST_MODELS_TIMEOUT)?;

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
    use super::{http_client, ModelListingError, ModelListingResult, LIST_MODELS_TIMEOUT};
    use serde_json::Value;

    pub async fn list_models(endpoint: &str, api_key: &str) -> ModelListingResult<Vec<String>> {
        let endpoint = endpoint.trim_end_matches('/');
        let url = format!("{}/models", endpoint);

        let client = http_client(LIST_MODELS_TIMEOUT)?;

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
use crucible_core::config::BackendType;
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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// A server that answers every request with a redirect to `target`.
    async fn redirecting_to(target: &MockServer) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", target.uri().as_str()),
            )
            .mount(&server)
            .await;
        server
    }

    /// The SSRF the endpoint allow-list does not cover on its own: the URL that
    /// was validated answers `302` and names an address nothing ever checked.
    /// Every listing probe must stop at the redirect rather than dial it.
    ///
    /// `internal` stands in for 169.254.169.254 — what is under test is whether
    /// the client dials the `Location` at all, not which address it names.
    #[tokio::test]
    async fn a_provider_redirect_is_never_followed() {
        let internal = MockServer::start().await;
        let validated = redirecting_to(&internal).await;
        let endpoint = validated.uri();

        assert!(ollama::list_models(&endpoint).await.is_err());
        assert!(openai_compat::list_models(&endpoint, "").await.is_err());
        assert!(anthropic::list_models(&endpoint, "").await.is_err());

        let dialed = internal.received_requests().await.unwrap_or_default();
        assert!(
            dialed.is_empty(),
            "the redirect target was dialed {} time(s) — a 302 from a validated \
             endpoint would bypass the internal-address check entirely",
            dialed.len()
        );
    }
}
