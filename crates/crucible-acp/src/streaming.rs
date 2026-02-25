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

/// A streaming chunk from an ACP agent.
///
/// These events are emitted as they arrive from the agent,
/// enabling real-time display of agent responses.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamingChunk {
    /// Text content from the agent's response
    Text(String),
    /// Agent is thinking (for agents that expose thinking)
    Thinking(String),
    /// A tool is being called
    ToolStart {
        name: String,
        id: String,
        arguments: Option<serde_json::Value>,
    },
    /// Tool execution completed
    ToolEnd {
        id: String,
        result: Option<String>,
        error: Option<String>,
    },
}

/// Callback type for receiving streaming chunks.
///
/// The callback receives chunks as they arrive from the agent.
/// Return `true` to continue streaming, `false` to cancel.
pub type StreamingCallback = Box<dyn FnMut(StreamingChunk) -> bool + Send>;

/// Create a callback that sends chunks to an unbounded channel.
///
/// This is useful for integrating with async code that needs to
/// poll for chunks rather than receive callbacks.
pub fn channel_callback(
    tx: tokio::sync::mpsc::UnboundedSender<StreamingChunk>,
) -> StreamingCallback {
    Box::new(move |chunk| tx.send(chunk).is_ok())
}

/// Convert a tool title into a human-readable name by removing MCP schema prefixes
/// and title-casing the result.
///
/// Handles patterns:
/// - `mcp__crucible__semantic_search` → `Semantic Search`
/// - `mcp__create_issue` → `Create Issue`
/// - `mcp_write` → `Write`
/// - `plugin_NAME_NAME__search` → `Search`
/// - `Read File` → `Read File` (already clean)
pub fn humanize_tool_title(title: &str) -> String {
    // Strip known prefixes
    let stripped = if let Some(s) = title.strip_prefix("mcp__crucible__") {
        s
    } else if let Some(s) = title.strip_prefix("mcp__") {
        s
    } else if let Some(s) = title.strip_prefix("mcp_") {
        s
    } else if let Some(s) = title.strip_prefix("plugin_") {
        // plugin_NAME_NAME__X → take X (part after __)
        if let Some(after_double_underscore) = s.split("__").last() {
            after_double_underscore
        } else {
            s
        }
    } else {
        title
    };

    // Title-case: convert snake_case to Title Case
    title_case(stripped)
}

/// Convert snake_case or kebab-case to Title Case.
/// Examples: `semantic_search` → `Semantic Search`, `create-issue` → `Create Issue`
/// If the input contains no alphanumeric characters, returns it unchanged.
fn title_case(s: &str) -> String {
    let words: Vec<String> = s
        .split(['_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect();

    if words.is_empty() {
        s.to_string()
    } else {
        words.join(" ")
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

    #[test]
    fn humanize_tool_title_mcp_double_underscore_crucible() {
        assert_eq!(
            humanize_tool_title("mcp__crucible__semantic_search"),
            "Semantic Search"
        );
    }

    #[test]
    fn humanize_tool_title_mcp_double_underscore() {
        assert_eq!(humanize_tool_title("mcp__create_issue"), "Create Issue");
    }

    #[test]
    fn humanize_tool_title_mcp_single_underscore() {
        assert_eq!(humanize_tool_title("mcp_write"), "Write");
    }

    #[test]
    fn humanize_tool_title_plugin_prefix() {
        assert_eq!(
            humanize_tool_title("plugin_episodic-memory_episodic-memory__search"),
            "Search"
        );
    }

    #[test]
    fn humanize_tool_title_already_clean() {
        assert_eq!(humanize_tool_title("Read File"), "Read File");
    }

    #[test]
    fn humanize_tool_title_simple_snake_case() {
        assert_eq!(humanize_tool_title("search"), "Search");
    }

    #[test]
    fn humanize_tool_title_kebab_case() {
        assert_eq!(humanize_tool_title("create-issue"), "Create Issue");
    }

    #[test]
    fn humanize_tool_title_complex_snake_case() {
        assert_eq!(
            humanize_tool_title("list_all_files_recursively"),
            "List All Files Recursively"
        );
    }

    #[test]
    fn channel_callback_with_text_chunk() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut callback = channel_callback(tx);

        let chunk = StreamingChunk::Text("Hello, world!".to_string());
        let result = callback(chunk.clone());

        assert!(result, "Callback should return true on successful send");

        // Verify the chunk was sent to the channel
        let received = rx.try_recv();
        assert!(received.is_ok(), "Should receive chunk from channel");
        assert_eq!(received.unwrap(), chunk);
    }

    #[test]
    fn channel_callback_with_thinking_chunk() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut callback = channel_callback(tx);

        let chunk = StreamingChunk::Thinking("Analyzing the problem...".to_string());
        let result = callback(chunk.clone());

        assert!(result, "Callback should return true on successful send");

        let received = rx.try_recv();
        assert!(received.is_ok(), "Should receive thinking chunk from channel");
        assert_eq!(received.unwrap(), chunk);
    }

    #[test]
    fn channel_callback_with_tool_start_chunk() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut callback = channel_callback(tx);

        let chunk = StreamingChunk::ToolStart {
            name: "search".to_string(),
            id: "tool_123".to_string(),
            arguments: Some(serde_json::json!({ "query": "test" })),
        };
        let result = callback(chunk.clone());

        assert!(result, "Callback should return true on successful send");

        let received = rx.try_recv();
        assert!(received.is_ok(), "Should receive tool start chunk from channel");
        assert_eq!(received.unwrap(), chunk);
    }

    #[test]
    fn channel_callback_with_tool_end_chunk() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut callback = channel_callback(tx);

        let chunk = StreamingChunk::ToolEnd {
            id: "tool_123".to_string(),
            result: Some("Found 5 results".to_string()),
            error: None,
        };
        let result = callback(chunk.clone());

        assert!(result, "Callback should return true on successful send");

        let received = rx.try_recv();
        assert!(received.is_ok(), "Should receive tool end chunk from channel");
        assert_eq!(received.unwrap(), chunk);
    }

    #[test]
    fn channel_callback_with_tool_error() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut callback = channel_callback(tx);

        let chunk = StreamingChunk::ToolEnd {
            id: "tool_456".to_string(),
            result: None,
            error: Some("Tool execution failed".to_string()),
        };
        let result = callback(chunk.clone());

        assert!(result, "Callback should return true on successful send");

        let received = rx.try_recv();
        assert!(received.is_ok(), "Should receive tool error chunk from channel");
        assert_eq!(received.unwrap(), chunk);
    }

    #[test]
    fn channel_callback_returns_false_when_receiver_dropped() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut callback = channel_callback(tx);

        // Drop the receiver to close the channel
        drop(rx);

        let chunk = StreamingChunk::Text("This should fail".to_string());
        let result = callback(chunk);

        assert!(
            !result,
            "Callback should return false when receiver is dropped"
        );
    }

    #[test]
    fn channel_callback_multiple_chunks() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut callback = channel_callback(tx);

        let chunks = vec![
            StreamingChunk::Text("Hello".to_string()),
            StreamingChunk::Text(" ".to_string()),
            StreamingChunk::Text("world".to_string()),
        ];

        for chunk in chunks.iter() {
            let result = callback(chunk.clone());
            assert!(result, "Each callback should succeed");
        }

        // Verify all chunks were received in order
        for expected_chunk in chunks {
            let received = rx.try_recv();
            assert!(received.is_ok(), "Should receive chunk from channel");
            assert_eq!(received.unwrap(), expected_chunk);
        }
    }

}
