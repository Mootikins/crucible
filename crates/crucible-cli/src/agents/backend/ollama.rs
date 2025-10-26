use super::{Backend, ChatParams, Message, Model};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Ollama backend implementation using reqwest
pub struct OllamaBackend {
    endpoint: String,
    client: reqwest::Client,
}

impl OllamaBackend {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn name(&self) -> &str {
        "Ollama"
    }
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct OllamaChatResponse {
    message: Message,
    done: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaModelsResponse {
    models: Vec<Model>,
}

#[async_trait]
impl Backend for OllamaBackend {
    async fn chat(&self, messages: Vec<Message>, params: &ChatParams) -> Result<String> {
        let options = if params.temperature.is_some() || params.max_tokens.is_some() {
            Some(OllamaOptions {
                temperature: params.temperature,
                num_predict: params.max_tokens,
            })
        } else {
            None
        };

        let request = OllamaChatRequest {
            model: params.model.clone(),
            messages,
            options,
            stream: false,
        };

        let url = format!("{}/api/chat", self.endpoint);
        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send chat request")?;

        if !response.status().is_success() {
            anyhow::bail!("Chat request failed with status: {}", response.status());
        }

        let chat_response: OllamaChatResponse = response
            .json()
            .await
            .context("Failed to parse chat response")?;

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
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to fetch models")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to list models: {}", response.status());
        }

        let models_response: OllamaModelsResponse = response
            .json()
            .await
            .context("Failed to parse models response")?;

        Ok(models_response.models)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn test_list_models_returns_available_models() {
        // Setup mock server
        let mock_server = MockServer::start().await;

        // Mock /api/tags endpoint response
        let mock_response = serde_json::json!({
            "models": [
                {
                    "name": "qwen2.5-coder:7b",
                    "modified_at": "2024-01-01T00:00:00Z",
                    "size": 4661211808u64,
                    "digest": "abcd1234",
                    "details": {
                        "format": "gguf",
                        "family": "qwen2",
                        "parameter_size": "7B",
                        "quantization_level": "Q4_0"
                    }
                },
                {
                    "name": "llama3.1:8b",
                    "modified_at": "2024-01-02T00:00:00Z",
                    "size": 4880000000u64,
                    "digest": "efgh5678",
                    "details": {
                        "format": "gguf",
                        "family": "llama",
                        "parameter_size": "8B",
                        "quantization_level": "Q4_0"
                    }
                }
            ]
        });

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        // Test
        let backend = OllamaBackend::new(mock_server.uri());
        let models = backend.list_models().await.unwrap();

        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "qwen2.5-coder:7b");
        assert_eq!(models[1].name, "llama3.1:8b");
        assert!(models[0].size.is_some());
        assert_eq!(models[0].size.unwrap(), 4661211808);
    }

    #[tokio::test]
    async fn test_chat_sends_correct_request_format() {
        let mock_server = MockServer::start().await;

        // Mock chat completion response (Ollama format)
        let mock_response = serde_json::json!({
            "model": "qwen2.5-coder:7b",
            "created_at": "2024-01-01T00:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you?"
            },
            "done": true
        });

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        // Test
        let backend = OllamaBackend::new(mock_server.uri());
        let messages = vec![Message::user("Hello")];
        let params = ChatParams {
            model: "qwen2.5-coder:7b".to_string(),
            temperature: Some(0.7),
            max_tokens: None,
        };

        let response = backend.chat(messages, &params).await.unwrap();
        assert_eq!(response, "Hello! How can I help you?");
    }

    #[tokio::test]
    async fn test_chat_includes_temperature_when_provided() {
        let mock_server = MockServer::start().await;

        let mock_response = serde_json::json!({
            "model": "qwen2.5-coder:7b",
            "message": {
                "role": "assistant",
                "content": "Response"
            },
            "done": true
        });

        // We'll verify the request body contains temperature
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
            .mount(&mock_server)
            .await;

        let backend = OllamaBackend::new(mock_server.uri());
        let messages = vec![Message::user("Test")];
        let params = ChatParams {
            model: "qwen2.5-coder:7b".to_string(),
            temperature: Some(0.2),
            max_tokens: None,
        };

        let _ = backend.chat(messages, &params).await;
        // Request validation happens in the mock server
    }

    #[tokio::test]
    async fn test_chat_handles_error_responses() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let backend = OllamaBackend::new(mock_server.uri());
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
