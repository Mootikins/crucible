//! Composable context operations for conversation management
//!
//! This module provides a set of primitives for manipulating conversation context
//! that can be composed by scripts (Lua) in response to events.
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextMessage {
    /// Message role (System, User, Assistant, Tool)
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Associated metadata
    pub metadata: MessageMetadata,
}

impl ContextMessage {
    // ─────────────────────────────────────────────────────────────────────
    // Constructors
    // ─────────────────────────────────────────────────────────────────────

    /// Internal helper for creating messages with standard metadata
    fn with_role(role: MessageRole, content: impl Into<String>) -> Self {
        let content = content.into();
        let token_estimate = content.len().div_ceil(4); // ~4 chars/token
        Self {
            role,
            content,
            metadata: MessageMetadata {
                token_estimate,
                timestamp: Some(chrono::Utc::now().timestamp()),
                ..Default::default()
            },
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self::with_role(MessageRole::User, content)
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::with_role(MessageRole::Assistant, content)
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self::with_role(MessageRole::System, content)
    }

    /// Create an assistant message with tool calls
    pub fn assistant_with_tools(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self::assistant(content).with_tool_calls(tool_calls)
    }

    /// Create a tool result message
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        let mut msg = Self::with_role(MessageRole::Tool, content);
        msg.metadata.tool_call_id = Some(tool_call_id.into());
        msg
    }

    // ─────────────────────────────────────────────────────────────────────
    // Builder methods
    // ─────────────────────────────────────────────────────────────────────

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

    /// Add tool calls to this message (typically used with assistant messages)
    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.metadata.tool_calls = tool_calls;
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


#[cfg(test)]
mod context_ops_tests;
