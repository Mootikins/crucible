//! The result type of a Lua tool run

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

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
