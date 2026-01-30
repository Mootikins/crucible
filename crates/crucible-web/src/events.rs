//! SSE event types for chat streaming

use serde::{Deserialize, Serialize};

/// Events sent to the browser via SSE
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    /// A token/chunk of the response
    Token { content: String },

    /// A tool call is being made
    ToolCall {
        id: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments: Option<serde_json::Value>,
    },

    /// Tool call result
    ToolResult {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },

    /// Agent is thinking/reasoning
    Thinking { content: String },

    /// Message is complete
    MessageComplete {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        tool_calls: Vec<ToolCallSummary>,
    },

    /// An error occurred
    Error { code: String, message: String },

    /// An interaction is requested from the user
    InteractionRequested {
        id: String,
        #[serde(flatten)]
        request: serde_json::Value,
    },
}

/// Summary of a tool call for the complete message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallSummary {
    pub id: String,
    pub title: String,
}

impl ChatEvent {
    /// Format as SSE event string
    pub fn to_sse(&self) -> String {
        let event_type = match self {
            ChatEvent::Token { .. } => "token",
            ChatEvent::ToolCall { .. } => "tool_call",
            ChatEvent::ToolResult { .. } => "tool_result",
            ChatEvent::Thinking { .. } => "thinking",
            ChatEvent::MessageComplete { .. } => "message_complete",
            ChatEvent::Error { .. } => "error",
            ChatEvent::InteractionRequested { .. } => "interaction_requested",
        };

        let data = serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string());

        format!("event: {}\ndata: {}\n\n", event_type, data)
    }
}
