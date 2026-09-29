//! LLM (Large Language Model) data types
//!
//! Canonical data types for LLM tool-calling and token accounting shared across
//! crates. Provider-specific request/response shapes live in the provider
//! adapters (crucible-daemon `llm/`), not here.
//!
//! A tool that an LLM provider calls is a [`super::tools::ToolDefinition`],
//! the same type `ToolExecutor::list_tools` returns. A second
//! `LlmToolDefinition`/`FunctionDefinition` pair used to wrap it in an
//! OpenAI-shaped `{type: "function", function: {...}}` envelope, but no
//! caller ever read `r#type` (it was always `"function"`) or serialized the
//! wrapper to the wire — every caller converted it straight to the
//! provider crate's own tool type behind the provider seam
//! (`crucible-daemon/src/provider/tool_bridge.rs`). The wrapper is gone;
//! `tool_bridge::llm_tool_to_genai` takes a `ToolDefinition` directly.

use serde::{Deserialize, Serialize};

/// Message role in LLM API conversations (canonical type).
///
/// This is the canonical message role type for LLM provider communication.
/// It maps directly to OpenAI/Anthropic API message roles.
///
/// Use this type for:
/// - LLM provider communication
/// - Session persistence (Rig sessions, TUI state)
/// - Any code that needs standard assistant/user/system roles
///
/// Note: `crucible_daemon::acp::MessageRole` uses `Agent` instead of `Assistant`
/// for ACP protocol terminology - convert using `From`/`Into` when bridging.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
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

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsage {
    /// Prompt tokens used
    pub prompt_tokens: u32,
    /// Completion tokens used
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
    /// Tokens read from prompt cache (Anthropic: 90% cost reduction)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
    /// Tokens written to prompt cache (Anthropic: 1.25x cost on first write)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_tokens: Option<u32>,
}
