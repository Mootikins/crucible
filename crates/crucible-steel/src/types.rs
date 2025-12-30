//! Types for Steel tool definitions

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// A Steel tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteelTool {
    /// Tool name (function name)
    pub name: String,
    /// Description from doc comment
    pub description: String,
    /// Parameters with types
    pub params: Vec<ToolParam>,
    /// Path to source file
    pub source_path: String,
}

/// Tool parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    /// Parameter name
    pub name: String,
    /// Type (string, number, boolean, etc.)
    pub param_type: String,
    /// Description
    pub description: String,
    /// Whether required
    pub required: bool,
}

/// Result of executing a tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Result content (if success)
    pub content: JsonValue,
    /// Error message (if failure)
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(content: JsonValue) -> Self {
        Self {
            success: true,
            content,
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            content: JsonValue::Null,
            error: Some(message.into()),
        }
    }
}
