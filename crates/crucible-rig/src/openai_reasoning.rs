//! OpenAI-compatible SSE parsing with reasoning_content support
//!
//! This module provides SSE streaming that extracts the non-standard
//! `reasoning_content` field from OpenAI-compatible responses (Ollama, llama.cpp).

use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use tracing::debug;

/// A streaming chunk from an OpenAI-compatible API with reasoning support
#[derive(Debug, Clone)]
pub enum ReasoningChunk {
    /// Regular text content
    Text(String),
    /// Reasoning/thinking content (from reasoning_content field)
    Reasoning(String),
    /// Tool call
    ToolCall {
        /// Tool call ID for correlation with results
        id: String,
        /// Function/tool name
        name: String,
        /// JSON arguments string
        arguments: String,
    },
    /// Stream finished
    Done,
}

/// Delta content from SSE
#[derive(Debug, Deserialize, Default)]
struct Delta {
    content: Option<String>,
    reasoning_content: Option<String>,
    #[allow(dead_code)]
    role: Option<String>,
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDelta {
    #[allow(dead_code)]
    index: Option<usize>,
    id: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    call_type: Option<String>,
    function: Option<FunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct FunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    delta: Delta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamResponse {
    choices: Vec<Choice>,
}

/// Options for streaming with reasoning support
#[derive(Debug, Clone, Default)]
pub struct ReasoningOptions {
    /// Tool definitions for function calling
    pub tools: Option<Vec<serde_json::Value>>,
    /// Thinking budget for reasoning models:
    /// - None: Use model's default
    /// - Some(-1): Unlimited thinking tokens (maps to very large number)
    /// - Some(0): Disable thinking (not all models support this)
    /// - Some(n) where n > 0: Max thinking tokens
    pub thinking_budget: Option<i64>,
}

/// Stream completions with reasoning_content support
///
/// This bypasses Rig's streaming to directly parse SSE events,
/// extracting the non-standard `reasoning_content` field.
pub fn stream_with_reasoning(
    client: Client,
    endpoint: &str,
    model: &str,
    messages: Vec<serde_json::Value>,
    options: ReasoningOptions,
) -> BoxStream<'static, Result<ReasoningChunk, String>> {
    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let model = model.to_string();

    Box::pin(async_stream::stream! {
        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
        });

        if let Some(tools) = options.tools {
            body["tools"] = serde_json::Value::Array(tools);
        }

        // max_completion_tokens is supported by OpenAI o1/o3, Ollama, and DeepSeek R1
        if let Some(budget) = options.thinking_budget {
            let effective_budget = if budget == -1 { 1_000_000i64 } else { budget };
            if effective_budget > 0 {
                body["max_completion_tokens"] = serde_json::json!(effective_budget);
            }
        }

        let response = match client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                yield Err(format!("Request failed: {}", e));
                return;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            yield Err(format!("HTTP {}: {}", status, text));
            return;
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        // Track tool call state for accumulation
        let mut current_tool_id: Option<String> = None;
        let mut current_tool_name: Option<String> = None;
        let mut current_tool_args = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(c) => c,
                Err(e) => {
                    yield Err(format!("Stream error: {}", e));
                    return;
                }
            };

            buffer.push_str(&String::from_utf8_lossy(&chunk));

            // Process complete SSE lines
            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() || line == "data: [DONE]" {
                    if line == "data: [DONE]" {
                        // Emit any pending tool call
                        if let (Some(id), Some(name)) = (current_tool_id.take(), current_tool_name.take()) {
                            yield Ok(ReasoningChunk::ToolCall {
                                id,
                                name,
                                arguments: std::mem::take(&mut current_tool_args),
                            });
                        }
                        yield Ok(ReasoningChunk::Done);
                        return;
                    }
                    continue;
                }

                if !line.starts_with("data: ") {
                    continue;
                }

                let json_str = &line[6..];
                let response: StreamResponse = match serde_json::from_str(json_str) {
                    Ok(r) => r,
                    Err(e) => {
                        debug!(error = %e, json = %json_str, "Failed to parse SSE chunk");
                        continue;
                    }
                };

                for choice in response.choices {
                    // Handle finish_reason
                    if choice.finish_reason.is_some() {
                        // Emit any pending tool call
                        if let (Some(id), Some(name)) = (current_tool_id.take(), current_tool_name.take()) {
                            yield Ok(ReasoningChunk::ToolCall {
                                id,
                                name,
                                arguments: std::mem::take(&mut current_tool_args),
                            });
                        }
                    }

                    // Handle reasoning_content (non-standard Ollama/llama.cpp field)
                    if let Some(reasoning) = choice.delta.reasoning_content {
                        if !reasoning.is_empty() {
                            yield Ok(ReasoningChunk::Reasoning(reasoning));
                        }
                    }

                    // Handle regular content
                    if let Some(content) = choice.delta.content {
                        if !content.is_empty() {
                            yield Ok(ReasoningChunk::Text(content));
                        }
                    }

                    // Handle tool calls (accumulated across chunks)
                    if let Some(tool_calls) = choice.delta.tool_calls {
                        for tc in tool_calls {
                            // New tool call starting
                            if let Some(id) = tc.id {
                                // Emit previous tool call if any
                                if let (Some(prev_id), Some(prev_name)) = (current_tool_id.take(), current_tool_name.take()) {
                                    yield Ok(ReasoningChunk::ToolCall {
                                        id: prev_id,
                                        name: prev_name,
                                        arguments: std::mem::take(&mut current_tool_args),
                                    });
                                }
                                current_tool_id = Some(id);
                            }

                            if let Some(func) = tc.function {
                                if let Some(name) = func.name {
                                    current_tool_name = Some(name);
                                }
                                if let Some(args) = func.arguments {
                                    current_tool_args.push_str(&args);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Emit any remaining tool call
        if let (Some(id), Some(name)) = (current_tool_id, current_tool_name) {
            yield Ok(ReasoningChunk::ToolCall {
                id,
                name,
                arguments: current_tool_args,
            });
        }

        yield Ok(ReasoningChunk::Done);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reasoning_delta() {
        let json = r#"{"choices":[{"finish_reason":null,"index":0,"delta":{"reasoning_content":"thinking..."}}]}"#;
        let response: StreamResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.choices[0].delta.reasoning_content,
            Some("thinking...".to_string())
        );
    }

    #[test]
    fn test_parse_content_delta() {
        let json = r#"{"choices":[{"finish_reason":null,"index":0,"delta":{"content":"hello"}}]}"#;
        let response: StreamResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.choices[0].delta.content, Some("hello".to_string()));
    }

    #[test]
    fn test_parse_tool_call_delta() {
        let json = r#"{"choices":[{"finish_reason":null,"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"read_file","arguments":""}}]}}]}"#;
        let response: StreamResponse = serde_json::from_str(json).unwrap();
        let tc = &response.choices[0].delta.tool_calls.as_ref().unwrap()[0];
        assert_eq!(tc.id, Some("call_123".to_string()));
        assert_eq!(
            tc.function.as_ref().unwrap().name,
            Some("read_file".to_string())
        );
    }
}
