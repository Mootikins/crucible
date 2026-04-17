//! Fetches a model's context length from an OpenAI-compatible endpoint.
//!
//! Probes `/v1/models` (for llamaswap metadata), falling back to Ollama's
//! `/api/show` if the first endpoint does not advertise a context length.
//! Intended for use during `session.create` setup when the daemon needs to
//! discover the context window of a newly configured model.

use std::time::Duration;

/// Fetch context length for a model from OpenAI-compatible /v1/models endpoint
/// Falls back to Ollama /api/show if /v1/models doesn't provide context length
pub async fn fetch_model_context_length(endpoint: &str, model_id: &str) -> Option<usize> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    // Try OpenAI-compatible /v1/models endpoint first
    let url = format!("{}/v1/models", endpoint.trim_end_matches('/'));
    let resp = client.get(&url).send().await.ok()?;

    if !resp.status().is_success() {
        return try_ollama_api_show(&client, endpoint, model_id).await;
    }

    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelData>,
    }

    #[derive(serde::Deserialize)]
    struct ModelData {
        id: String,
        #[serde(default)]
        meta: Option<ModelMeta>,
    }

    #[derive(serde::Deserialize)]
    struct ModelMeta {
        #[serde(default)]
        llamaswap: Option<LlamaSwapMeta>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LlamaSwapMeta {
        context_length: Option<usize>,
    }

    let models: ModelsResponse = resp.json().await.ok()?;
    let result = models
        .data
        .iter()
        .find(|m| m.id == model_id)
        .and_then(|m| m.meta.as_ref())
        .and_then(|meta| meta.llamaswap.as_ref())
        .and_then(|ls| ls.context_length);

    // If llamaswap didn't provide context length, try Ollama /api/show
    if result.is_none() {
        return try_ollama_api_show(&client, endpoint, model_id).await;
    }

    result
}

/// Try to fetch context length from Ollama's /api/show endpoint
async fn try_ollama_api_show(
    client: &reqwest::Client,
    endpoint: &str,
    model_id: &str,
) -> Option<usize> {
    // Strip /v1 suffix if present to get the base Ollama endpoint
    let base_url = endpoint
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .to_string();

    let url = format!("{}/api/show", base_url);

    #[derive(serde::Serialize)]
    struct ShowRequest {
        model: String,
    }

    #[derive(serde::Deserialize)]
    struct ShowResponse {
        #[serde(default)]
        model_info: Option<serde_json::Value>,
        #[serde(default)]
        parameters: Option<String>,
    }

    let req_body = ShowRequest {
        model: model_id.to_string(),
    };

    let resp = client.post(&url).json(&req_body).send().await.ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let show_resp: ShowResponse = resp.json().await.ok()?;

    // Try to extract context length from model_info
    if let Some(model_info) = show_resp.model_info {
        if let Some(ctx_len) = model_info.get("llama.context_length") {
            if let Some(n) = ctx_len.as_u64() {
                return Some(n as usize);
            }
        }
        if let Some(ctx_len) = model_info.get("context_length") {
            if let Some(n) = ctx_len.as_u64() {
                return Some(n as usize);
            }
        }
        // Try any key containing "context_length"
        for (key, value) in model_info.as_object().iter().flat_map(|o| o.iter()) {
            if key.contains("context_length") {
                if let Some(n) = value.as_u64() {
                    return Some(n as usize);
                }
            }
        }
    }

    // Try to extract from parameters string (e.g., "num_ctx 4096")
    if let Some(params) = show_resp.parameters {
        if let Some(pos) = params.find("num_ctx") {
            let after_num_ctx = &params[pos + 7..];
            if let Some(num_str) = after_num_ctx.split_whitespace().next() {
                if let Ok(n) = num_str.parse::<usize>() {
                    return Some(n);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_none_when_endpoint_unreachable() {
        // Port 1 is reserved and reliably refuses connections.
        let result = fetch_model_context_length("http://127.0.0.1:1", "test-model").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn fetches_context_length_from_ollama_api_show() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // Mock the /v1/models endpoint to return 404 (not found)
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Mock the /api/show endpoint with llama.context_length
        let show_response = serde_json::json!({
            "model_info": {
                "llama.context_length": 131072,
                "llama.embedding_length": 4096
            }
        });

        Mock::given(method("POST"))
            .and(path("/api/show"))
            .respond_with(ResponseTemplate::new(200).set_body_json(show_response))
            .mount(&mock_server)
            .await;

        let result = fetch_model_context_length(&mock_server.uri(), "test-model").await;
        assert_eq!(result, Some(131072));
    }
}
