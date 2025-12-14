//! OpenAI-compatible API client with grammar support

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: {0}")]
    Api(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Timeout after {0}s")]
    Timeout(u64),
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// Chat completion request
#[derive(Debug, Clone, Serialize)]
pub struct CompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
    /// Additional kwargs passed to the chat template (e.g., {"enable_thinking": false})
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_template_kwargs: Option<HashMap<String, Value>>,
}

/// Text completion request (no chat template)
#[derive(Debug, Clone, Serialize)]
pub struct TextCompletionRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grammar: Option<String>,
}

impl TextCompletionRequest {
    pub fn new(model: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            prompt: prompt.into(),
            max_tokens: Some(256),
            temperature: Some(0.0),
            grammar: None,
        }
    }

    pub fn with_grammar(mut self, grammar: impl Into<String>) -> Self {
        self.grammar = Some(grammar.into());
        self
    }

    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }
}

impl CompletionRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            max_tokens: Some(256),
            temperature: Some(0.0), // Deterministic for testing
            grammar: None,
            chat_template_kwargs: None,
        }
    }

    pub fn with_grammar(mut self, grammar: impl Into<String>) -> Self {
        self.grammar = Some(grammar.into());
        self
    }

    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Disable thinking mode for thinking models (e.g., Qwen3)
    /// This prevents the `<think>` token from interfering with grammar constraints
    pub fn without_thinking(mut self) -> Self {
        let mut kwargs = self.chat_template_kwargs.unwrap_or_default();
        kwargs.insert("enable_thinking".to_string(), Value::Bool(false));
        self.chat_template_kwargs = Some(kwargs);
        self
    }

    /// Set arbitrary chat template kwargs
    pub fn with_chat_template_kwargs(mut self, kwargs: HashMap<String, Value>) -> Self {
        self.chat_template_kwargs = Some(kwargs);
        self
    }
}

/// API response structures
#[derive(Debug, Clone, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
    #[serde(default)]
    pub timings: Option<Timings>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Choice {
    pub index: u32,
    pub message: ResponseMessage,
    pub finish_reason: String,
}

/// Text completion response (for /v1/completions)
#[derive(Debug, Clone, Deserialize)]
pub struct TextCompletionResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<TextChoice>,
    pub usage: Usage,
    #[serde(default)]
    pub timings: Option<Timings>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TextChoice {
    pub index: u32,
    pub text: String,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseMessage {
    pub role: String,
    pub content: String,
    /// Reasoning/thinking content (populated when thinking mode is enabled)
    #[serde(default)]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Timings {
    pub prompt_ms: Option<f64>,
    pub predicted_ms: Option<f64>,
    pub predicted_per_token_ms: Option<f64>,
}

/// Model info from /v1/models
#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub owned_by: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

/// Client for llama-server / llama-swap
pub struct LlamaClient {
    client: Client,
    base_url: String,
    timeout: Duration,
}

impl LlamaClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        // Accept self-signed certs for local/internal endpoints
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("Failed to build HTTP client");
        Self {
            client,
            base_url: base_url.into(),
            timeout: Duration::from_secs(120),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// List available models
    pub async fn list_models(&self) -> ApiResult<Vec<ModelInfo>> {
        let url = format!("{}/v1/models", self.base_url);
        let resp: ModelsResponse = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await?
            .json()
            .await?;
        Ok(resp.data)
    }

    /// Send a chat completion request
    pub async fn complete(&self, request: CompletionRequest) -> ApiResult<(CompletionResponse, Duration)> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let start = Instant::now();

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .timeout(self.timeout)
            .send()
            .await?;

        let elapsed = start.elapsed();

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ApiError::Api(text));
        }

        let response: CompletionResponse = resp.json().await?;
        Ok((response, elapsed))
    }

    /// Send a text completion request (bypasses chat template)
    pub async fn complete_text(&self, request: TextCompletionRequest) -> ApiResult<(TextCompletionResponse, Duration)> {
        let url = format!("{}/v1/completions", self.base_url);
        let start = Instant::now();

        let resp = self
            .client
            .post(&url)
            .json(&request)
            .timeout(self.timeout)
            .send()
            .await?;

        let elapsed = start.elapsed();

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ApiError::Api(text));
        }

        let response: TextCompletionResponse = resp.json().await?;
        Ok((response, elapsed))
    }

    /// Get the content from a completion response
    pub fn extract_content(response: &CompletionResponse) -> Option<&str> {
        response
            .choices
            .first()
            .map(|c| c.message.content.as_str())
    }

    /// Get the text from a text completion response
    pub fn extract_text(response: &TextCompletionResponse) -> Option<&str> {
        response.choices.first().map(|c| c.text.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_builders() {
        let sys = ChatMessage::system("You are helpful");
        assert_eq!(sys.role, "system");

        let user = ChatMessage::user("Hello");
        assert_eq!(user.role, "user");
    }

    #[test]
    fn test_completion_request_builder() {
        let req = CompletionRequest::new("model", vec![ChatMessage::user("Hi")])
            .with_grammar("root ::= \"hello\"")
            .with_max_tokens(100);

        assert_eq!(req.grammar, Some("root ::= \"hello\"".to_string()));
        assert_eq!(req.max_tokens, Some(100));
    }
}
