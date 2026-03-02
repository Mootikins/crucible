//! Shared helper functions for constructing MCP tool responses.

use rmcp::model::{CallToolResult, Content};
use serde::Serialize;

/// Create a successful tool response containing JSON content.
///
/// Replaces the verbose `Ok(CallToolResult::success(vec![Content::json(value)?]))` pattern.
///
/// # Errors
///
/// Returns `rmcp::ErrorData` if the value cannot be serialized to JSON.
pub fn json_success(value: impl Serialize) -> Result<CallToolResult, rmcp::ErrorData> {
    Ok(CallToolResult::success(vec![Content::json(value)?]))
}

/// Create a successful tool response containing text content.
///
/// Replaces the verbose `Ok(CallToolResult::success(vec![Content::text(...)]))` pattern.
pub fn text_success(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text)])
}

/// Extension trait to convert any error into `rmcp::ErrorData`.
///
/// Replaces verbose `.map_err(|e| rmcp::ErrorData::internal_error(..., None))` chains.
pub trait McpResultExt<T> {
    /// Convert error to an internal MCP error, using the error's Display as message.
    ///
    /// # Errors
    ///
    /// Returns `rmcp::ErrorData` wrapping the original error.
    fn mcp_err(self) -> Result<T, rmcp::ErrorData>;

    /// Convert error to an internal MCP error with a context prefix.
    ///
    /// Produces `"{context}: {error}"` as the error message.
    ///
    /// # Errors
    ///
    /// Returns `rmcp::ErrorData` with context prefix and original error.
    fn mcp_err_ctx(self, context: &str) -> Result<T, rmcp::ErrorData>;

    /// Convert error to an invalid-params MCP error with a context prefix.
    ///
    /// # Errors
    ///
    /// Returns `rmcp::ErrorData` as invalid-params with context prefix.
    fn mcp_invalid(self, context: &str) -> Result<T, rmcp::ErrorData>;
}

impl<T, E: std::fmt::Display> McpResultExt<T> for Result<T, E> {
    fn mcp_err(self) -> Result<T, rmcp::ErrorData> {
        self.map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))
    }

    fn mcp_err_ctx(self, context: &str) -> Result<T, rmcp::ErrorData> {
        self.map_err(|e| rmcp::ErrorData::internal_error(format!("{context}: {e}"), None))
    }

    fn mcp_invalid(self, context: &str) -> Result<T, rmcp::ErrorData> {
        self.map_err(|e| rmcp::ErrorData::invalid_params(format!("{context}: {e}"), None))
    }
}
