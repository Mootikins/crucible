//! OpenAI chat provider implementation

use async_trait::async_trait;
use crucible_core::traits::{
    LlmMessage, LlmProvider, LlmRequest, LlmResponse, LlmError, LlmResult,
    MessageRole, ToolCall, TokenUsage,
};
use serde::Deserialize;
use std::time::Duration;

/// OpenAI chat provider
pub struct OpenAIChatProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    default_model: String,
    timeout: Duration,
}

impl OpenAIChatProvider {
    /// Create a new OpenAI provider
    pub fn new(
        api_key: String,
        base_url: Option<String>,
        model: String,
        timeout_secs: u64,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            default_model: model,
            timeout: Duration::from_secs(timeout_secs),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAIChatProvider {
    async fn complete(&self, request: LlmRequest) -> LlmResult<LlmResponse> {
        // Build OpenAI API request
        let mut api_request = serde_json::json!({
            "model": self.default_model,
            "messages": request.messages.iter().map(|m| {
                let mut msg = serde_json::json!({
                    "role": match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Tool => "tool",
                    },
                    "content": m.content.clone(),
                });

                // Add tool_call_id for tool messages
                if m.role == MessageRole::Tool {
                    if let Some(tool_call_id) = &m.tool_call_id {
                        msg["tool_call_id"] = serde_json::json!(tool_call_id);
                    }
                }

                // Add tool_calls for assistant messages
                if let Some(tool_calls) = &m.tool_calls {
                    let calls: Vec<serde_json::Value> = tool_calls.iter().map(|tc| {
                        serde_json::json!({
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": serde_json::to_string(&tc.parameters).unwrap_or_default(),
                            }
                        })
                    }).collect();
                    msg["tool_calls"] = serde_json::json!(calls);
                }

                msg
            }).collect::<Vec<_>>(),
        });

        // Add optional parameters
        if let Some(temp) = request.temperature {
            api_request["temperature"] = serde_json::json!(temp);
        }

        if let Some(max_tokens) = request.max_tokens {
            api_request["max_tokens"] = serde_json::json!(max_tokens);
        }

        // Add tool definitions if present
        if let Some(tools) = &request.tools {
            let openai_tools: Vec<serde_json::Value> = tools
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
            api_request["tools"] = serde_json::json!(openai_tools);
        }

        // Make request
        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
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
                "OpenAI API error ({}): {}",
                status, error_text
            )));
        }

        let openai_response: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("Failed to parse response: {}", e)))?;

        // Parse response into our LlmResponse
        let choice = openai_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| LlmError::InvalidResponse("No choices in response".to_string()))?;

        let message_content = choice.message.content.unwrap_or_default();

        // Check for tool calls
        let tool_calls = choice.message.tool_calls.map(|calls| {
            calls
                .into_iter()
                .map(|tc| {
                    // Parse arguments from string to JSON
                    let params = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::json!({}));

                    ToolCall::new(tc.id, tc.function.name, params)
                })
                .collect()
        });

        let message = if let Some(calls) = tool_calls {
            LlmMessage::assistant_with_tools(message_content, calls)
        } else {
            LlmMessage::assistant(message_content)
        };

        Ok(LlmResponse {
            message,
            usage: TokenUsage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                completion_tokens: openai_response.usage.completion_tokens,
                total_tokens: openai_response.usage.total_tokens,
            },
            model: openai_response.model,
        })
    }

    fn provider_name(&self) -> &str {
        "OpenAI"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn health_check(&self) -> LlmResult<bool> {
        let url = format!("{}/models", self.base_url);
        match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
        {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
    }
}

// OpenAI API response types
#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: OpenAIUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAIMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIToolCall {
    id: String,
    function: OpenAIFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunction {
    name: String,
    arguments: String, // JSON string
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider = OpenAIChatProvider::new(
            "sk-test-key".to_string(),
            None,
            "gpt-4".to_string(),
            60,
        );

        assert_eq!(provider.provider_name(), "OpenAI");
        assert_eq!(provider.default_model(), "gpt-4");
    }
}
