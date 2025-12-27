//! Types for Lua tool definitions and execution

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// A tool defined in Lua
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuaTool {
    /// Tool name (used for invocation)
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// Parameter definitions
    pub params: Vec<ToolParam>,

    /// Source file path
    pub source_path: String,

    /// Whether this is a Fennel source (vs plain Lua)
    pub is_fennel: bool,
}

/// Parameter definition for a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    /// Parameter name
    pub name: String,

    /// Parameter type hint (string, number, boolean, table)
    #[serde(rename = "type")]
    pub param_type: String,

    /// Human-readable description
    #[serde(default)]
    pub description: String,

    /// Whether parameter is required
    #[serde(default = "default_true")]
    pub required: bool,

    /// Default value if not provided
    #[serde(default)]
    pub default: Option<JsonValue>,
}

fn default_true() -> bool {
    true
}

/// Result of executing a Lua tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LuaExecutionResult {
    /// Execution succeeded
    pub success: bool,

    /// Result content (if successful)
    #[serde(default)]
    pub content: Option<JsonValue>,

    /// Error message (if failed)
    #[serde(default)]
    pub error: Option<String>,

    /// Execution time in milliseconds
    pub duration_ms: u64,
}

/// Unified tool result that both Rune and Lua return
///
/// This is the common interface for the Rust core to consume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Result content (JSON-serializable)
    pub content: JsonValue,

    /// Optional metadata
    #[serde(default)]
    pub metadata: Option<JsonValue>,

    /// Whether execution succeeded
    #[serde(default = "default_true")]
    pub success: bool,

    /// Error message if failed
    #[serde(default)]
    pub error: Option<String>,
}

impl ToolResult {
    /// Create a successful result
    pub fn ok(content: impl Into<JsonValue>) -> Self {
        Self {
            content: content.into(),
            metadata: None,
            success: true,
            error: None,
        }
    }

    /// Create a successful result with metadata
    pub fn ok_with_metadata(content: impl Into<JsonValue>, metadata: impl Into<JsonValue>) -> Self {
        Self {
            content: content.into(),
            metadata: Some(metadata.into()),
            success: true,
            error: None,
        }
    }

    /// Create an error result
    pub fn err(message: impl Into<String>) -> Self {
        Self {
            content: JsonValue::Null,
            metadata: None,
            success: false,
            error: Some(message.into()),
        }
    }
}
