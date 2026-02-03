//! Completion backend trait for LLM integration
//!
//! This module defines the abstraction layer between Crucible's context management
//! and LLM providers. The trait is implemented by adapters (e.g., Rig) so the
//! underlying LLM library can be swapped without changing context management.
//!
//! ## Design Rationale
//!
//! - **No Rig types leak**: Everything goes through our own request/response types
//! - **Streaming-first**: Primary interface is streaming, non-streaming is convenience
//! - **Thin adapter**: Implementations just convert types and forward calls

use crate::traits::context_ops::ContextMessage;
use crate::traits::llm::{LlmToolDefinition, TokenUsage, ToolCall};
use async_trait::async_trait;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};

/// Request to send to a completion backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCompletionRequest {
    /// System prompt
    pub system_prompt: String,
    /// Conversation messages
    pub messages: Vec<ContextMessage>,
    /// Available tools
    pub tools: Vec<LlmToolDefinition>,
    /// Temperature for generation (0.0-2.0)
    pub temperature: Option<f64>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u64>,
}

impl BackendCompletionRequest {
    /// Create a new completion request
    pub fn new(system_prompt: impl Into<String>, messages: Vec<ContextMessage>) -> Self {
        Self {
            system_prompt: system_prompt.into(),
            messages,
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
        }
    }

    /// Add tools to the request
    pub fn with_tools(mut self, tools: Vec<LlmToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f64) -> Self {
        self.temperature = Some(temp.clamp(0.0, 2.0));
        self
    }

    /// Set max tokens
    pub fn with_max_tokens(mut self, tokens: u64) -> Self {
        self.max_tokens = Some(tokens);
        self
    }
}

/// A chunk from a streaming completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCompletionChunk {
    /// Text delta (if any)
    pub delta: Option<String>,
    /// Tool calls (accumulated)
    pub tool_calls: Vec<ToolCall>,
    /// Whether this is the final chunk
    pub done: bool,
    /// Token usage (usually only on final chunk)
    pub usage: Option<TokenUsage>,
}

impl BackendCompletionChunk {
    /// Create a text delta chunk
    pub fn text(delta: impl Into<String>) -> Self {
        Self {
            delta: Some(delta.into()),
            tool_calls: Vec::new(),
            done: false,
            usage: None,
        }
    }

    /// Create a tool call chunk
    pub fn tool_call(call: ToolCall) -> Self {
        Self {
            delta: None,
            tool_calls: vec![call],
            done: false,
            usage: None,
        }
    }

    /// Create a final chunk
    pub fn finished(usage: Option<TokenUsage>) -> Self {
        Self {
            delta: None,
            tool_calls: Vec::new(),
            done: true,
            usage,
        }
    }
}

/// Complete response from a non-streaming completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCompletionResponse {
    /// Generated text content
    pub content: String,
    /// Tool calls (if any)
    pub tool_calls: Vec<ToolCall>,
    /// Token usage
    pub usage: Option<TokenUsage>,
}

/// Errors from the completion backend
#[derive(Debug, Clone, thiserror::Error)]
pub enum BackendError {
    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Rate limit exceeded, retry after {retry_after_secs}s")]
    RateLimit { retry_after_secs: u64 },

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Timeout after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("Internal error: {0}")]
    Internal(String),
}

impl BackendError {
    /// Check if the error is retryable (transient failure).
    ///
    /// Matches the pattern established by `EmbeddingError::is_retryable()`.
    pub fn is_retryable(&self) -> bool {
        match self {
            BackendError::Http(_) => true,
            BackendError::RateLimit { .. } => true,
            BackendError::Timeout { .. } => true,
            BackendError::InvalidResponse(_) => false,
            BackendError::Authentication(_) => false,
            BackendError::ModelNotFound(_) => false,
            BackendError::Provider(_) => false,
            BackendError::Internal(_) => false,
        }
    }

    /// Get the recommended retry delay in seconds.
    ///
    /// Returns `None` for non-retryable errors.
    pub fn retry_delay_secs(&self) -> Option<u64> {
        match self {
            BackendError::RateLimit { retry_after_secs } => Some(*retry_after_secs),
            BackendError::Http(_) => Some(1),
            BackendError::Timeout { .. } => Some(2),
            _ => None,
        }
    }
}

/// Result type for backend operations
pub type BackendResult<T> = Result<T, BackendError>;

/// Trait for completion backends (Rig, direct API calls, etc.)
///
/// Implementations convert Crucible's request types to provider-specific
/// formats, make the LLM call, and convert responses back.
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync for concurrent usage.
#[async_trait]
pub trait CompletionBackend: Send + Sync {
    /// Stream a completion
    ///
    /// Returns a stream of chunks. The final chunk has `done: true`.
    fn complete_stream(
        &self,
        request: BackendCompletionRequest,
    ) -> BoxStream<'static, BackendResult<BackendCompletionChunk>>;

    /// Non-streaming completion (convenience method)
    ///
    /// Default implementation collects the stream.
    async fn complete(
        &self,
        request: BackendCompletionRequest,
    ) -> BackendResult<BackendCompletionResponse> {
        use futures::StreamExt;

        let mut stream = self.complete_stream(request);
        let mut content = String::new();
        let mut tool_calls = Vec::new();
        let mut usage = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            if let Some(delta) = chunk.delta {
                content.push_str(&delta);
            }
            tool_calls.extend(chunk.tool_calls);
            if chunk.usage.is_some() {
                usage = chunk.usage;
            }
        }

        Ok(BackendCompletionResponse {
            content,
            tool_calls,
            usage,
        })
    }

    /// Get the provider name (e.g., "rig-ollama", "rig-openai")
    fn provider_name(&self) -> &str;

    /// Get the model name
    fn model_name(&self) -> &str;

    /// Check if the backend is healthy
    async fn health_check(&self) -> BackendResult<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder() {
        let request = BackendCompletionRequest::new("You are helpful", vec![])
            .with_temperature(0.7)
            .with_max_tokens(1000);

        assert_eq!(request.system_prompt, "You are helpful");
        assert_eq!(request.temperature, Some(0.7));
        assert_eq!(request.max_tokens, Some(1000));
    }

    #[test]
    fn test_chunk_builders() {
        let text_chunk = BackendCompletionChunk::text("Hello");
        assert_eq!(text_chunk.delta, Some("Hello".to_string()));
        assert!(!text_chunk.done);

        let done_chunk = BackendCompletionChunk::finished(None);
        assert!(done_chunk.done);
    }

    // ─────────────────────────────────────────────────────────────────────
    // BackendError contract tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_http_error() {
        let err = BackendError::Http("connection refused".to_string());
        let msg = err.to_string();
        assert!(msg.contains("HTTP"));
        assert!(msg.contains("connection refused"));
    }

    #[test]
    fn test_rate_limit_error() {
        let err = BackendError::RateLimit {
            retry_after_secs: 30,
        };
        let msg = err.to_string();
        assert!(msg.contains("30"));
        assert!(msg.contains("retry"));
    }

    #[test]
    fn test_model_not_found_error() {
        let err = BackendError::ModelNotFound("gpt-5".to_string());
        let msg = err.to_string();
        assert!(msg.contains("gpt-5"));
    }

    #[test]
    fn test_timeout_error() {
        let err = BackendError::Timeout { timeout_secs: 60 };
        let msg = err.to_string();
        assert!(msg.contains("60"));
        assert!(msg.contains("Timeout"));
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BackendError>();
    }

    #[test]
    fn test_error_is_clone() {
        let err = BackendError::Internal("test".into());
        let cloned = err.clone();
        assert_eq!(err.to_string(), cloned.to_string());
    }

    #[test]
    fn test_backend_result_type() {
        let ok: BackendResult<i32> = Ok(42);
        assert!(matches!(ok, Ok(42)));

        let err: BackendResult<i32> = Err(BackendError::Internal("test".into()));
        assert!(err.is_err());
    }

    #[test]
    fn test_retryable_errors() {
        assert!(BackendError::Http("connection refused".into()).is_retryable());
        assert!(BackendError::RateLimit {
            retry_after_secs: 30
        }
        .is_retryable());
        assert!(BackendError::Timeout { timeout_secs: 60 }.is_retryable());
    }

    #[test]
    fn test_non_retryable_errors() {
        assert!(!BackendError::InvalidResponse("bad json".into()).is_retryable());
        assert!(!BackendError::Authentication("bad key".into()).is_retryable());
        assert!(!BackendError::ModelNotFound("gpt-5".into()).is_retryable());
        assert!(!BackendError::Provider("unknown".into()).is_retryable());
        assert!(!BackendError::Internal("bug".into()).is_retryable());
    }

    #[test]
    fn test_retry_delay_secs() {
        assert_eq!(
            BackendError::RateLimit {
                retry_after_secs: 30
            }
            .retry_delay_secs(),
            Some(30)
        );
        assert_eq!(BackendError::Http("err".into()).retry_delay_secs(), Some(1));
        assert_eq!(
            BackendError::Timeout { timeout_secs: 60 }.retry_delay_secs(),
            Some(2)
        );
        assert_eq!(
            BackendError::Authentication("bad".into()).retry_delay_secs(),
            None
        );
        assert_eq!(
            BackendError::Internal("bug".into()).retry_delay_secs(),
            None
        );
    }
}
