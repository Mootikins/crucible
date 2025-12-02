//! Terminal display formatting for chat interface
//!
//! Provides colorized, formatted output for chat messages, commands, and agent responses.

use colored::Colorize;
use crucible_core::traits::chat::ChatMode;
use crate::chat::mode_ext::ChatModeDisplay;
use crate::formatting::render_markdown;

/// Display utilities for chat interface
pub struct Display;

impl Display {
    /// Display welcome banner with mode information and command help
    pub fn welcome_banner(mode: ChatMode) {
        println!("\n{}", "🤖 Crucible Chat".bright_blue().bold());
        println!("{}", "=================".bright_blue());
        println!(
            "Mode: {} {}",
            mode.display_name().bright_cyan().bold(),
            format!("({})", mode.description()).dimmed()
        );
        println!();
        println!("{}", "Commands:".bold());
        println!("  {} - Switch to plan mode (read-only)", "/plan".green());
        println!("  {} - Switch to act mode (write-enabled)", "/act".green());
        println!("  {} - Switch to auto-approve mode", "/auto".green());
        println!("  {} - Cycle modes (or Shift+Tab)", "/mode".green());
        println!(
            "  {} - Search knowledge base",
            "/search <query>".green()
        );
        println!();
        println!(
            "{} | {}",
            "Ctrl+J for newline".dimmed(),
            "Ctrl+C twice to exit".dimmed()
        );
    }

    /// Display mode change notification
    pub fn mode_change(mode: ChatMode) {
        println!(
            "{} Mode: {} ({})",
            "→".bright_cyan(),
            mode.display_name().bright_cyan().bold(),
            mode.description()
        );
    }

    /// Display goodbye message
    pub fn goodbye() {
        println!("{}", "👋 Goodbye!".bright_blue());
    }

    /// Display search usage hint
    pub fn search_usage() {
        println!(
            "{} Usage: /search <query>",
            "!".yellow()
        );
    }

    /// Display search results header
    pub fn search_results_header(query: &str, count: usize) {
        println!(
            "{} Found {} results:\n",
            "●".bright_blue(),
            count
        );
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
            let preview = snippet
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            if !preview.is_empty() {
                let truncated = if preview.len() > 80 {
                    format!("{}...", &preview[..77])
                } else {
                    preview.to_string()
                };
                println!("     {}", truncated.dimmed());
            }
        }
    }

    /// Display no results message
    pub fn no_results(query: &str) {
        println!(
            "{} No results found for: {}",
            "○".dimmed(),
            query.italic()
        );
    }

    /// Display search error
    pub fn search_error(error: &str) {
        println!("{} Search failed: {}", "✗".red(), error);
    }

    /// Display agent response with optional tool calls
    pub fn agent_response(response: &str, tool_calls: &[ToolCallDisplay]) {
        // Print agent response with markdown rendering
        let rendered = render_markdown(response);
        // Print with indicator on first line, rest indented
        let mut lines = rendered.lines();
        if let Some(first) = lines.next() {
            println!("{} {}", "●".bright_blue(), first);
            for line in lines {
                println!("  {}", line);
            }
        }

        // Show tool calls that are missing from the inline stream (fallback)
        let has_inline_tools = response.contains('▷');
        if !tool_calls.is_empty()
            && (response.trim().is_empty() || !has_inline_tools)
        {
            for tool in tool_calls {
                let args_str = format_tool_args(&tool.arguments);
                let display_title = humanize_tool_title(&tool.title);
                println!(
                    "  {} {}({})",
                    "▷".cyan(),
                    display_title,
                    args_str.dimmed()
                );
            }
        }
        println!(); // Blank line after response
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
                format!("{}...", &s[..27])
            } else {
                s.clone()
            };
            format!("\"{}\"", truncated)
        }
        other => {
            let s = other.to_string();
            if s.len() > 30 {
                format!("{}...", &s[..27])
            } else {
                s
            }
        }
    }
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
        assert_eq!(humanize_tool_title("search_knowledge_base"), "Search knowledge base");
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

    // Note: Display methods output to stdout, so we can't easily test exact output.
    // We test the underlying formatting functions that they use.
    // Integration tests would verify actual terminal output.
}
