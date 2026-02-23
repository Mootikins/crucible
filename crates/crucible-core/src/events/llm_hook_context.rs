//! LLM lifecycle hook context types.
//!
//! This module provides context types for LLM pre-call and post-call hooks,
//! enabling handlers to inspect and modify LLM interactions.

use crate::traits::context_ops::ContextMessage;
use serde::{Deserialize, Serialize};

/// Context passed to pre-LLM-call hooks.
///
/// Handlers can inspect the prompt, model, and context messages before
/// the LLM is invoked. The handler can return `PreLlmResult::Continue` to
/// proceed (possibly with modifications) or `PreLlmResult::Cancel` to abort.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreLlmContext {
    /// The prompt text being sent to the LLM.
    pub prompt: String,

    /// The model identifier being used.
    pub model: String,

    /// Optional system prompt (if set).
    pub system_prompt: Option<String>,

    /// The conversation context messages.
    pub context_messages: Vec<ContextMessage>,

    /// The session ID for this LLM call.
    pub session_id: String,
}

/// Context passed to post-LLM-call hooks.
///
/// Handlers can observe the LLM response and metadata. This is observe-only
/// in v1 — no modification capability. Future versions may support response
/// rewriting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostLlmContext {
    /// The response text from the LLM.
    pub response: String,

    /// The model that generated the response.
    pub model: String,

    /// The session ID for this LLM call.
    pub session_id: String,

    /// Duration of the LLM call in milliseconds.
    pub duration_ms: u64,

    /// Token count (if available from the LLM provider).
    pub token_count: Option<u64>,
}

/// Result type for pre-LLM-call hooks.
///
/// Handlers return this to control whether the LLM call proceeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreLlmResult {
    /// Proceed with the LLM call (possibly with modified context).
    Continue(PreLlmContext),

    /// Cancel the LLM call. The string is the cancellation reason.
    Cancel(String),
}
