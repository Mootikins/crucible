//! Agent behavior definitions
//!
//! Defines different behaviors the mock agent can exhibit for testing various scenarios.

use serde_json::Value;
use std::collections::HashMap;

/// Defines the behavior of the mock agent
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentBehavior {
    /// Mimics OpenCode agent
    OpenCode,

    /// Mimics Claude ACP agent
    ClaudeAcp,

    /// Mimics Gemini agent
    Gemini,

    /// Mimics Codex agent
    Codex,

    /// Sends streaming responses (4 chunks + final response)
    Streaming,

    /// Sends streaming responses with delays between chunks
    StreamingSlow,

    /// Sends streaming chunks but never sends final response (for timeout testing)
    StreamingIncomplete,

    /// Custom behavior defined by user
    Custom(HashMap<String, Value>),
}

impl AgentBehavior {
    /// Parse from string (for CLI arguments)
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "opencode" => Some(Self::OpenCode),
            "claude-acp" | "claude" => Some(Self::ClaudeAcp),
            "gemini" => Some(Self::Gemini),
            "codex" => Some(Self::Codex),
            "streaming" => Some(Self::Streaming),
            "streaming-slow" => Some(Self::StreamingSlow),
            "streaming-incomplete" => Some(Self::StreamingIncomplete),
            _ => None,
        }
    }

    /// Check if this behavior should send streaming responses
    pub fn is_streaming(&self) -> bool {
        matches!(
            self,
            Self::Streaming | Self::StreamingSlow | Self::StreamingIncomplete
        )
    }

    /// Check if this behavior should send a final response
    pub fn sends_final_response(&self) -> bool {
        !matches!(self, Self::StreamingIncomplete)
    }
}

impl Default for AgentBehavior {
    fn default() -> Self {
        Self::Streaming
    }
}
