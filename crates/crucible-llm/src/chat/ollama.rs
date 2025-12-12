//! Ollama chat provider implementation

use async_trait::async_trait;
use crucible_core::traits::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessageDelta, FunctionCallDelta, LlmError,
    LlmMessage, LlmResult, MessageRole, TextGenerationProvider, TokenUsage, ToolCall,
    ToolCallDelta,
};
use futures::stream::BoxStream;
use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;

// Re-export for convenience
use crucible_core::traits::{
    ChatCompletionChoice, ChatCompletionChunk, CompletionChunk, CompletionRequest,
    CompletionResponse, ProviderCapabilities, TextModelInfo,
};

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
impl TextGenerationProvider for OllamaChatProvider {
    async fn generate_completion(
        &self,
        _request: CompletionRequest,
    ) -> LlmResult<CompletionResponse> {
        todo!("Ollama text completion not implemented")
    }

    fn generate_completion_stream<'a>(
        &'a self,
        _request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<CompletionChunk>> {
        todo!("Ollama text completion streaming not implemented")
    }

    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse> {
        // Build Ollama API request
        let mut api_request = serde_json::json!({
            "model": self.default_model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Function => "assistant", // Map to assistant
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
                            "name": tool.function.name,
                            "description": tool.function.description,
                            "parameters": tool.function.parameters.clone().unwrap_or(serde_json::json!({})),
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
                        serde_json::to_string(&tc.function.arguments).unwrap_or_default(),
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

        Ok(ChatCompletionResponse {
            choices: vec![ChatCompletionChoice {
                index: 0,
                message,
                finish_reason: ollama_response.done_reason,
                logprobs: None,
            }],
            model: ollama_response.model,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            id: format!("chatcmpl-{}", Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: chrono::Utc::now(),
            system_fingerprint: None,
        })
    }

    fn provider_name(&self) -> &str {
        "Ollama"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn generate_chat_completion_stream<'a>(
        &'a self,
        request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        use async_stream::stream;
        use futures::StreamExt;

        let url = format!("{}/api/chat", self.base_url);

        // Build Ollama API request
        let mut api_request = serde_json::json!({
            "model": self.default_model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Function => "assistant", // Map to assistant
                        MessageRole::Tool => "tool",
                    },
                    "content": m.content.clone(),
                })
            }).collect::<Vec<_>>(),
            "stream": true,
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
                            "name": tool.function.name,
                            "description": tool.function.description,
                            "parameters": tool.function.parameters.clone().unwrap_or(serde_json::json!({})),
                        }
                    })
                })
                .collect();
            api_request["tools"] = serde_json::json!(ollama_tools);
        }

        let client = self.client.clone();
        let timeout = self.timeout;

        Box::pin(stream! {
            let response = client
                .post(&url)
                .json(&api_request)
                .timeout(timeout)
                .send()
                .await;

            match response {
                Ok(res) if res.status().is_success() => {
                    let mut stream = res.bytes_stream();
                    let mut buffer = String::new();
                    let mut index = 0u32;

                    while let Some(chunk_result) = stream.next().await {
                        match chunk_result {
                            Ok(bytes) => {
                                buffer.push_str(&String::from_utf8_lossy(&bytes));

                                // Process complete lines (NDJSON format)
                                while let Some(line_end) = buffer.find('\n') {
                                    let line = buffer[..line_end].trim().to_string();
                                    buffer = buffer[line_end + 1..].to_string();

                                    if line.is_empty() {
                                        continue;
                                    }

                                    match serde_json::from_str::<OllamaStreamResponse>(&line) {
                                        Ok(stream_resp) => {
                                            // Convert tool calls if present
                                            let tool_calls = stream_resp.message.tool_calls.map(|calls| {
                                                calls
                                                    .into_iter()
                                                    .enumerate()
                                                    .map(|(idx, tc)| ToolCallDelta {
                                                        index: idx as u32,
                                                        id: tc.id,
                                                        function: Some(FunctionCallDelta {
                                                            name: Some(tc.function.name),
                                                            arguments: Some(serde_json::to_string(&tc.function.arguments).unwrap_or_default()),
                                                        }),
                                                    })
                                                    .collect()
                                            });

                                            let chunk = ChatCompletionChunk {
                                                index,
                                                delta: ChatMessageDelta {
                                                    role: stream_resp.message.role.map(|r| match r.as_str() {
                                                        "assistant" => MessageRole::Assistant,
                                                        "system" => MessageRole::System,
                                                        "user" => MessageRole::User,
                                                        _ => MessageRole::Assistant,
                                                    }),
                                                    content: if stream_resp.message.content.is_empty() {
                                                        None
                                                    } else {
                                                        Some(stream_resp.message.content)
                                                    },
                                                    function_call: None,
                                                    tool_calls,
                                                },
                                                finish_reason: if stream_resp.done {
                                                    stream_resp.done_reason.or(Some("stop".to_string()))
                                                } else {
                                                    None
                                                },
                                                logprobs: None,
                                            };
                                            index += 1;
                                            yield Ok(chunk);
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
                        "Ollama API error ({}): {}",
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
        todo!("Ollama model listing not implemented")
    }

    async fn health_check(&self) -> LlmResult<bool> {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(_) => Ok(false),
        }
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
            max_batch_size: Some(1),
            input_formats: vec!["text".to_string()],
            output_formats: vec!["text".to_string()],
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
    done_reason: Option<String>,
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

// Ollama streaming response types
#[derive(Debug, Deserialize)]
struct OllamaStreamResponse {
    model: String,
    message: OllamaStreamMessage,
    done: bool,
    #[serde(default)]
    done_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OllamaStreamMessage {
    role: Option<String>,
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaToolCall>>,
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
