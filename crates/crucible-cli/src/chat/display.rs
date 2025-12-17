//! Terminal display formatting for chat interface
//!
//! Provides colorized, formatted output for chat messages, commands, and agent responses.

use crate::chat::diff::DiffRenderer;
use crate::formatting::render_markdown;
use colored::Colorize;
use crucible_core::traits::chat::{mode_display_name, mode_description};
use std::path::Path;

/// Display utilities for chat interface
pub struct Display;

impl Display {
    /// Display welcome banner with mode information and command help
    pub fn welcome_banner(mode_id: &str) {
        println!("
{}", "🤖 Crucible Chat".bright_blue().bold());
        println!("{}", "=================".bright_blue());
        println!(
            "Mode: {} {}",
            mode_display_name(mode_id).bright_cyan().bold(),
            format!("({})", mode_description(mode_id)).dimmed()
        );
        println!();
        println!("{}", "Commands:".bold());
        println!("  {} - Switch to plan mode (read-only)", "/plan".green());
        println!("  {} - Switch to act mode (write-enabled)", "/act".green());
        println!("  {} - Switch to auto-approve mode", "/auto".green());
        println!("  {} - Cycle modes (or Shift+Tab)", "/mode".green());
        println!("  {} - Search knowledge base", "/search <query>".green());
        println!();
        println!(
            "{} | {}",
            "Ctrl+J for newline".dimmed(),
            "Ctrl+C twice to exit".dimmed()
        );
    }

    /// Display mode change notification
    pub fn mode_change(mode_id: &str) {
        println!(
            "{} Mode: {} ({})",
            "→".bright_cyan(),
            mode_display_name(mode_id).bright_cyan().bold(),
            mode_description(mode_id)
        );
    }

    /// Display goodbye message
    pub fn goodbye() {
        println!("{}", "👋 Goodbye!".bright_blue());
    }

    /// Display search usage hint
    pub fn search_usage() {
        println!("{} Usage: /search <query>", "!".yellow());
    }

    /// Display search results header
    pub fn search_results_header(_query: &str, count: usize) {
        println!("{} Found {} results:
", "●".bright_blue(), count);
    }

    /// Display a single search result
    pub fn search_result(index: usize, title: &str, similarity: f32, snippet: &str) {
        println!(
            "  {} {} {}",
            format!("{}.", index + 1).dimmed(),
            title.bright_white(),
            format!("({:.0}%)", similarity * 100.0).dimmed()
        );
        // Show snippet preview (first non-empty line)
        if !snippet.is_empty() {
            let preview = snippet.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            if !preview.is_empty() {
                let truncated = if preview.len() > 80 {
                    format!("{}...", truncate_at_char_boundary(preview, 77))
                } else {
                    preview.to_string()
                };
                println!("     {}", truncated.dimmed());
            }
        }
    }

    /// Display no results message
    pub fn no_results(query: &str) {
        println!("{} No results found for: {}", "○".dimmed(), query.italic());
    }

    /// Display search error
    pub fn search_error(error: &str) {
        println!("{} Search failed: {}", "✗".red(), error);
    }

    /// Display agent response with optional tool calls
    pub fn agent_response(response: &str, tool_calls: &[ToolCallDisplay]) {
        // Check if response contains inline tools (▷)
        let has_inline_tools = response.contains('▷');

        // For responses with inline tools, skip markdown rendering to preserve formatting
        // Markdown rendering converts single newlines to spaces, breaking tool display
        let rendered = if has_inline_tools {
            response.to_string()
        } else {
            render_markdown(response)
        };

        // Print with indicator on first line, rest indented
        let mut lines = rendered.lines();
        if let Some(first) = lines.next() {
            println!("{} {}", "●".bright_blue(), first);
            for line in lines {
                println!("  {}", line);
            }
        }

        // Show tool calls that are missing from the inline stream (fallback)
        if !tool_calls.is_empty() && (response.trim().is_empty() || !has_inline_tools) {
            for tool in tool_calls {
                let args_str = format_tool_args(&tool.arguments);
                let display_title = humanize_tool_title(&tool.title);
                println!("  {} {}({})", "▷".cyan(), display_title, args_str.dimmed());

                // Try to display diff for write operations
                Self::maybe_display_diff(tool);
            }
        }
        println!(); // Blank line after response
    }

    /// Check if tool call is a write operation and display diff if possible
    fn maybe_display_diff(tool: &ToolCallDisplay) {
        // Identify write operations by common tool names
        // Check both original title (e.g., "mcp__crucible__update_note") and humanized
        let write_tools = [
            "Edit",
            "edit",
            "WriteFile",
            "write_file",
            "write_text_file",
            "update_note",
            "create_note",
            "Write",
            "write",
        ];

        let humanized = humanize_tool_title(&tool.title);
        let is_write = write_tools
            .iter()
            .any(|w| tool.title.contains(w) || humanized.contains(w));
        if !is_write {
            return;
        }

        // Extract path and content from arguments
        let Some(args) = &tool.arguments else {
            return;
        };

        let Some(obj) = args.as_object() else {
            return;
        };

        // Try common parameter names for file path
        let path = obj
            .get("path")
            .or_else(|| obj.get("file_path"))
            .or_else(|| obj.get("file"))
            .and_then(|v| v.as_str());

        // Try common parameter names for content
        let new_content = obj
            .get("content")
            .or_else(|| obj.get("new_content"))
            .or_else(|| obj.get("text"))
            .and_then(|v| v.as_str());

        let Some(path_str) = path else {
            return;
        };

        let Some(new_content) = new_content else {
            return;
        };

        // Try to read current file content for diff
        let path = Path::new(path_str);
        let old_content = std::fs::read_to_string(path).ok();

        // Display diff
        let renderer = DiffRenderer::new();
        let old = old_content.as_deref().unwrap_or("");
        renderer.print_result(path_str, old, new_content);
    }

    /// Display error message
    pub fn error(message: &str) {
        println!("{} Error: {}", "✗".red(), message);
    }
}

/// Tool call information for display
#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    pub title: String,
    pub arguments: Option<serde_json::Value>,
}

/// Format tool call arguments for display
pub fn format_tool_args(args: &Option<serde_json::Value>) -> String {
    match args {
        Some(serde_json::Value::Object(map)) => map
            .iter()
            .map(|(k, v)| format!("{}={}", k, format_arg_value(v)))
            .collect::<Vec<_>>()
            .join(", "),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

/// Format a single argument value, truncating if too long
fn format_arg_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => {
            let truncated = if s.len() > 30 {
                format!("{}...", truncate_at_char_boundary(s, 27))
            } else {
                s.clone()
            };
            format!("\"{}\"", truncated)
        }
        other => {
            let s = other.to_string();
            if s.len() > 30 {
                format!("{}...", truncate_at_char_boundary(&s, 27))
            } else {
                s
            }
        }
    }
}

/// Safely truncate a string at a char boundary, never panicking on multi-byte UTF-8
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    // Find the largest valid char boundary <= max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
}

/// Humanize tool title (capitalize first letter, replace underscores)
fn humanize_tool_title(title: &str) -> String {
    if title.is_empty() {
        return String::new();
    }

    // Replace underscores with spaces
    let with_spaces = title.replace('_', " ");

    // Capitalize first letter
    let mut chars = with_spaces.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Tool argument formatting tests
    #[test]
    fn test_format_tool_args_none() {
        assert_eq!(format_tool_args(&None), "");
    }

    #[test]
    fn test_format_tool_args_object() {
        let args = json!({
            "path": "/tmp/test.txt",
            "mode": "read"
        });
        let formatted = format_tool_args(&Some(args));
        assert!(formatted.contains("path="));
        assert!(formatted.contains("mode="));
        assert!(formatted.contains("\"/tmp/test.txt\""));
        assert!(formatted.contains("\"read\""));
    }

    #[test]
    fn test_format_tool_args_string_truncation() {
        let long_string = "a".repeat(50);
        let args = json!({
            "long": long_string
        });
        let formatted = format_tool_args(&Some(args));
        assert!(formatted.len() < 100); // Should be truncated
        assert!(formatted.contains("..."));
    }

    #[test]
    fn test_format_tool_args_non_object() {
        let args = json!("simple string");
        let formatted = format_tool_args(&Some(args));
        assert_eq!(formatted, "\"simple string\"");
    }

    #[test]
    fn test_format_arg_value_string() {
        let value = json!("test string");
        let formatted = format_arg_value(&value);
        assert_eq!(formatted, "\"test string\"");
    }

    #[test]
    fn test_format_arg_value_string_truncation() {
        let long_string = "a".repeat(50);
        let value = json!(long_string);
        let formatted = format_arg_value(&value);
        assert!(formatted.len() < 50);
        assert!(formatted.ends_with("...\""));
    }

    #[test]
    fn test_format_arg_value_number() {
        let value = json!(42);
        let formatted = format_arg_value(&value);
        assert_eq!(formatted, "42");
    }

    #[test]
    fn test_format_arg_value_boolean() {
        let value = json!(true);
        let formatted = format_arg_value(&value);
        assert_eq!(formatted, "true");
    }

    #[test]
    fn test_format_arg_value_array_truncation() {
        let value = json!([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let formatted = format_arg_value(&value);
        if formatted.len() > 30 {
            assert!(formatted.contains("..."));
        }
    }

    // Humanize tool title tests
    #[test]
    fn test_humanize_tool_title_simple() {
        assert_eq!(humanize_tool_title("test"), "Test");
    }

    #[test]
    fn test_humanize_tool_title_with_underscores() {
        assert_eq!(humanize_tool_title("read_file"), "Read file");
    }

    #[test]
    fn test_humanize_tool_title_multiple_words() {
        assert_eq!(
            humanize_tool_title("search_knowledge_base"),
            "Search knowledge base"
        );
    }

    #[test]
    fn test_humanize_tool_title_empty() {
        assert_eq!(humanize_tool_title(""), "");
    }

    // ToolCallDisplay tests
    #[test]
    fn test_tool_call_display_creation() {
        let tool = ToolCallDisplay {
            title: "test".to_string(),
            arguments: Some(json!({"key": "value"})),
        };
        assert_eq!(tool.title, "test");
        assert!(tool.arguments.is_some());
    }

    #[test]
    fn test_tool_call_display_clone() {
        let tool = ToolCallDisplay {
            title: "test".to_string(),
            arguments: None,
        };
        let cloned = tool.clone();
        assert_eq!(tool.title, cloned.title);
    }

    // === Diff display helper tests ===

    #[test]
    fn test_write_tools_detected() {
        let write_names = ["Edit", "write_file", "update_note", "create_note"];
        for name in write_names {
            let tool = ToolCallDisplay {
                title: name.to_string(),
                arguments: None,
            };
            Display::maybe_display_diff(&tool);
        }
    }

    #[test]
    fn test_non_write_tools_ignored() {
        let non_write_names = ["read_file", "search", "list_notes", "get_info"];
        for name in non_write_names {
            let tool = ToolCallDisplay {
                title: name.to_string(),
                arguments: Some(json!({"path": "/tmp/test.txt", "content": "test"})),
            };
            Display::maybe_display_diff(&tool);
        }
    }

    // UTF-8 safety tests
    #[test]
    fn test_format_arg_value_utf8_boundary_safety() {
        let dangerous = "aaaaaaaaaaaaaaaaaaaaaaaaaaa🔥more";
        let value = json!(dangerous);

        let result = std::panic::catch_unwind(|| format_arg_value(&value));
        assert!(result.is_ok(), "Should not panic on UTF-8 boundary");
    }

    #[test]
    fn test_format_arg_value_emoji_truncation() {
        let emojis = "🎉🎊🎋🎌🎍🎎🎏🎐🎑🎃🎄";
        let value = json!(emojis);

        let result = std::panic::catch_unwind(|| format_arg_value(&value));
        assert!(result.is_ok(), "Should handle emojis (4-byte chars)");
    }
}
