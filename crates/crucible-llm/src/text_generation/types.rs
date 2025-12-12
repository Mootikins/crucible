//! # Text Generation Providers
//!
//! This module provides text generation providers for various LLM services including
//! OpenAI, Ollama, and other providers. It implements the TextGenerationProvider trait
//! defined in crucible-core.

use async_trait::async_trait;
use chrono::Utc;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use crate::embeddings::error::EmbeddingResult;

// Re-export all types from crucible-core for backwards compatibility
pub use crucible_core::traits::{
    ChatCompletionChoice, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessageDelta, CompletionChoice, CompletionChunk, CompletionRequest, CompletionResponse,
    FunctionCall, FunctionCallBehavior, FunctionCallDelta, FunctionDefinition, LlmError,
    LlmMessage, LlmResult, LlmToolDefinition, LogProbs, MessageRole, ModelCapability, ModelStatus,
    ProviderCapabilities, ResponseFormat, TextGenerationProvider, TextModelInfo, TokenUsage,
    ToolCall, ToolCallDelta, ToolChoice,
};

/// OpenAI text generation provider
pub struct OpenAITextProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    default_model: String,
    timeout: Duration,
}

impl OpenAITextProvider {
    /// Create a new OpenAI text provider
    pub fn new(config: TextProviderConfig) -> Self {
        if let TextProviderConfig::OpenAI(openai_config) = config {
            Self {
                client: reqwest::Client::new(),
                api_key: openai_config.api_key,
                base_url: openai_config
                    .base_url
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                default_model: openai_config
                    .default_model
                    .unwrap_or_else(|| "gpt-3.5-turbo".to_string()),
                timeout: Duration::from_secs(openai_config.timeout_secs.unwrap_or(60)),
            }
        } else {
            panic!("OpenAI provider requires OpenAI configuration");
        }
    }
}

#[async_trait]
impl TextGenerationProvider for OpenAITextProvider {
    async fn generate_completion(
        &self,
        _request: CompletionRequest,
    ) -> LlmResult<CompletionResponse> {
        todo!("OpenAI completion implementation")
    }

    fn generate_completion_stream<'a>(
        &'a self,
        _request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<CompletionChunk>> {
        todo!("OpenAI streaming completion implementation")
    }

    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse> {
        // Build OpenAI API request
        let mut api_request = serde_json::json!({
            "model": request.model,
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

                // Add name for function messages
                if m.role == MessageRole::Function {
                    if let Some(name) = &m.name {
                        msg["name"] = serde_json::json!(name);
                    }
                }

                // Add tool_calls for assistant messages
                if let Some(tool_calls) = &m.tool_calls {
                    msg["tool_calls"] = serde_json::json!(tool_calls);
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
        if let Some(top_p) = request.top_p {
            api_request["top_p"] = serde_json::json!(top_p);
        }
        if let Some(stop) = &request.stop {
            api_request["stop"] = serde_json::json!(stop);
        }
        if let Some(frequency_penalty) = request.frequency_penalty {
            api_request["frequency_penalty"] = serde_json::json!(frequency_penalty);
        }
        if let Some(presence_penalty) = request.presence_penalty {
            api_request["presence_penalty"] = serde_json::json!(presence_penalty);
        }

        // Add tool definitions if present
        if let Some(tools) = &request.tools {
            api_request["tools"] = serde_json::json!(tools);
        }

        // Add tool_choice if present
        if let Some(tool_choice) = &request.tool_choice {
            api_request["tool_choice"] =
                serde_json::to_value(tool_choice).unwrap_or(serde_json::json!("auto"));
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

        let api_response: ChatCompletionResponse = response.json().await.map_err(|e| {
            LlmError::InvalidResponse(format!("Failed to parse JSON response: {}", e))
        })?;

        Ok(api_response)
    }

    fn generate_chat_completion_stream<'a>(
        &'a self,
        _request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        todo!("OpenAI streaming chat completion implementation")
    }

    fn provider_name(&self) -> &str {
        "OpenAI"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn list_models(&self) -> LlmResult<Vec<TextModelInfo>> {
        todo!("OpenAI model listing implementation")
    }

    async fn health_check(&self) -> LlmResult<bool> {
        // Simple health check - try to list models
        match self.list_models().await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            text_completion: true,
            chat_completion: true,
            streaming: true,
            function_calling: true,
            tool_use: true,
            vision: true, // GPT-4V
            audio: true,  // Whisper/TTS
            max_batch_size: None,
            input_formats: vec!["text".to_string(), "json".to_string()],
            output_formats: vec!["text".to_string(), "json".to_string()],
        }
    }
}

/// OpenAI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIConfig {
    /// API key
    pub api_key: String,
    /// Base URL
    pub base_url: Option<String>,
    /// Default model
    pub default_model: Option<String>,
    /// Timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Default temperature
    pub temperature: Option<f32>,
    /// Organization ID
    pub organization: Option<String>,
}

/// Ollama text generation provider
pub struct OllamaTextProvider {
    client: reqwest::Client,
    base_url: String,
    default_model: String,
    timeout: Duration,
}

impl OllamaTextProvider {
    /// Create a new Ollama text provider
    pub fn new(config: TextProviderConfig) -> Self {
        if let TextProviderConfig::Ollama(ollama_config) = config {
            Self {
                client: reqwest::Client::new(),
                base_url: ollama_config.base_url,
                default_model: ollama_config
                    .default_model
                    .unwrap_or_else(|| "llama2".to_string()),
                timeout: Duration::from_secs(ollama_config.timeout_secs.unwrap_or(120)),
            }
        } else {
            panic!("Ollama provider requires Ollama configuration");
        }
    }
}

#[async_trait]
impl TextGenerationProvider for OllamaTextProvider {
    async fn generate_completion(
        &self,
        _request: CompletionRequest,
    ) -> LlmResult<CompletionResponse> {
        todo!("Ollama completion implementation")
    }

    fn generate_completion_stream<'a>(
        &'a self,
        _request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<CompletionChunk>> {
        todo!("Ollama streaming completion implementation")
    }

    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse> {
        // Build Ollama chat request
        let mut ollama_request = serde_json::json!({
            "model": request.model,
            "messages": request.messages.iter().map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Function => "assistant", // Map to assistant
                        MessageRole::Tool => "assistant", // Map to assistant
                    },
                    "content": m.content.clone(),
                })
            }).collect::<Vec<_>>(),
            "stream": false,
        });

        // Add optional parameters
        if let Some(temp) = request.temperature {
            ollama_request["options"] = serde_json::json!({
                "temperature": temp,
            });
        }
        if let Some(max_tokens) = request.max_tokens {
            if let Some(options) = ollama_request.get_mut("options") {
                options["num_predict"] = serde_json::json!(max_tokens);
            } else {
                ollama_request["options"] = serde_json::json!({
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
            ollama_request["tools"] = serde_json::json!(ollama_tools);
        }

        // Make request
        let url = format!("{}/api/chat", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&ollama_request)
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

        let ollama_response: serde_json::Value = response.json().await.map_err(|e| {
            LlmError::InvalidResponse(format!("Failed to parse JSON response: {}", e))
        })?;

        // Parse response
        let message_content = ollama_response["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Check for tool calls in response
        let mut tool_calls = None;
        if let Some(tool_call_array) = ollama_response["message"]["tool_calls"].as_array() {
            let parsed_tool_calls: Vec<ToolCall> = tool_call_array
                .iter()
                .enumerate()
                .filter_map(|(idx, tc)| {
                    let function_name = tc["function"]["name"].as_str()?;
                    let function_args = tc["function"]["arguments"].clone();
                    Some(ToolCall {
                        id: format!("call_{}", idx),
                        r#type: "function".to_string(),
                        function: FunctionCall {
                            name: function_name.to_string(),
                            arguments: serde_json::to_string(&function_args).unwrap_or_default(),
                        },
                    })
                })
                .collect();

            if !parsed_tool_calls.is_empty() {
                tool_calls = Some(parsed_tool_calls);
            }
        }

        let message = LlmMessage {
            role: MessageRole::Assistant,
            content: message_content,
            function_call: None,
            tool_calls,
            name: None,
            tool_call_id: None,
        };

        // Extract token usage (Ollama might not provide this)
        let prompt_tokens = ollama_response["prompt_eval_count"].as_u64().unwrap_or(0) as u32;
        let completion_tokens = ollama_response["eval_count"].as_u64().unwrap_or(0) as u32;

        Ok(ChatCompletionResponse {
            choices: vec![ChatCompletionChoice {
                index: 0,
                message,
                finish_reason: Some(
                    ollama_response["done_reason"]
                        .as_str()
                        .unwrap_or("stop")
                        .to_string(),
                ),
                logprobs: None,
            }],
            model: request.model,
            usage: TokenUsage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
            id: format!("chatcmpl-{}", Uuid::new_v4()),
            object: "chat.completion".to_string(),
            created: Utc::now(),
            system_fingerprint: None,
        })
    }

    fn generate_chat_completion_stream<'a>(
        &'a self,
        _request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>> {
        todo!("Ollama streaming chat completion implementation")
    }

    fn provider_name(&self) -> &str {
        "Ollama"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn list_models(&self) -> LlmResult<Vec<TextModelInfo>> {
        todo!("Ollama model listing implementation")
    }

    async fn health_check(&self) -> LlmResult<bool> {
        todo!("Ollama health check implementation")
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            text_completion: true,
            chat_completion: true,
            streaming: true,
            function_calling: false, // Ollama doesn't support this natively
            tool_use: false,
            vision: false,
            audio: false,
            max_batch_size: Some(1), // Ollama typically processes one at a time
            input_formats: vec!["text".to_string()],
            output_formats: vec!["text".to_string()],
        }
    }
}

/// Ollama configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Base URL for Ollama API
    pub base_url: String,
    /// Default model
    pub default_model: Option<String>,
    /// Timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Default temperature
    pub temperature: Option<f32>,
}

/// Factory function to create a text generation provider from configuration
pub async fn create_text_provider(
    config: TextProviderConfig,
) -> EmbeddingResult<Box<dyn TextGenerationProvider>> {
    match config {
        TextProviderConfig::OpenAI(openai_config) => {
            let provider =
                OpenAITextProvider::new(TextProviderConfig::OpenAI(openai_config.clone()));
            Ok(Box::new(provider))
        }
        TextProviderConfig::Ollama(ollama_config) => {
            let provider =
                OllamaTextProvider::new(TextProviderConfig::Ollama(ollama_config.clone()));
            Ok(Box::new(provider))
        }
    }
}

/// Text provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum TextProviderConfig {
    /// OpenAI configuration
    OpenAI(OpenAIConfig),
    /// Ollama configuration
    Ollama(OllamaConfig),
}

impl TextProviderConfig {
    /// Create OpenAI configuration
    pub fn openai(api_key: String) -> Self {
        Self::OpenAI(OpenAIConfig {
            api_key,
            base_url: None,
            default_model: None,
            timeout_secs: None,
            temperature: None,
            organization: None,
        })
    }

    /// Create Ollama configuration
    pub fn ollama(base_url: String) -> Self {
        Self::Ollama(OllamaConfig {
            base_url,
            default_model: None,
            timeout_secs: None,
            temperature: None,
        })
    }
}
