//! # Text Generation Providers
//!
//! This module provides text generation providers for various LLM services including
//! OpenAI, Ollama, and other providers. It implements a common trait for text generation
//! operations including completion and chat completion.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

use super::embeddings::error::EmbeddingResult;

/// Text generation provider trait
///
/// This trait defines the interface for text generation providers that can
/// generate completions and chat completions.
#[async_trait]
pub trait TextGenerationProvider: Send + Sync {
    /// Configuration type for this provider
    type Config: Clone + Send + Sync;

    /// Generate a text completion
    async fn generate_completion(&self, request: CompletionRequest) -> EmbeddingResult<CompletionResponse>;

    /// Generate a streaming text completion
    async fn generate_completion_stream(
        &self,
        request: CompletionRequest
    ) -> EmbeddingResult<tokio::sync::mpsc::UnboundedReceiver<CompletionChunk>>;

    /// Generate a chat completion
    async fn generate_chat_completion(&self, request: ChatCompletionRequest) -> EmbeddingResult<ChatCompletionResponse>;

    /// Generate a streaming chat completion
    async fn generate_chat_completion_stream(
        &self,
        request: ChatCompletionRequest
    ) -> EmbeddingResult<tokio::sync::mpsc::UnboundedReceiver<ChatCompletionChunk>>;

    /// Get the provider name
    fn provider_name(&self) -> &str;

    /// Get the default model name
    fn default_model(&self) -> &str;

    /// List available models
    async fn list_models(&self) -> EmbeddingResult<Vec<TextModelInfo>>;

    /// Check if the provider is healthy
    async fn health_check(&self) -> EmbeddingResult<bool>;

    /// Get provider capabilities
    fn capabilities(&self) -> ProviderCapabilities;
}

/// Completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model to use
    pub model: String,
    /// Prompt text
    pub prompt: String,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature for generation (0.0-2.0)
    pub temperature: Option<f32>,
    /// Top p for nucleus sampling (0.0-1.0)
    pub top_p: Option<f32>,
    /// Frequency penalty (-2.0 to 2.0)
    pub frequency_penalty: Option<f32>,
    /// Presence penalty (-2.0 to 2.0)
    pub presence_penalty: Option<f32>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
    /// Number of completions to generate
    pub n: Option<u32>,
    /// Echo the prompt in the response
    pub echo: Option<bool>,
    /// Logit bias
    pub logit_bias: Option<HashMap<i32, f32>>,
    /// User identifier
    pub user: Option<String>,
}

impl CompletionRequest {
    /// Create a new completion request
    pub fn new(model: String, prompt: String) -> Self {
        Self {
            model,
            prompt,
            max_tokens: None,
            temperature: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            stop: None,
            n: None,
            echo: None,
            logit_bias: None,
            user: None,
        }
    }

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set temperature
    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp.clamp(0.0, 2.0));
        self
    }

    /// Set top p
    pub fn top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p.clamp(0.0, 1.0));
        self
    }
}

/// Completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Generated completions
    pub choices: Vec<CompletionChoice>,
    /// Model used
    pub model: String,
    /// Usage information
    pub usage: TokenUsage,
    /// Request ID
    pub id: String,
    /// Object type
    pub object: String,
    /// Created timestamp
    pub created: DateTime<Utc>,
    /// System fingerprint
    pub system_fingerprint: Option<String>,
}

/// Individual completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChoice {
    /// Completion text
    pub text: String,
    /// Choice index
    pub index: u32,
    /// Log probabilities
    pub logprobs: Option<LogProbs>,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// Completion chunk for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChunk {
    /// Partial text
    pub text: String,
    /// Chunk index
    pub index: u32,
    /// Finish reason if complete
    pub finish_reason: Option<String>,
    /// Log probabilities
    pub logprobs: Option<LogProbs>,
}

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    /// Model to use
    pub model: String,
    /// Conversation messages
    pub messages: Vec<ChatMessage>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Top p for nucleus sampling
    pub top_p: Option<f32>,
    /// Function calling configuration
    pub functions: Option<Vec<FunctionDefinition>>,
    /// Function call behavior
    pub function_call: Option<FunctionCallBehavior>,
    /// System prompt
    pub system: Option<String>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
    /// Frequency penalty
    pub frequency_penalty: Option<f32>,
    /// Presence penalty
    pub presence_penalty: Option<f32>,
    /// Logit bias
    pub logit_bias: Option<HashMap<i32, f32>>,
    /// User identifier
    pub user: Option<String>,
    /// Response format
    pub response_format: Option<ResponseFormat>,
    /// Seed for deterministic generation
    pub seed: Option<i64>,
    /// Tool choice configuration
    pub tool_choice: Option<ToolChoice>,
    /// Available tools
    pub tools: Option<Vec<ToolDefinition>>,
}

impl ChatCompletionRequest {
    /// Create a new chat completion request
    pub fn new(model: String, messages: Vec<ChatMessage>) -> Self {
        Self {
            model,
            messages,
            max_tokens: None,
            temperature: None,
            top_p: None,
            functions: None,
            function_call: None,
            system: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            logit_bias: None,
            user: None,
            response_format: None,
            seed: None,
            tool_choice: None,
            tools: None,
        }
    }

    /// Add a system message
    pub fn with_system(mut self, system: String) -> Self {
        self.system = Some(system);
        self
    }

    /// Set max tokens
    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set temperature
    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp.clamp(0.0, 2.0));
        self
    }
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Function call (if any)
    pub function_call: Option<FunctionCall>,
    /// Tool calls (if any)
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Message name (for function results)
    pub name: Option<String>,
    /// Tool call ID (for tool results)
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    /// Create a user message
    pub fn user(content: String) -> Self {
        Self {
            role: MessageRole::User,
            content,
            function_call: None,
            tool_calls: None,
            name: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: String) -> Self {
        Self {
            role: MessageRole::Assistant,
            content,
            function_call: None,
            tool_calls: None,
            name: None,
            tool_call_id: None,
        }
    }

    /// Create a system message
    pub fn system(content: String) -> Self {
        Self {
            role: MessageRole::System,
            content,
            function_call: None,
            tool_calls: None,
            name: None,
            tool_call_id: None,
        }
    }

    /// Create a function result message
    pub fn function(name: String, content: String) -> Self {
        Self {
            role: MessageRole::Function,
            content,
            function_call: None,
            tool_calls: None,
            name: Some(name),
            tool_call_id: None,
        }
    }

    /// Create a tool result message
    pub fn tool(tool_call_id: String, content: String) -> Self {
        Self {
            role: MessageRole::Tool,
            content,
            function_call: None,
            tool_calls: None,
            name: None,
            tool_call_id: Some(tool_call_id),
        }
    }
}

/// Message role
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    /// System message (sets behavior)
    System,
    /// User message (input)
    User,
    /// Assistant message (response)
    Assistant,
    /// Function result message
    Function,
    /// Tool result message
    Tool,
}

/// Chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    /// Chat message choices
    pub choices: Vec<ChatCompletionChoice>,
    /// Model used
    pub model: String,
    /// Usage information
    pub usage: TokenUsage,
    /// Request ID
    pub id: String,
    /// Object type
    pub object: String,
    /// Created timestamp
    pub created: DateTime<Utc>,
    /// System fingerprint
    pub system_fingerprint: Option<String>,
}

/// Chat completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChoice {
    /// Message index
    pub index: u32,
    /// Chat message
    pub message: ChatMessage,
    /// Finish reason
    pub finish_reason: Option<String>,
    /// Log probabilities
    pub logprobs: Option<LogProbs>,
}

/// Chat completion chunk for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    /// Choice index
    pub index: u32,
    /// Delta message
    pub delta: ChatMessageDelta,
    /// Finish reason
    pub finish_reason: Option<String>,
    /// Log probabilities
    pub logprobs: Option<LogProbs>,
}

/// Chat message delta for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageDelta {
    /// Message role (may be omitted)
    pub role: Option<MessageRole>,
    /// Message content delta
    pub content: Option<String>,
    /// Function call delta
    pub function_call: Option<FunctionCall>,
    /// Tool calls delta
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// Tool call delta for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Tool call index
    pub index: u32,
    /// Tool call ID delta
    pub id: Option<String>,
    /// Function call delta
    pub function: Option<FunctionCallDelta>,
}

/// Function call delta
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallDelta {
    /// Function name delta
    pub name: Option<String>,
    /// Function arguments delta
    pub arguments: Option<String>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Prompt tokens used
    pub prompt_tokens: u32,
    /// Completion tokens used
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
}

/// Log probabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogProbs {
    /// Token probabilities
    pub tokens: Vec<String>,
    /// Log probabilities
    pub token_logprobs: Vec<f32>,
    /// Top log probabilities
    pub top_logprobs: Vec<HashMap<String, f32>>,
    /// Byte offsets
    pub bytes_offset: Vec<u32>,
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Function parameters schema
    pub parameters: Option<serde_json::Value>,
}

/// Function call behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionCallBehavior {
    /// Auto mode (model decides)
    Auto,
    /// Force function call
    Force(String),
    /// No function call
    None,
}

/// Function call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Function arguments (JSON string)
    pub arguments: String,
}

/// Tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID
    pub id: String,
    /// Tool call type
    pub r#type: String,
    /// Function call
    pub function: FunctionCall,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool type (typically "function")
    pub r#type: String,
    /// Function definition
    pub function: FunctionDefinition,
}

/// Tool choice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Auto mode
    Auto,
    /// Required mode
    Required,
    /// No tools
    None,
    /// Specific tool
    Specific {
        /// Tool type (typically "function")
        r#type: String,
        /// Function definition to use
        function: FunctionDefinition
    },
}

/// Response format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    /// Response format type (e.g., "text", "json_object")
    pub r#type: String,
}

/// Text model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextModelInfo {
    /// Model identifier
    pub id: String,
    /// Model name
    pub name: String,
    /// Model owner/creator
    pub owner: Option<String>,
    /// Model capabilities
    pub capabilities: Vec<ModelCapability>,
    /// Maximum context length
    pub max_context_length: Option<u32>,
    /// Maximum output tokens
    pub max_output_tokens: Option<u32>,
    /// Input token pricing
    pub input_price: Option<f64>,
    /// Output token pricing
    pub output_price: Option<f64>,
    /// Model creation date
    pub created: Option<DateTime<Utc>>,
    /// Model status
    pub status: ModelStatus,
}

/// Model capability
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelCapability {
    /// Text completion
    TextCompletion,
    /// Chat completion
    ChatCompletion,
    /// Function calling
    FunctionCalling,
    /// Tool use
    ToolUse,
    /// Vision/image processing
    Vision,
    /// Audio processing
    Audio,
    /// Streaming support
    Streaming,
    /// JSON mode
    JsonMode,
}

/// Model status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelStatus {
    /// Model is available
    Available,
    /// Model is deprecated
    Deprecated,
    /// Model is in beta
    Beta,
    /// Model is unavailable
    Unavailable,
}

/// Provider capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// Supports text completion
    pub text_completion: bool,
    /// Supports chat completion
    pub chat_completion: bool,
    /// Supports streaming
    pub streaming: bool,
    /// Supports function calling
    pub function_calling: bool,
    /// Supports tool use
    pub tool_use: bool,
    /// Supports vision
    pub vision: bool,
    /// Supports audio
    pub audio: bool,
    /// Maximum batch size
    pub max_batch_size: Option<u32>,
    /// Supported input formats
    pub input_formats: Vec<String>,
    /// Supported output formats
    pub output_formats: Vec<String>,
}

/// OpenAI text generation provider
pub struct OpenAITextProvider {
    #[allow(dead_code)]
    client: reqwest::Client,
    #[allow(dead_code)]
    api_key: String,
    #[allow(dead_code)]
    base_url: String,
    default_model: String,
    #[allow(dead_code)]
    timeout: Duration,
}

impl OpenAITextProvider {
    /// Create a new OpenAI text provider
    pub fn new(config: TextProviderConfig) -> Self {
        if let TextProviderConfig::OpenAI(openai_config) = config {
            Self {
                client: reqwest::Client::new(),
                api_key: openai_config.api_key,
                base_url: openai_config.base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                default_model: openai_config.default_model.unwrap_or_else(|| "gpt-3.5-turbo".to_string()),
                timeout: Duration::from_secs(openai_config.timeout_secs.unwrap_or(60)),
            }
        } else {
            panic!("OpenAI provider requires OpenAI configuration");
        }
    }
}

#[async_trait]
impl TextGenerationProvider for OpenAITextProvider {
    type Config = TextProviderConfig;

    async fn generate_completion(&self, _request: CompletionRequest) -> EmbeddingResult<CompletionResponse> {
        // Implementation would go here
        todo!("OpenAI completion implementation")
    }

    async fn generate_completion_stream(
        &self,
        _request: CompletionRequest
    ) -> EmbeddingResult<tokio::sync::mpsc::UnboundedReceiver<CompletionChunk>> {
        // Implementation would go here
        todo!("OpenAI streaming completion implementation")
    }

    async fn generate_chat_completion(&self, _request: ChatCompletionRequest) -> EmbeddingResult<ChatCompletionResponse> {
        // Implementation would go here
        todo!("OpenAI chat completion implementation")
    }

    async fn generate_chat_completion_stream(
        &self,
        _request: ChatCompletionRequest
    ) -> EmbeddingResult<tokio::sync::mpsc::UnboundedReceiver<ChatCompletionChunk>> {
        // Implementation would go here
        todo!("OpenAI streaming chat completion implementation")
    }

    fn provider_name(&self) -> &str {
        "OpenAI"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<TextModelInfo>> {
        // Implementation would go here
        todo!("OpenAI model listing implementation")
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
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
    #[allow(dead_code)]
    client: reqwest::Client,
    #[allow(dead_code)]
    base_url: String,
    default_model: String,
    #[allow(dead_code)]
    timeout: Duration,
}

impl OllamaTextProvider {
    /// Create a new Ollama text provider
    pub fn new(config: TextProviderConfig) -> Self {
        if let TextProviderConfig::Ollama(ollama_config) = config {
            Self {
                client: reqwest::Client::new(),
                base_url: ollama_config.base_url,
                default_model: ollama_config.default_model.unwrap_or_else(|| "llama2".to_string()),
                timeout: Duration::from_secs(ollama_config.timeout_secs.unwrap_or(120)),
            }
        } else {
            panic!("Ollama provider requires Ollama configuration");
        }
    }
}

#[async_trait]
impl TextGenerationProvider for OllamaTextProvider {
    type Config = TextProviderConfig;

    async fn generate_completion(&self, _request: CompletionRequest) -> EmbeddingResult<CompletionResponse> {
        // Implementation would go here
        todo!("Ollama completion implementation")
    }

    async fn generate_completion_stream(
        &self,
        _request: CompletionRequest
    ) -> EmbeddingResult<tokio::sync::mpsc::UnboundedReceiver<CompletionChunk>> {
        // Implementation would go here
        todo!("Ollama streaming completion implementation")
    }

    async fn generate_chat_completion(&self, _request: ChatCompletionRequest) -> EmbeddingResult<ChatCompletionResponse> {
        // Implementation would go here
        todo!("Ollama chat completion implementation")
    }

    async fn generate_chat_completion_stream(
        &self,
        _request: ChatCompletionRequest
    ) -> EmbeddingResult<tokio::sync::mpsc::UnboundedReceiver<ChatCompletionChunk>> {
        // Implementation would go here
        todo!("Ollama streaming chat completion implementation")
    }

    fn provider_name(&self) -> &str {
        "Ollama"
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn list_models(&self) -> EmbeddingResult<Vec<TextModelInfo>> {
        // Implementation would go here
        todo!("Ollama model listing implementation")
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        // Implementation would go here
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
pub async fn create_text_provider(config: TextProviderConfig) -> EmbeddingResult<Box<dyn TextGenerationProvider<Config = TextProviderConfig>>> {
    match config {
        TextProviderConfig::OpenAI(openai_config) => {
            let provider = OpenAITextProvider::new(TextProviderConfig::OpenAI(openai_config.clone()));
            Ok(Box::new(provider))
        }
        TextProviderConfig::Ollama(ollama_config) => {
            let provider = OllamaTextProvider::new(TextProviderConfig::Ollama(ollama_config.clone()));
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