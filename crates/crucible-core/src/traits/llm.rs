//! LLM (Large Language Model) abstraction traits
//!
//! This module defines the core abstractions for LLM integration following
//! SOLID principles and dependency inversion.
//!
//! ## Architecture Pattern
//!
//! Following SOLID principles (Interface Segregation & Dependency Inversion):
//! - **crucible-core** defines traits and associated types (this module)
//! - **crucible-llm** implements provider-specific logic (Ollama, OpenAI, etc.)
//! - **crucible-cli** provides glue code and configuration
//!
//! ## Design Principles
//!
//! **Interface Segregation**: Separate traits for distinct capabilities
//! - `TextGenerationProvider` - Chat completion with streaming and tool calling
//! - `CompletionProvider` - Text completion (future)
//! - `EmbeddingProvider` - Text embeddings (already exists in enrichment)
//!
//! **Dependency Inversion**: Traits use associated types for flexibility
//! - Implementations choose concrete types (Message, ToolCall, etc.)
//! - Core never depends on concrete implementations

use super::context_ops::ContextMessage;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result type for LLM operations
pub type LlmResult<T> = Result<T, LlmError>;

/// LLM operation errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Authentication failed: {0}")]
    AuthenticationError(String),

    #[error("Rate limit exceeded, retry after {retry_after_secs}s")]
    RateLimitExceeded { retry_after_secs: u64 },

    #[error("Provider error: {provider}: {message}")]
    ProviderError { provider: String, message: String },

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Request timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Invalid tool call: {0}")]
    InvalidToolCall(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

/// Message role in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// System message (sets behavior)
    System,
    /// User message (input)
    User,
    /// Assistant message (response)
    Assistant,
    /// Function result message (legacy, prefer Tool)
    Function,
    /// Tool result message
    Tool,
}

/// Tool call made by the assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Tool type (typically "function")
    pub r#type: String,
    /// Function call details
    pub function: FunctionCall,
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: String) -> Self {
        Self {
            id: id.into(),
            r#type: "function".to_string(),
            function: FunctionCall {
                name: name.into(),
                arguments,
            },
        }
    }
}

/// Function call details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Function arguments (JSON string)
    pub arguments: String,
}

/// Tool definition for LLM tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolDefinition {
    /// Tool type (typically "function")
    pub r#type: String,
    /// Function definition
    pub function: FunctionDefinition,
}

impl LlmToolDefinition {
    /// Create a new tool definition
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: name.into(),
                description: description.into(),
                parameters: Some(parameters),
            },
        }
    }
}

impl From<super::tools::ToolDefinition> for LlmToolDefinition {
    fn from(tool: super::tools::ToolDefinition) -> Self {
        Self {
            r#type: "function".to_string(),
            function: FunctionDefinition {
                name: tool.name,
                description: tool.description,
                parameters: tool.parameters,
            },
        }
    }
}

/// Function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Function parameters schema (JSON Schema)
    pub parameters: Option<serde_json::Value>,
}

/// Tool choice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Auto mode (model decides)
    Auto,
    /// Required mode (must use a tool)
    Required,
    /// No tools
    None,
    /// Specific tool to use
    Specific {
        /// Tool type (typically "function")
        r#type: String,
        /// Function to use
        function: FunctionDefinition,
    },
}

/// Chat completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    /// Model to use
    pub model: String,
    /// Conversation messages
    pub messages: Vec<ContextMessage>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature for generation (0.0-2.0)
    pub temperature: Option<f32>,
    /// Top p for nucleus sampling (0.0-1.0)
    pub top_p: Option<f32>,
    /// System prompt (alternative to system message)
    pub system: Option<String>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
    /// Frequency penalty (-2.0 to 2.0)
    pub frequency_penalty: Option<f32>,
    /// Presence penalty (-2.0 to 2.0)
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
    pub tools: Option<Vec<LlmToolDefinition>>,
    /// Function calling configuration (legacy, prefer tools)
    pub functions: Option<Vec<FunctionDefinition>>,
    /// Function call behavior (legacy, prefer tool_choice)
    pub function_call: Option<FunctionCallBehavior>,
}

impl ChatCompletionRequest {
    /// Create a new chat completion request
    pub fn new(model: String, messages: Vec<ContextMessage>) -> Self {
        Self {
            model,
            messages,
            max_tokens: None,
            temperature: None,
            top_p: None,
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
            functions: None,
            function_call: None,
        }
    }

    /// Add a system message
    pub fn with_system(mut self, system: String) -> Self {
        self.system = Some(system);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp.clamp(0.0, 2.0));
        self
    }

    /// Set tools
    pub fn with_tools(mut self, tools: Vec<LlmToolDefinition>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set tool choice
    pub fn with_tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = Some(choice);
        self
    }
}

/// Response format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFormat {
    /// Response format type (e.g., "text", "json_object")
    pub r#type: String,
}

/// Function call behavior (legacy)
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
    pub message: ContextMessage,
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

/// Completion request (non-chat)
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
    pub fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp.clamp(0.0, 2.0));
        self
    }

    /// Set top p
    pub fn with_top_p(mut self, top_p: f32) -> Self {
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// Text generation provider trait
///
/// This trait defines the interface for text generation providers that can
/// generate completions and chat completions with streaming support.
///
/// ## Design Rationale
///
/// - **Streaming-first**: Primary interface is streaming, non-streaming is convenience
/// - **Tool calling**: Built-in support for tool/function calling
/// - **Provider-agnostic**: Types work across OpenAI, Ollama, Anthropic, etc.
/// - **Async**: All operations are async for I/O efficiency
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync to enable concurrent usage.
#[async_trait]
pub trait TextGenerationProvider: Send + Sync {
    /// Generate a text completion
    async fn generate_completion(
        &self,
        request: CompletionRequest,
    ) -> LlmResult<CompletionResponse>;

    /// Generate a streaming text completion
    fn generate_completion_stream<'a>(
        &'a self,
        request: CompletionRequest,
    ) -> BoxStream<'a, LlmResult<CompletionChunk>>;

    /// Generate a chat completion
    async fn generate_chat_completion(
        &self,
        request: ChatCompletionRequest,
    ) -> LlmResult<ChatCompletionResponse>;

    /// Generate a streaming chat completion
    fn generate_chat_completion_stream<'a>(
        &'a self,
        request: ChatCompletionRequest,
    ) -> BoxStream<'a, LlmResult<ChatCompletionChunk>>;

    /// Get the provider name
    fn provider_name(&self) -> &str;

    /// Get the default model name
    fn default_model(&self) -> &str;

    /// List available models
    async fn list_models(&self) -> LlmResult<Vec<TextModelInfo>>;

    /// Check if the provider is healthy
    async fn health_check(&self) -> LlmResult<bool>;

    /// Get provider capabilities
    fn capabilities(&self) -> ProviderCapabilities;
}

// Legacy type aliases for backwards compatibility
// Note: These are type aliases for the request/response types only.
// For trait bounds, use TextGenerationProvider directly.

/// Simplified request type (legacy compatibility)
pub type LlmRequest = ChatCompletionRequest;

/// Simplified response type (legacy compatibility)
pub type LlmResponse = ChatCompletionResponse;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_call() {
        let call = ToolCall::new("call_1", "search", r#"{"query": "rust"}"#.to_string());
        assert_eq!(call.id, "call_1");
        assert_eq!(call.function.name, "search");
    }

    #[test]
    fn test_chat_completion_request_builder() {
        let request =
            ChatCompletionRequest::new("gpt-4".to_string(), vec![ContextMessage::user("Hello")])
                .with_max_tokens(100)
                .with_temperature(0.7);

        assert_eq!(request.max_tokens, Some(100));
        assert_eq!(request.temperature, Some(0.7));
    }

    #[test]
    fn test_completion_request_builder() {
        let request = CompletionRequest::new("gpt-3.5-turbo".to_string(), "Hello".to_string())
            .with_max_tokens(50)
            .with_temperature(0.5);

        assert_eq!(request.max_tokens, Some(50));
        assert_eq!(request.temperature, Some(0.5));
    }

    #[test]
    fn test_llm_error_display() {
        let err = LlmError::Timeout { timeout_secs: 30 };
        assert!(err.to_string().contains("30"));

        let err2 = LlmError::RateLimitExceeded {
            retry_after_secs: 60,
        };
        assert!(err2.to_string().contains("60"));
    }

    #[test]
    fn test_tool_definition() {
        let tool = LlmToolDefinition::new(
            "search",
            "Search for information",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }),
        );
        assert_eq!(tool.function.name, "search");
        assert_eq!(tool.r#type, "function");
    }

    #[test]
    fn test_llm_tool_definition_from_tool_definition() {
        use crate::traits::tools::ToolDefinition;

        let tool_def = ToolDefinition::new("read_file", "Read contents of a file").with_parameters(
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
        );

        let llm_tool: LlmToolDefinition = tool_def.into();

        assert_eq!(llm_tool.r#type, "function");
        assert_eq!(llm_tool.function.name, "read_file");
        assert_eq!(llm_tool.function.description, "Read contents of a file");
        assert!(llm_tool.function.parameters.is_some());
    }
}
