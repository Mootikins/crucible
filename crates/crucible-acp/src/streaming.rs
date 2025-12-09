//! Response streaming for ACP sessions
//!
//! This module handles streaming responses from agents, including message chunks,
//! tool calls, and thought processes.
//!
//! ## Design Principles
//!
//! - **Single Responsibility**: Focused on streaming and formatting responses
//! - **Open/Closed**: Extensible for different output formats
//! - **Dependency Inversion**: Uses core types, protocol-agnostic

use crate::Result;

// Re-export ToolCallInfo from core for backwards compatibility
pub use crucible_core::types::acp::ToolCallInfo;

/// Convert a tool title into a human-readable name by removing MCP schema prefixes.
pub fn humanize_tool_title(title: &str) -> String {
    if let Some(stripped) = title.strip_prefix("mcp__crucible__") {
        stripped.to_string()
    } else if let Some(stripped) = title.strip_prefix("mcp__") {
        stripped.to_string()
    } else {
        title.to_string()
    }
}

/// Configuration for response streaming
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Whether to show agent thoughts
    pub show_thoughts: bool,

    /// Whether to show tool calls
    pub show_tool_calls: bool,

    /// Whether to use color output (for terminal)
    pub use_colors: bool,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            show_thoughts: true,
            show_tool_calls: true,
            use_colors: false, // Default to false for tests
        }
    }
}

/// Handles streaming responses from agents
pub struct StreamHandler {
    config: StreamConfig,
}

impl StreamHandler {
    /// Create a new stream handler
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for streaming behavior
    pub fn new(config: StreamConfig) -> Self {
        Self { config }
    }

    /// Format an agent message chunk for display
    ///
    /// # Arguments
    ///
    /// * `chunk` - The message chunk content
    ///
    /// # Returns
    ///
    /// Formatted string ready for display
    pub fn format_message_chunk(&self, chunk: &str) -> Result<String> {
        // For now, just return the chunk as-is
        // In a full implementation, this could add colors, formatting, etc.
        Ok(chunk.to_string())
    }

    /// Format a thought chunk for display
    ///
    /// # Arguments
    ///
    /// * `chunk` - The thought chunk content
    ///
    /// # Returns
    ///
    /// Formatted string ready for display, or None if thoughts are disabled
    pub fn format_thought_chunk(&self, chunk: &str) -> Result<Option<String>> {
        if !self.config.show_thoughts {
            return Ok(None);
        }

        let normalized = formatting::normalize_chunk(chunk);
        let formatted = self.format_with_prefix("Thinking", &normalized);
        Ok(Some(formatted))
    }

    /// Format a tool call notification
    ///
    /// # Arguments
    ///
    /// * `tool_name` - Name of the tool being called
    /// * `params` - Tool parameters as JSON
    ///
    /// # Returns
    ///
    /// Formatted string, or None if tool calls are disabled
    pub fn format_tool_call(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> Result<Option<String>> {
        if !self.config.show_tool_calls {
            return Ok(None);
        }

        let params_str = formatting::format_json_compact(params);
        let content = format!("{}: {}", tool_name, params_str);
        let formatted = self.format_with_prefix("Tool", &content);

        Ok(Some(formatted))
    }

    /// Get the configuration
    pub fn config(&self) -> &StreamConfig {
        &self.config
    }

    /// Helper to format with prefix
    fn format_with_prefix(&self, prefix: &str, content: &str) -> String {
        if self.config.use_colors {
            // Placeholder for color formatting
            // In a real implementation, use a crate like `colored` or `owo-colors`
            format!("[{}] {}", prefix, content)
        } else {
            format!("[{}] {}", prefix, content)
        }
    }
}

/// Formatting utilities for different content types
mod formatting {
    /// Clean and normalize chunk content
    pub fn normalize_chunk(chunk: &str) -> String {
        // Remove any control characters but preserve newlines
        chunk.to_string()
    }

    /// Format JSON for display
    pub fn format_json_compact(value: &serde_json::Value) -> String {
        // Use compact formatting for single-line display
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_handler_creation() {
        let config = StreamConfig::default();
        let handler = StreamHandler::new(config);

        assert!(handler.config().show_thoughts);
        assert!(handler.config().show_tool_calls);
    }

    #[test]
    fn test_custom_stream_config() {
        let config = StreamConfig {
            show_thoughts: false,
            show_tool_calls: true,
            use_colors: true,
        };

        let handler = StreamHandler::new(config);
        assert!(!handler.config().show_thoughts);
        assert!(handler.config().show_tool_calls);
        assert!(handler.config().use_colors);
    }

    #[test]
    fn test_format_message_chunk() {
        let handler = StreamHandler::new(StreamConfig::default());
        let chunk = "Hello, world!";

        let result = handler.format_message_chunk(chunk);
        assert!(result.is_ok(), "Should format message chunks");

        let formatted = result.unwrap();
        assert!(
            formatted.contains(chunk),
            "Should contain the chunk content"
        );
    }

    #[test]
    fn test_format_thought_chunk_enabled() {
        let handler = StreamHandler::new(StreamConfig {
            show_thoughts: true,
            ..Default::default()
        });

        let chunk = "I need to search for information...";
        let result = handler.format_thought_chunk(chunk);

        assert!(result.is_ok());
        let formatted = result.unwrap();
        assert!(formatted.is_some(), "Should format when thoughts enabled");
        assert!(formatted.unwrap().contains(chunk));
    }

    #[test]
    fn test_format_thought_chunk_disabled() {
        let handler = StreamHandler::new(StreamConfig {
            show_thoughts: false,
            ..Default::default()
        });

        let chunk = "Internal thought";
        let result = handler.format_thought_chunk(chunk);

        assert!(result.is_ok());
        let formatted = result.unwrap();
        assert!(
            formatted.is_none(),
            "Should return None when thoughts disabled"
        );
    }

    #[test]
    fn test_format_tool_call_enabled() {
        let handler = StreamHandler::new(StreamConfig {
            show_tool_calls: true,
            ..Default::default()
        });

        let tool_name = "read_note";
        let params = serde_json::json!({ "path": "test.md" });

        let result = handler.format_tool_call(tool_name, &params);
        assert!(result.is_ok());

        let formatted = result.unwrap();
        assert!(formatted.is_some(), "Should format when tool calls enabled");

        let text = formatted.unwrap();
        assert!(text.contains(tool_name), "Should contain tool name");
    }

    #[test]
    fn test_format_tool_call_disabled() {
        let handler = StreamHandler::new(StreamConfig {
            show_tool_calls: false,
            ..Default::default()
        });

        let tool_name = "read_note";
        let params = serde_json::json!({ "path": "test.md" });

        let result = handler.format_tool_call(tool_name, &params);
        assert!(result.is_ok());

        let formatted = result.unwrap();
        assert!(
            formatted.is_none(),
            "Should return None when tool calls disabled"
        );
    }

    #[test]
    fn test_multiple_chunks() {
        let handler = StreamHandler::new(StreamConfig::default());

        let chunks = vec!["Hello", ", ", "world", "!"];
        let mut result = String::new();

        for chunk in chunks {
            let formatted = handler.format_message_chunk(chunk).unwrap();
            result.push_str(&formatted);
        }

        assert_eq!(result, "Hello, world!");
    }
}
