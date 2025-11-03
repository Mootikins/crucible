//! Agent provider abstraction trait
//!
//! This trait defines the interface for agent providers (Claude, GPT-4, local models, etc.)
//! Currently a placeholder for future Phase 2 implementation.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result type for agent operations
pub type AgentResult<T> = Result<T, AgentError>;

/// Agent operation errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum AgentError {
    #[error("Provider not available: {0}")]
    ProviderUnavailable(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Agent provider abstraction (placeholder)
///
/// This trait will define the interface for interacting with LLM agents.
/// Currently a placeholder for future implementation.
///
/// ## Future Design
///
/// The trait will likely include:
/// - `execute_task()` - Execute an agent task
/// - `stream_response()` - Stream agent responses
/// - `get_capabilities()` - Query agent capabilities
/// - `create_session()` - Create a conversation session
///
/// ## Thread Safety
///
/// Implementations must be Send + Sync for concurrent agent execution.
#[async_trait]
pub trait AgentProvider: Send + Sync {
    /// Get the agent provider name
    fn name(&self) -> &str;

    /// Get the agent provider version/model
    fn version(&self) -> &str;

    /// Check if the provider is available
    async fn is_available(&self) -> bool;
}

/// Agent capabilities metadata (placeholder)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    /// Provider name (e.g., "claude", "gpt-4", "local")
    pub provider: String,

    /// Model name/version
    pub model: String,

    /// Maximum context window size
    pub max_context_tokens: Option<u32>,

    /// Supports streaming responses
    pub supports_streaming: bool,

    /// Supports function calling
    pub supports_functions: bool,

    /// Supports vision/image understanding
    pub supports_vision: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_capabilities() {
        let caps = AgentCapabilities {
            provider: "claude".to_string(),
            model: "claude-3-5-sonnet".to_string(),
            max_context_tokens: Some(200_000),
            supports_streaming: true,
            supports_functions: true,
            supports_vision: true,
        };

        assert_eq!(caps.provider, "claude");
        assert!(caps.supports_streaming);
    }
}
