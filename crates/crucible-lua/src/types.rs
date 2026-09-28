//! Types for Lua tool definitions and execution

use crucible_core::serde_helpers::default_true;
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
