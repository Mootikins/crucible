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
//! - `ChatProvider` - Chat completion with tool calling
//! - `CompletionProvider` - Text completion
//! - `EmbeddingProvider` - Text embeddings (already exists in enrichment)
//!
//! **Dependency Inversion**: Traits use associated types for flexibility
//! - Implementations choose concrete types (Message, ToolCall, etc.)
//! - Core never depends on concrete implementations

use async_trait::async_trait;
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
    /// Tool result message
    Tool,
}

/// LLM message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Tool calls made by assistant (if any)
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Tool call ID (for tool result messages)
    pub tool_call_id: Option<String>,
}

impl LlmMessage {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    /// Create a tool result message
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_calls: Some(tool_calls),
            tool_call_id: None,
        }
    }
}

/// Tool call made by the assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this tool call
    pub id: String,
    /// Tool name
    pub name: String,
    /// Tool parameters (JSON)
    pub parameters: serde_json::Value,
}

impl ToolCall {
    /// Create a new tool call
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            parameters,
        }
    }
}

/// Tool definition for LLM tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Parameter schema (JSON Schema)
    pub parameters: serde_json::Value,
}

impl LlmToolDefinition {
    /// Create a new tool definition
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }
}

/// LLM completion request
#[derive(Debug, Clone)]
pub struct LlmRequest {
    /// Conversation messages
    pub messages: Vec<LlmMessage>,
    /// Available tools (optional)
    pub tools: Option<Vec<LlmToolDefinition>>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Temperature for generation (0.0-2.0)
    pub temperature: Option<f32>,
}

impl LlmRequest {
    /// Create a new request
    pub fn new(messages: Vec<LlmMessage>) -> Self {
        Self {
            messages,
            tools: None,
            max_tokens: None,
            temperature: None,
        }
    }

    /// Set available tools
    pub fn with_tools(mut self, tools: Vec<LlmToolDefinition>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }
}

/// LLM completion response
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// Assistant's message
    pub message: LlmMessage,
    /// Token usage information
    pub usage: TokenUsage,
    /// Model used
    pub model: String,
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

/// LLM provider abstraction
///
/// This trait defines the interface for LLM completion with tool calling support.
/// Implementations (Ollama, OpenAI, Anthropic) provide the concrete logic.
///
/// ## Design Rationale
///
/// - **Minimal interface**: Only completion, not streaming (separate trait)
/// - **Tool calling**: Built-in support for function calling
/// - **Async**: All operations are async for I/O efficiency
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync to enable concurrent usage.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Generate a completion
    ///
    /// # Arguments
    ///
    /// * `request` - The request with messages and optional tools
    ///
    /// # Returns
    ///
    /// Returns the response with the assistant's message and tool calls.
    ///
    /// # Errors
    ///
    /// - `LlmError::HttpError` - Network error
    /// - `LlmError::InvalidResponse` - Invalid response from provider
    /// - `LlmError::RateLimitExceeded` - Rate limit hit
    /// - `LlmError::Timeout` - Request timed out
    async fn complete(&self, request: LlmRequest) -> LlmResult<LlmResponse>;

    /// Get the provider name
    fn provider_name(&self) -> &str;

    /// Get the default model name
    fn default_model(&self) -> &str;

    /// Check if the provider is healthy
    async fn health_check(&self) -> LlmResult<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_message_builders() {
        let user_msg = LlmMessage::user("Hello");
        assert_eq!(user_msg.role, MessageRole::User);
        assert_eq!(user_msg.content, "Hello");

        let assistant_msg = LlmMessage::assistant("Hi there");
        assert_eq!(assistant_msg.role, MessageRole::Assistant);

        let system_msg = LlmMessage::system("You are helpful");
        assert_eq!(system_msg.role, MessageRole::System);

        let tool_msg = LlmMessage::tool("call_123", "result");
        assert_eq!(tool_msg.role, MessageRole::Tool);
        assert_eq!(tool_msg.tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_tool_call() {
        let call = ToolCall::new("call_1", "search", serde_json::json!({"query": "rust"}));
        assert_eq!(call.id, "call_1");
        assert_eq!(call.name, "search");
    }

    #[test]
    fn test_llm_request_builder() {
        let request = LlmRequest::new(vec![LlmMessage::user("Hello")])
            .with_max_tokens(100)
            .with_temperature(0.7);

        assert_eq!(request.max_tokens, Some(100));
        assert_eq!(request.temperature, Some(0.7));
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
}
