//! Ollama chat provider implementation

use async_trait::async_trait;
use crucible_core::traits::{
    LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmError, LlmResult, LlmToolDefinition,
    MessageRole, ToolCall, TokenUsage,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Ollama chat provider
pub struct OllamaChatProvider {
    client: reqwest::Client,
    base_url: String,
    default_model: String,
    timeout: Duration,
}

impl OllamaChatProvider {
    /// Create a new Ollama provider
    pub fn new(base_url: String, model: String, timeout_secs: u64) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url,
            default_model: model,
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaChatProvider {
    async fn complete(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
        // Build Ollama API request
        let mut api_request = serde_json::json!({
            "model": self.default_model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Tool => "tool",
                    },
                    "content": m.content.clone(),
                })
            }).collect::<Vec<_>>(),
            "stream": false,
        });

        // Add optional parameters
        if let Some(temp) = request.temperature {
            api_request["options"] = serde_json::json!({
                "temperature": temp,
            });
        }

        if let Some(max_tokens) = request.max_tokens {
            if let Some(options) = api_request.get_mut("options") {
                options["num_predict"] = serde_json::json!(max_tokens);
            } else {
                api_request["options"] = serde_json::json!({
                    "num_predict": max_tokens,
                });
            }
        }

        // Add tool definitions if present
        if let Some(tools) = &request.tools {
            let ollama_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|tool| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters,
                        }
                    })
                })
                .collect();
            api_request["tools"] = serde_json::json!(ollama_tools);
        }

        // Make request
        let url = format!("{}/api/chat", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&api_request)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| LlmError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(LlmError::InvalidResponse(format!(
                "Ollama API error ({}): {}",
                status, error_text
            )));
        }

        let ollama_response: OllamaResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("Failed to parse response: {}", e)))?;

        // Parse response into our LlmResponse
        let message_content = ollama_response.message.content;

        // Check for tool calls
        let tool_calls = ollama_response.message.tool_calls.map(|calls| {
            calls
                .into_iter()
                .map(|tc| {
                    ToolCall::new(
                        tc.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
                        tc.function.name,
                        tc.function.arguments,
                    )
                })
                .collect()
        });

        let message = if let Some(calls) = tool_calls {
            LlmMessage::assistant_with_tools(message_content, calls)
        } else {
            LlmMessage::assistant(message_content)
        };

        // Extract token usage (Ollama provides these)
        let prompt_tokens = ollama_response.prompt_eval_count.unwrap_or(0);
        let completion_tokens = ollama_response.eval_count.unwrap_or(0);

        Ok(LlmResponse {
            message,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            model: ollama_response.model,
        })
    }

    fn provider_name(&self) -> &str {
        "Ollama"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn health_check(&self) -> LlmResult<bool> {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

// Ollama API response types
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    model: String,
    message: OllamaMessage,
    #[serde(rename = "prompt_eval_count")]
    prompt_eval_count: Option<u32>,
    #[serde(rename = "eval_count")]
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OllamaToolCall {
    id: Option<String>,
    function: OllamaFunction,
}

#[derive(Debug, Deserialize)]
struct OllamaFunction {
    name: String,
    arguments: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_provider_creation() {
        let provider = OllamaChatProvider::new(
            "http://localhost:11434".to_string(),
            "llama3.2".to_string(),
            120,
        );

        assert_eq!(provider.provider_name(), "Ollama");
        assert_eq!(provider.default_model(), "llama3.2");
    }

    // Integration tests will be in tests/ directory
}
