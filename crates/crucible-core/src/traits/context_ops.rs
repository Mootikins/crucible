//! Composable context operations for conversation management
//!
//! This module provides a set of primitives for manipulating conversation context
//! that can be composed by scripts (Rune/Lua) in response to events.
//!
//! ## Design Philosophy
//!
//! Context operations are low-level primitives that scripts orchestrate:
//! - Event bus detects patterns (e.g., "5 failed tool calls")
//! - Scripts subscribe to events and call context primitives
//! - Primitives are simple: take, drop, inject, replace, summarize
//!
//! This separation allows flexible context management without hardcoding policies.

use crate::traits::llm::{MessageRole, ToolCall};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Metadata associated with a context message
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Tool call ID (for tool result messages)
    pub tool_call_id: Option<String>,
    /// Tool calls made by assistant (if any)
    pub tool_calls: Vec<ToolCall>,
    /// Estimated token count for this message
    pub token_estimate: usize,
    /// Unix timestamp when message was added
    pub timestamp: Option<i64>,
    /// Whether this message represents a successful operation
    pub success: Option<bool>,
    /// Custom tags for filtering
    pub tags: Vec<String>,
}

/// A message in the conversation context with rich metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    /// Message role (System, User, Assistant, Tool)
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Associated metadata
    pub metadata: MessageMetadata,
}

impl ContextMessage {
    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        let content = content.into();
        let token_estimate = content.len() / 4;
        Self {
            role: MessageRole::User,
            content,
            metadata: MessageMetadata {
                token_estimate,
                timestamp: Some(chrono::Utc::now().timestamp()),
                ..Default::default()
            },
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        let content = content.into();
        let token_estimate = content.len() / 4;
        Self {
            role: MessageRole::Assistant,
            content,
            metadata: MessageMetadata {
                token_estimate,
                timestamp: Some(chrono::Utc::now().timestamp()),
                ..Default::default()
            },
        }
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        let content = content.into();
        let token_estimate = content.len() / 4;
        Self {
            role: MessageRole::Assistant,
            content,
            metadata: MessageMetadata {
                token_estimate,
                tool_calls,
                timestamp: Some(chrono::Utc::now().timestamp()),
                ..Default::default()
            },
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        let content = content.into();
        let token_estimate = content.len() / 4;
        Self {
            role: MessageRole::System,
            content,
            metadata: MessageMetadata {
                token_estimate,
                timestamp: Some(chrono::Utc::now().timestamp()),
                ..Default::default()
            },
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let token_estimate = content.len() / 4;
        Self {
            role: MessageRole::Tool,
            content,
            metadata: MessageMetadata {
                tool_call_id: Some(tool_call_id.into()),
                token_estimate,
                timestamp: Some(chrono::Utc::now().timestamp()),
                ..Default::default()
            },
        }
    }

    /// Mark this message as successful or failed
    pub fn with_success(mut self, success: bool) -> Self {
        self.metadata.success = Some(success);
        self
    }

    /// Add a tag to this message
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.metadata.tags.push(tag.into());
        self
    }
}

/// Position for context insertions
#[derive(Debug, Clone)]
pub enum Position {
    /// Insert at the start (after system prompt)
    Start,
    /// Insert at the end
    End,
    /// Insert before a specific index
    Before(usize),
    /// Insert after a specific index
    After(usize),
    /// Insert before the last user message (common pattern for KB context)
    BeforeLastUser,
}

/// Range specification for context operations
#[derive(Debug, Clone)]
pub enum Range {
    /// All messages
    All,
    /// Last N messages
    Last(usize),
    /// First N messages
    First(usize),
    /// Specific index range
    Indices(std::ops::Range<usize>),
}

/// Predicate for filtering messages
pub type MessagePredicate = Box<dyn Fn(&ContextMessage) -> bool + Send + Sync>;

/// Composable context operations trait
///
/// Provides primitives for manipulating conversation context that can be
/// composed by scripts in response to events.
#[async_trait]
pub trait ContextOps: Send + Sync {
    // === Selection ===

    /// Get all messages (excluding system prompt)
    fn messages(&self) -> Vec<ContextMessage>;

    /// Get the system prompt if set
    fn system_prompt(&self) -> Option<&str>;

    /// Take messages matching a range (non-destructive copy)
    fn take(&self, range: Range) -> Vec<ContextMessage>;

    /// Find messages matching a predicate
    fn find(&self, predicate: MessagePredicate) -> Vec<(usize, &ContextMessage)>;

    // === Mutation ===

    /// Set the system prompt
    fn set_system_prompt(&mut self, prompt: String);

    /// Drop messages matching a range
    fn drop_range(&mut self, range: Range);

    /// Drop messages matching a predicate
    fn drop_matching(&mut self, predicate: MessagePredicate);

    /// Replace messages in a range with new messages
    fn replace(&mut self, range: Range, messages: Vec<ContextMessage>);

    /// Inject messages at a position
    fn inject(&mut self, position: Position, messages: Vec<ContextMessage>);

    /// Clear all messages (keeps system prompt)
    fn clear(&mut self);

    // === Transformation ===

    /// Summarize messages in a range (async - may call LLM)
    ///
    /// Replaces the specified range with a single summary message.
    /// Returns an error if summarization fails.
    async fn summarize(&mut self, range: Range) -> Result<(), ContextError>;

    // === Budget ===

    /// Estimate current token usage
    fn token_estimate(&self) -> usize;

    /// Trim context to fit within token budget (removes oldest first)
    fn trim_to_budget(&mut self, max_tokens: usize);

    /// Get message count (excluding system prompt)
    fn message_count(&self) -> usize;
}

/// Errors that can occur during context operations
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("Summarization failed: {0}")]
    SummarizationFailed(String),

    #[error("Invalid range: {0}")]
    InvalidRange(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_message_user() {
        let msg = ContextMessage::user("Hello, world!");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello, world!");
        assert!(msg.metadata.timestamp.is_some());
    }

    #[test]
    fn test_context_message_with_tools() {
        let tool_call = ToolCall::new("call_1", "search", r#"{"q":"rust"}"#.to_string());
        let msg = ContextMessage::assistant_with_tools("Searching...", vec![tool_call]);
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.metadata.tool_calls.len(), 1);
    }

    #[test]
    fn test_context_message_with_tag() {
        let msg = ContextMessage::user("test").with_tag("important").with_success(true);
        assert_eq!(msg.metadata.tags, vec!["important"]);
        assert_eq!(msg.metadata.success, Some(true));
    }
}
