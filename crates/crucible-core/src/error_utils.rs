//! Utility functions for handling tool error messages.

/// Strip known tool error prefixes from the start of a string.
///
/// Repeatedly removes known error prefixes until none remain at the start.
/// This handles nested error wrapping where multiple layers add their own prefix.
///
/// Known prefixes (in order of checking):
/// - `"ToolCallError: "` — from Rig's `ToolSetError` and `ToolError`
/// - `"Toolset error: "` — appears in TUI display path
/// - `"tool execution failed: "` — from `InProcessToolError`
/// - `"MCP gateway error: "` — from `McpProxyError`
/// - `"MCP tool error: "` — from `McpProxyError`
///
/// # Examples
///
/// ```
/// use crucible_core::error_utils::strip_tool_error_prefix;
///
/// // Single prefix
/// assert_eq!(strip_tool_error_prefix("tool execution failed: bad path"), "bad path");
///
/// // Triple-nested ToolCallError
/// assert_eq!(
///     strip_tool_error_prefix("ToolCallError: ToolCallError: ToolCallError: actual error"),
///     "actual error"
/// );
///
/// // Mixed prefixes
/// assert_eq!(
///     strip_tool_error_prefix("ToolCallError: MCP gateway error: Tool 'x' not found"),
///     "Tool 'x' not found"
/// );
///
/// // No prefix
/// assert_eq!(strip_tool_error_prefix("Hello world"), "Hello world");
///
/// // Empty string
/// assert_eq!(strip_tool_error_prefix(""), "");
/// ```
#[must_use]
pub fn strip_tool_error_prefix(s: &str) -> String {
    let prefixes = [
        "ToolCallError: ",
        "Toolset error: ",
        "tool execution failed: ",
        "MCP gateway error: ",
        "MCP tool error: ",
    ];

    let mut result = s.to_string();
    let mut changed = true;

    // Keep stripping prefixes until none match
    while changed {
        changed = false;
        for prefix in &prefixes {
            if result.starts_with(prefix) {
                result = result[prefix.len()..].to_string();
                changed = true;
                break; // Restart from the beginning of the prefix list
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_single_tool_call_error() {
        assert_eq!(
            strip_tool_error_prefix("ToolCallError: actual error"),
            "actual error"
        );
    }

    #[test]
    fn test_strip_triple_nested_tool_call_error() {
        assert_eq!(
            strip_tool_error_prefix("ToolCallError: ToolCallError: ToolCallError: actual error"),
            "actual error"
        );
    }

    #[test]
    fn test_strip_mixed_prefixes() {
        assert_eq!(
            strip_tool_error_prefix("ToolCallError: MCP gateway error: Tool 'x' not found"),
            "Tool 'x' not found"
        );
    }

    #[test]
    fn test_strip_tool_execution_failed() {
        assert_eq!(
            strip_tool_error_prefix("tool execution failed: bad path"),
            "bad path"
        );
    }

    #[test]
    fn test_preserve_non_error_string() {
        assert_eq!(strip_tool_error_prefix("Hello world"), "Hello world");
    }

    #[test]
    fn test_handle_empty_string() {
        assert_eq!(strip_tool_error_prefix(""), "");
    }

    #[test]
    fn test_string_that_is_just_prefix() {
        assert_eq!(strip_tool_error_prefix("MCP gateway error: "), "");
    }

    #[test]
    fn test_strip_toolset_error() {
        assert_eq!(
            strip_tool_error_prefix("Toolset error: connection failed"),
            "connection failed"
        );
    }

    #[test]
    fn test_strip_mcp_tool_error() {
        assert_eq!(
            strip_tool_error_prefix("MCP tool error: timeout"),
            "timeout"
        );
    }

    #[test]
    fn test_complex_nested_chain() {
        assert_eq!(
            strip_tool_error_prefix(
                "ToolCallError: Toolset error: MCP gateway error: tool execution failed: network error"
            ),
            "network error"
        );
    }
}
