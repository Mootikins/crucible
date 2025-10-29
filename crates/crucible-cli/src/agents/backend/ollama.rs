use super::{Backend, ChatParams, Message, Model};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::{pin::Pin, sync::Arc};

/// Ollama backend implementation using reqwest
pub struct OllamaBackend {
    endpoint: String,
    client: Arc<dyn OllamaHttpClient>,
}

impl OllamaBackend {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self::with_http_client(endpoint, Arc::new(ReqwestOllamaHttpClient::default()))
    }

    fn with_http_client(endpoint: impl Into<String>, client: Arc<dyn OllamaHttpClient>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_client(
        endpoint: impl Into<String>,
        client: Arc<dyn OllamaHttpClient>,
    ) -> Self {
        Self::with_http_client(endpoint, client)
    }

    pub fn name(&self) -> &str {
        "Ollama"
    }

    fn build_chat_request(messages: Vec<Message>, params: &ChatParams) -> OllamaChatRequest {
        let options = if params.temperature.is_some() || params.max_tokens.is_some() {
            Some(OllamaOptions {
                temperature: params.temperature,
                num_predict: params.max_tokens,
            })
        } else {
            None
        };

        OllamaChatRequest {
            model: params.model.clone(),
            messages,
            options,
            stream: false,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub(crate) struct OllamaChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    stream: bool,
}

#[derive(Debug, Serialize, Clone)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub(crate) struct OllamaChatResponse {
    message: Message,
    done: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct OllamaModelsResponse {
    models: Vec<Model>,
}

#[async_trait]
pub(crate) trait OllamaHttpClient: Send + Sync {
    async fn post_chat(&self, url: &str, request: &OllamaChatRequest)
        -> Result<OllamaChatResponse>;
    async fn get_models(&self, url: &str) -> Result<OllamaModelsResponse>;
}

struct ReqwestOllamaHttpClient {
    client: reqwest::Client,
}

impl Default for ReqwestOllamaHttpClient {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl OllamaHttpClient for ReqwestOllamaHttpClient {
    async fn post_chat(
        &self,
        url: &str,
        request: &OllamaChatRequest,
    ) -> Result<OllamaChatResponse> {
        let response = self
            .client
            .post(url)
            .json(request)
            .send()
            .await
            .context("Failed to send chat request")?;

        if !response.status().is_success() {
            anyhow::bail!("Chat request failed with status: {}", response.status());
        }

        response
            .json()
            .await
            .context("Failed to parse chat response")
    }

    async fn get_models(&self, url: &str) -> Result<OllamaModelsResponse> {
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to fetch models")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to list models: {}", response.status());
        }

        response
            .json()
            .await
            .context("Failed to parse models response")
    }
}

#[async_trait]
impl Backend for OllamaBackend {
    async fn chat(&self, messages: Vec<Message>, params: &ChatParams) -> Result<String> {
        let request = Self::build_chat_request(messages, params);

        let url = format!("{}/api/chat", self.endpoint);
        let chat_response = self.client.post_chat(&url, &request).await?;

        Ok(chat_response.message.content)
    }

    async fn chat_stream(
        &self,
        _messages: Vec<Message>,
        _params: &ChatParams,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String>> + Send>>> {
        // TODO: Implement streaming
        unimplemented!("chat_stream not yet implemented")
    }

    async fn list_models(&self) -> Result<Vec<Model>> {
        let url = format!("{}/api/tags", self.endpoint);
        let models_response = self.client.get_models(&url).await?;

        Ok(models_response.models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct StubHttpClient {
        chat_response: Mutex<Option<OllamaChatResponse>>,
        chat_error: Mutex<Option<String>>,
        models_response: Mutex<Option<OllamaModelsResponse>>,
        models_error: Mutex<Option<String>>,
        chat_requests: Mutex<Vec<(String, OllamaChatRequest)>>,
        model_urls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl OllamaHttpClient for StubHttpClient {
        async fn post_chat(
            &self,
            url: &str,
            request: &OllamaChatRequest,
        ) -> Result<OllamaChatResponse> {
            self.chat_requests
                .lock()
                .unwrap()
                .push((url.to_string(), request.clone()));

            if let Some(err) = self.chat_error.lock().unwrap().take() {
                return Err(anyhow!(err));
            }

            if let Some(response) = self.chat_response.lock().unwrap().clone() {
                return Ok(response);
            }

            Err(anyhow!("chat response not configured"))
        }

        async fn get_models(&self, url: &str) -> Result<OllamaModelsResponse> {
            self.model_urls.lock().unwrap().push(url.to_string());

            if let Some(err) = self.models_error.lock().unwrap().take() {
                return Err(anyhow!(err));
            }

            if let Some(response) = self.models_response.lock().unwrap().clone() {
                return Ok(response);
            }

            Err(anyhow!("models response not configured"))
        }
    }

    #[tokio::test]
    async fn test_list_models_returns_available_models() {
        let models_response = OllamaModelsResponse {
            models: vec![
                Model {
                    name: "qwen2.5-coder:7b".to_string(),
                    modified_at: Some("2024-01-01T00:00:00Z".to_string()),
                    size: Some(4661211808),
                    digest: Some("abcd1234".to_string()),
                    details: None,
                },
                Model {
                    name: "llama3.1:8b".to_string(),
                    modified_at: Some("2024-01-02T00:00:00Z".to_string()),
                    size: Some(4_880_000_000),
                    digest: Some("efgh5678".to_string()),
                    details: None,
                },
            ],
        };

        let client = Arc::new(StubHttpClient {
            models_response: Mutex::new(Some(models_response)),
            ..Default::default()
        });

        let backend = OllamaBackend::new_with_client("http://localhost", client.clone());
        let models = backend.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "qwen2.5-coder:7b");
        assert_eq!(models[1].name, "llama3.1:8b");
        assert_eq!(models[0].size, Some(4661211808));

        let urls = client.model_urls.lock().unwrap();
        assert_eq!(urls.as_slice(), ["http://localhost/api/tags"]);
    }

    #[tokio::test]
    async fn test_chat_sends_correct_request_format() {
        let client = Arc::new(StubHttpClient {
            chat_response: Mutex::new(Some(OllamaChatResponse {
                message: Message::assistant("Hello! How can I help you?"),
                done: true,
            })),
            ..Default::default()
        });

        let backend = OllamaBackend::new_with_client("http://localhost", client.clone());
        let messages = vec![Message::user("Hello")];
        let params = ChatParams {
            model: "qwen2.5-coder:7b".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
        };

        let response = backend.chat(messages, &params).await.unwrap();
        assert_eq!(response, "Hello! How can I help you?");

        let requests = client.chat_requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let (url, recorded_request) = &requests[0];
        assert_eq!(url, "http://localhost/api/chat");
        assert_eq!(recorded_request.model, "qwen2.5-coder:7b");
        assert_eq!(recorded_request.messages.len(), 1);
        assert_eq!(recorded_request.messages[0].content, "Hello");
        assert_eq!(
            recorded_request.options.as_ref().unwrap().temperature,
            Some(0.7)
        );
    }

    #[tokio::test]
    async fn test_chat_includes_temperature_when_provided() {
        let client = Arc::new(StubHttpClient {
            chat_response: Mutex::new(Some(OllamaChatResponse {
                message: Message::assistant("Response"),
                done: true,
            })),
            ..Default::default()
        });

        let backend = OllamaBackend::new_with_client("http://localhost", client.clone());
        let messages = vec![Message::user("Test")];
        let params = ChatParams {
            model: "qwen2.5-coder:7b".to_string(),
            temperature: Some(0.2),
            max_tokens: None,
        };

        let _ = backend.chat(messages, &params).await;

        let requests = client.chat_requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let (_url, recorded_request) = &requests[0];
        assert_eq!(
            recorded_request.options.as_ref().unwrap().temperature,
            Some(0.2)
        );
        assert_eq!(recorded_request.options.as_ref().unwrap().num_predict, None);
    }

    #[tokio::test]
    async fn test_chat_handles_error_responses() {
        let client = Arc::new(StubHttpClient {
            chat_error: Mutex::new(Some("Internal Server Error".to_string())),
            ..Default::default()
        });

        let backend = OllamaBackend::new_with_client("http://localhost", client);
        let messages = vec![Message::user("Hello")];
        let params = ChatParams {
            model: "qwen2.5-coder:7b".to_string(),
            temperature: None,
            max_tokens: None,
        };

        let result = backend.chat(messages, &params).await;
        assert!(result.is_err());
    }
}
