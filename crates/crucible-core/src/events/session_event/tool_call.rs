//! Tool call representation
//!
//! Represents a tool call made by an agent.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// A tool call made by an agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    /// Tool name.
    pub name: String,
    /// Tool arguments as JSON.
    pub args: JsonValue,
    /// Optional call ID for correlation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
}

impl ToolCall {
    /// Create a new tool call.
    pub fn new(name: impl Into<String>, args: JsonValue) -> Self {
        Self {
            name: name.into(),
            args,
            call_id: None,
        }
    }

    /// Set the call ID.
    pub fn with_call_id(mut self, id: impl Into<String>) -> Self {
        self.call_id = Some(id.into());
        self
    }
}

impl Default for ToolCall {
    fn default() -> Self {
        Self {
            name: String::new(),
            args: JsonValue::Null,
            call_id: None,
        }
    }
}
