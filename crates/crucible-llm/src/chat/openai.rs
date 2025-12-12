//! OpenAI chat provider implementation

use async_trait::async_trait;
use crucible_core::traits::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageDelta, CompletionChunk, CompletionRequest, CompletionResponse, FunctionCallDelta,
    LlmError, LlmMessage, LlmResult, MessageRole, ProviderCapabilities, TextGenerationProvider,
    TextModelInfo, TokenUsage, ToolCall, ToolCallDelta,
};
use futures::stream::BoxStream;
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
impl TextGenerationProvider for OpenAIChatProvider {
    async fn generate_completion(
        &self,
        _request: CompletionRequest,
    ) -> LlmResult<CompletionResponse> {
        todo!("OpenAI text completion not implemented")
    }

    fn generate_completion_stream<'a>(
        &'a self,
        _request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<CompletionChunk>> {
        todo!("OpenAI text completion streaming not implemented")
    }

    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse> {
        // Build OpenAI API request
        let mut api_request = serde_json::json!({
            "model": self.default_model,
            "messages": request.messages.iter().map(|m| {
                let mut msg = serde_json::json!({
                    "role": match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Function => "function",
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
                            "type": tc.r#type,
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments.clone(),
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
                        "type": tool.r#type,
                        "function": {
                            "name": tool.function.name,
                            "description": tool.function.description,
                            "parameters": tool.function.parameters.clone().unwrap_or(serde_json::json!({})),
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

        // Parse response into our ChatCompletionResponse
        let choice = openai_response
            .choices
            .first()
            .ok_or_else(|| LlmError::InvalidResponse("No choices in response".to_string()))?;

        let message_content = choice.message.content.clone().unwrap_or_default();
        let finish_reason = choice.finish_reason.clone();

        // Check for tool calls
        let tool_calls = choice.message.tool_calls.as_ref().map(|calls| {
            calls
                .iter()
                .map(|tc| {
                    // arguments is already a string
                    ToolCall::new(
                        tc.id.clone(),
                        tc.function.name.clone(),
                        tc.function.arguments.clone(),
                    )
                })
                .collect()
        });

        let message = if let Some(calls) = tool_calls {
            LlmMessage::assistant_with_tools(message_content, calls)
        } else {
            LlmMessage::assistant(message_content)
        };

        Ok(ChatCompletionResponse {
            choices: vec![ChatCompletionChoice {
                index: 0,
                message,
                finish_reason,
                logprobs: None,
            }],
            model: openai_response.model,
            usage: TokenUsage {
                prompt_tokens: openai_response.usage.prompt_tokens,
                completion_tokens: openai_response.usage.completion_tokens,
                total_tokens: openai_response.usage.total_tokens,
            },
            id: openai_response.id,
            object: "chat.completion".to_string(),
            created: chrono::Utc::now(),
            system_fingerprint: None,
        })
    }

    fn generate_chat_completion_stream<'a>(
        &'a self,
        request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        use async_stream::stream;
        use futures::StreamExt;

        // Build OpenAI API request
        let mut api_request = serde_json::json!({
            "model": self.default_model,
            "messages": request.messages.iter().map(|m| {
                let mut msg = serde_json::json!({
                    "role": match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Function => "function",
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
                            "type": tc.r#type,
                            "function": {
                                "name": tc.function.name,
                                "arguments": tc.function.arguments.clone(),
                            }
                        })
                    }).collect();
                    msg["tool_calls"] = serde_json::json!(calls);
                }

                msg
            }).collect::<Vec<_>>(),
            "stream": true,
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
                        "type": tool.r#type,
                        "function": {
                            "name": tool.function.name,
                            "description": tool.function.description,
                            "parameters": tool.function.parameters.clone().unwrap_or(serde_json::json!({})),
                        }
                    })
                })
                .collect();
            api_request["tools"] = serde_json::json!(openai_tools);
        }

        let url = format!("{}/chat/completions", self.base_url);
        let client = self.client.clone();
        let api_key = self.api_key.clone();
        let timeout = self.timeout;

        Box::pin(stream! {
            let response = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&api_request)
                .timeout(timeout)
                .send()
                .await;

            match response {
                Ok(res) if res.status().is_success() => {
                    let mut stream = res.bytes_stream();
                    let mut buffer = String::new();

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(bytes) => {
                                buffer.push_str(&String::from_utf8_lossy(&bytes));

                                // Process complete lines (SSE format: "data: {...}")
                                while let Some(line_end) = buffer.find('\n') {
                                    let line = buffer[..line_end].trim().to_string();
                                    buffer = buffer[line_end + 1..].to_string();

                                    if line.is_empty() || !line.starts_with("data: ") {
                                        continue;
                                    }

                                    let json_str = &line[6..]; // Skip "data: "
                                    if json_str == "[DONE]" {
                                        break;
                                    }

                                    match serde_json::from_str::<OpenAIStreamResponse>(json_str) {
                                        Ok(stream_resp) => {
                                            for choice in stream_resp.choices {
                                                // Convert tool calls if present
                                                let tool_calls = choice.delta.tool_calls.map(|calls| {
                                                    calls
                                                        .into_iter()
                                                        .map(|tc| ToolCallDelta {
                                                            index: tc.index,
                                                            id: tc.id,
                                                            function: tc.function.map(|f| FunctionCallDelta {
                                                                name: f.name,
                                                                arguments: f.arguments,
                                                            }),
                                                        })
                                                        .collect()
                                                });

                                                let chunk = ChatCompletionChunk {
                                                    index: choice.index,
                                                    delta: ChatMessageDelta {
                                                        role: choice.delta.role.map(|r| match r.as_str() {
                                                            "assistant" => MessageRole::Assistant,
                                                            "system" => MessageRole::System,
                                                            "user" => MessageRole::User,
                                                            "function" => MessageRole::Function,
                                                            "tool" => MessageRole::Tool,
                                                            _ => MessageRole::Assistant,
                                                        }),
                                                        content: choice.delta.content,
                                                        function_call: None,
                                                        tool_calls,
                                                    },
                                                    finish_reason: choice.finish_reason,
                                                    logprobs: None,
                                                };
                                                yield Ok(chunk);
                                            }
                                        }
                                        Err(e) => {
                                            yield Err(LlmError::InvalidResponse(format!(
                                                "Failed to parse stream: {}",
                                                e
                                            )));
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                yield Err(LlmError::HttpError(e.to_string()));
                            }
                        }
                    }
                }
                Ok(res) => {
                    let status = res.status();
                    let error_text = res.text().await.unwrap_or_default();
                    yield Err(LlmError::InvalidResponse(format!(
                        "OpenAI API error ({}): {}",
                        status, error_text
                    )));
                }
                Err(e) => {
                    yield Err(LlmError::HttpError(e.to_string()));
                }
            }
        })
    }

    async fn list_models(&self) -> LlmResult<Vec<TextModelInfo>> {
        todo!("OpenAI model listing not implemented")
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

    fn provider_name(&self) -> &str {
        "OpenAI"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            text_completion: false,
            chat_completion: true,
            streaming: true,
            function_calling: true,
            tool_use: true,
            vision: false,
            audio: false,
            max_batch_size: None,
            input_formats: vec!["text".to_string()],
            output_formats: vec!["text".to_string()],
        }
    }
}

// OpenAI API response types
#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    model: String,
    choices: Vec<OpenAIChoice>,
    usage: OpenAIUsage,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
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

// OpenAI streaming response types
#[derive(Debug, Deserialize)]
struct OpenAIStreamResponse {
    choices: Vec<OpenAIStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamChoice {
    index: u32,
    delta: OpenAIDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIDelta {
    role: Option<String>,
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamToolCall {
    index: u32,
    id: Option<String>,
    #[serde(rename = "type")]
    r#type: Option<String>,
    function: Option<OpenAIStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAIStreamFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_provider_creation() {
        let provider =
            OpenAIChatProvider::new("sk-test-key".to_string(), None, "gpt-4".to_string(), 60);

        assert_eq!(provider.provider_name(), "OpenAI");
        assert_eq!(provider.default_model(), "gpt-4");
    }
}
