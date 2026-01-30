//! Tool call rendering component.
//!
//! Renders tool call states: running (with spinner), complete (with result summary),
//! and error (with error message).

use crate::tui::oil::node::{col, row, spinner_with_frames, styled, Node, BRAILLE_SPINNER_FRAMES};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::utils::{terminal_width, truncate_first_line, truncate_to_chars};
use crate::tui::oil::viewport_cache::CachedToolCall;
use std::time::Duration;

/// Render a tool call with default spinner frame (0).
pub fn render_tool_call(tool: &CachedToolCall) -> Node {
    render_tool_call_with_frame(tool, 0)
}

/// Render a tool call with specified spinner frame for animation.
pub fn render_tool_call_with_frame(tool: &CachedToolCall, spinner_frame: usize) -> Node {
    let display_name = display_tool_name(&tool.name);
    let args_formatted = format_tool_args(&tool.args);
    let result_str = tool.result();

    if let Some(ref error) = tool.error {
        return render_tool_error(tool, display_name, &args_formatted, error);
    }

    if tool.complete {
        return render_tool_complete(tool, display_name, &args_formatted, &result_str);
    }

    render_tool_running(
        tool,
        display_name,
        &args_formatted,
        &result_str,
        spinner_frame,
    )
}

fn render_tool_error(
    _tool: &CachedToolCall,
    display_name: &str,
    args_formatted: &str,
    error: &str,
) -> Node {
    let theme = ThemeTokens::default_ref();
    row([
        styled(" ✗ ", Style::new().fg(theme.error)),
        styled(display_name, Style::new().fg(theme.text_dim)),
        styled(format!("({}) ", args_formatted), theme.dim()),
        styled(
            format!("→ {}", truncate_first_line(error, 50, true)),
            theme.error_style(),
        ),
    ])
}

fn render_tool_complete(
    tool: &CachedToolCall,
    display_name: &str,
    args_formatted: &str,
    result_str: &str,
) -> Node {
    let result_summary = if !result_str.is_empty() {
        summarize_tool_result(&tool.name, result_str)
    } else {
        None
    };

    let collapsed = collapse_result(&tool.name, result_str, result_summary.as_deref());
    let has_arrow_suffix = tool.output_path.is_some() || collapsed.is_some();

    let theme = ThemeTokens::default_ref();
    let arrow_suffix = if let Some(ref path) = tool.output_path {
        styled(format!("→ {}", path.display()), theme.muted())
    } else if let Some(ref s) = collapsed {
        styled(format!("→ {}", s), theme.muted())
    } else {
        Node::Empty
    };

    let header = row([
        styled(" ✓ ", Style::new().fg(theme.success)),
        styled(display_name, Style::new().fg(theme.text_dim)),
        if args_formatted.is_empty() {
            Node::Empty
        } else if has_arrow_suffix {
            styled(format!("({}) ", args_formatted), theme.dim())
        } else {
            styled(format!("({})", args_formatted), theme.dim())
        },
        arrow_suffix,
    ]);

    let result_node = if has_arrow_suffix || result_str.is_empty() {
        Node::Empty
    } else {
        format_tool_result(&tool.name, result_str)
    };

    if matches!(result_node, Node::Empty) {
        header
    } else {
        col([header, result_node])
    }
}

fn render_tool_running(
    tool: &CachedToolCall,
    display_name: &str,
    args_formatted: &str,
    result_str: &str,
    spinner_frame: usize,
) -> Node {
    let elapsed = tool.elapsed();
    let show_elapsed = elapsed >= Duration::from_secs(2);

    let theme = ThemeTokens::default_ref();
    let header = row([
        styled(" ", Style::new()),
        spinner_with_frames(
            spinner_frame,
            Style::new().fg(theme.text_dim),
            BRAILLE_SPINNER_FRAMES,
        ),
        styled(" ", Style::new()),
        styled(display_name, Style::new().fg(theme.text_dim)),
        styled(format!("({})", args_formatted), theme.dim()),
        if show_elapsed {
            styled(format!("  {}", format_elapsed(elapsed)), theme.dim())
        } else {
            Node::Empty
        },
    ]);

    let result_node = if result_str.is_empty() {
        Node::Empty
    } else {
        format_streaming_output(result_str)
    };

    if matches!(result_node, Node::Empty) {
        header
    } else {
        col([header, result_node])
    }
}

// --- Utility functions ---

fn display_tool_name(name: &str) -> &str {
    name.strip_prefix("mcp_").unwrap_or(name)
}

pub(crate) fn format_elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m{}s", secs / 60, secs % 60)
    }
}

fn collapse_result(name: &str, result: &str, summary: Option<&str>) -> Option<String> {
    if let Some(s) = summary {
        return Some(s.to_string());
    }

    if result.is_empty() {
        return None;
    }

    let inner = unwrap_json_result(result);
    let lines: Vec<&str> = inner.lines().collect();
    if lines.len() == 1 && inner.len() <= 60 {
        return Some(inner.trim().to_string());
    }

    match name {
        "write" | "mcp_write" => Some("written".to_string()),
        "edit" | "mcp_edit" => Some("applied".to_string()),
        _ => None,
    }
}

/// Format tool arguments for display.
pub fn format_tool_args(args: &str) -> String {
    if args.is_empty() || args == "{}" {
        return String::new();
    }

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) {
        if let Some(obj) = parsed.as_object() {
            let pairs: Vec<String> = obj
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        serde_json::Value::String(s) => {
                            let collapsed = s.replace('\n', "↵").replace('\r', "");
                            if collapsed.chars().count() > 30 {
                                format!("\"{}…\"", truncate_to_chars(&collapsed, 27, false))
                            } else {
                                format!("\"{}\"", collapsed)
                            }
                        }
                        other => {
                            let s = other.to_string();
                            if s.chars().count() > 30 {
                                format!("{}…", truncate_to_chars(&s, 27, false))
                            } else {
                                s
                            }
                        }
                    };
                    format!("{}={}", k, val)
                })
                .collect();
            return pairs.join(", ");
        }
    }

    let oneline = args.replace('\n', " ").replace("  ", " ");
    if oneline.chars().count() <= 60 {
        oneline
    } else {
        format!("{}…", truncate_to_chars(&oneline, 57, false))
    }
}

/// Format tool result for display.
pub fn format_tool_result(name: &str, result: &str) -> Node {
    if let Some(summary) = summarize_tool_result(name, result) {
        return styled(
            format!("   {}", summary),
            ThemeTokens::default_ref().muted(),
        );
    }
    let inner = unwrap_json_result(result);
    format_output_tail(&inner, "   ")
}

/// Summarize tool result into a short string.
pub fn summarize_tool_result(name: &str, result: &str) -> Option<String> {
    let inner = unwrap_json_result(result);
    match name {
        "read_file" | "mcp_read" => inner
            .rfind('[')
            .map(|i| inner[i..].to_string())
            .or_else(|| Some(format!("{} lines", inner.lines().count()))),
        "glob" | "mcp_glob" => count_newline_items(&inner).map(|n| format!("{} files", n)),
        "grep" | "mcp_grep" => count_grep_matches(&inner).map(|n| format!("{} matches", n)),
        "edit" | "mcp_edit" if inner.contains("success") || inner.contains("applied") => {
            Some("applied".to_string())
        }
        "write" | "mcp_write" if inner.contains("success") || inner.contains("written") => {
            Some("written".to_string())
        }
        "bash" | "mcp_bash" => {
            let lines: Vec<&str> = inner.lines().collect();
            if lines.len() <= 1 && inner.len() < 60 {
                Some(inner.trim().to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Format streaming output from a running tool.
pub fn format_streaming_output(output: &str) -> Node {
    let unwrapped = unwrap_json_result(output);
    format_output_tail(&unwrapped, "     ")
}

/// Format the tail of output with a prefix and optional "more lines" indicator.
pub fn format_output_tail(output: &str, prefix: &str) -> Node {
    let width = terminal_width();
    let all_lines: Vec<&str> = output.lines().collect();
    let lines: Vec<&str> = all_lines.iter().rev().take(3).rev().copied().collect();
    let hidden_count = all_lines.len().saturating_sub(3);
    let bar_prefix = format!("{}│ ", prefix);
    let truncate_at = width.saturating_sub(bar_prefix.len() + 1);

    col(std::iter::once(if hidden_count > 0 {
        styled(
            format!("{}({} more lines)", bar_prefix, hidden_count),
            ThemeTokens::default_ref().tool_result(),
        )
    } else {
        Node::Empty
    })
    .chain(lines.iter().map(|line| {
        let display = if line.len() > truncate_at {
            format!("{}{}…", bar_prefix, &line[..truncate_at])
        } else {
            format!("{}{}", bar_prefix, line)
        };
        styled(display, ThemeTokens::default_ref().tool_result())
    })))
}

/// Unwraps JSON-encoded strings and `{"result": "..."}` objects.
///
/// This is defense-in-depth: the daemon-client should already unwrap,
/// but we handle it here too in case of:
/// - Direct tool execution (bypassing daemon)
/// - Future format changes
/// - Data from cached/persisted sources
pub(crate) fn unwrap_json_result(result: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(result) {
        // Handle plain JSON string: "content with \n newlines"
        if let Some(s) = v.as_str() {
            return s.to_string();
        }
        // Handle wrapped result: {"result": "content"}
        if let Some(inner) = v.get("result").and_then(|r| r.as_str()) {
            return inner.to_string();
        }
    }
    result.to_string()
}

fn count_newline_items(result: &str) -> Option<usize> {
    let newline_count = result.matches('\n').count();
    let escaped_newline_count = result.matches("\\n").count();
    let count = newline_count.max(escaped_newline_count) + 1;
    if count > 1 {
        Some(count)
    } else {
        None
    }
}

fn count_grep_matches(result: &str) -> Option<usize> {
    let count = result
        .lines()
        .filter(|l| l.contains(':') && !l.trim().is_empty())
        .count();
    if count > 0 {
        Some(count)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::render::render_to_plain_text;

    fn test_tool(name: &str, args: &str, complete: bool) -> CachedToolCall {
        let mut tool = CachedToolCall::new("tool-1", name, args);
        if complete {
            tool.mark_complete();
        }
        tool
    }

    fn test_tool_with_output(
        name: &str,
        args: &str,
        output: &str,
        complete: bool,
    ) -> CachedToolCall {
        let mut tool = CachedToolCall::new("tool-1", name, args);
        tool.append_output(output);
        if complete {
            tool.mark_complete();
        }
        tool
    }

    #[test]
    fn format_tool_args_empty() {
        assert_eq!(format_tool_args(""), "");
        assert_eq!(format_tool_args("{}"), "");
    }

    #[test]
    fn format_tool_args_json_object() {
        let args = r#"{"path": "foo.txt", "content": "hello"}"#;
        let result = format_tool_args(args);
        assert!(result.contains("path="));
        assert!(result.contains("content="));
    }

    #[test]
    fn format_tool_args_truncates_long_values() {
        let args =
            r#"{"content": "this is a very long string that should be truncated at some point"}"#;
        let result = format_tool_args(args);
        assert!(result.contains("…"));
    }

    #[test]
    fn summarize_tool_result_read_file() {
        let result = summarize_tool_result("mcp_read", "line1\nline2\nline3");
        assert!(result.is_some());
        assert!(result.unwrap().contains("lines"));
    }

    #[test]
    fn summarize_tool_result_glob() {
        let result = summarize_tool_result("mcp_glob", "file1.rs\nfile2.rs\nfile3.rs");
        assert_eq!(result, Some("3 files".to_string()));
    }

    #[test]
    fn summarize_tool_result_grep() {
        let result = summarize_tool_result("mcp_grep", "file.rs:10: match1\nfile.rs:20: match2");
        assert_eq!(result, Some("2 matches".to_string()));
    }

    #[test]
    fn summarize_tool_result_edit_success() {
        let result = summarize_tool_result("mcp_edit", "Edit applied successfully");
        assert_eq!(result, Some("applied".to_string()));
    }

    #[test]
    fn summarize_tool_result_bash_short() {
        let result = summarize_tool_result("mcp_bash", "OK");
        assert_eq!(result, Some("OK".to_string()));
    }

    #[test]
    fn summarize_tool_result_bash_long_returns_none() {
        let result = summarize_tool_result("mcp_bash", "line1\nline2\nline3\nline4");
        assert!(result.is_none());
    }

    #[test]
    fn format_output_tail_short_output() {
        let node = format_output_tail("line1\nline2", "  ");
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("line1"));
        assert!(plain.contains("line2"));
        assert!(!plain.contains("…"));
    }

    #[test]
    fn format_output_tail_truncates_long_output() {
        let node = format_output_tail("line1\nline2\nline3\nline4\nline5", "  ");
        let plain = render_to_plain_text(&node, 80);
        assert!(
            plain.contains("(2 more lines)"),
            "Should show count: {:?}",
            plain
        );
        assert!(plain.contains("line5"));
    }

    #[test]
    fn format_output_tail_count_line_has_bar_prefix() {
        let node = format_output_tail("a\nb\nc\nd\ne\nf", "  ");
        let plain = render_to_plain_text(&node, 80);
        let first_line = plain.lines().next().unwrap();
        assert!(
            first_line.contains("│"),
            "Count line should have bar: {:?}",
            first_line
        );
        assert!(
            first_line.contains("(3 more lines)"),
            "Should show count: {:?}",
            first_line
        );
        assert!(
            !first_line.contains("…"),
            "Should not have ellipsis, just parenthetical: {:?}",
            first_line
        );
    }

    #[test]
    fn summarize_read_tool_preserves_closing_bracket() {
        let result = "[Directory Context: /home/user/project]";
        let summary = summarize_tool_result("mcp_read", result);
        assert!(
            summary.as_ref().is_some_and(|s| s.ends_with(']')),
            "Should preserve closing bracket: {:?}",
            summary
        );
    }

    #[test]
    fn render_tool_call_complete() {
        let tool = test_tool_with_output("mcp_read", r#"{"path": "test.rs"}"#, "content", true);
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("✓"), "Should show checkmark: {:?}", plain);
        assert!(
            plain.contains("read"),
            "Should show tool name (without mcp_ prefix): {:?}",
            plain
        );
    }

    #[test]
    fn render_tool_call_in_progress() {
        let tool = test_tool("mcp_bash", r#"{"command": "ls"}"#, false);
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(
            plain.contains("bash"),
            "Should show tool name (without mcp_ prefix): {:?}",
            plain
        );
    }

    #[test]
    fn render_tool_call_with_error() {
        let mut tool = test_tool("mcp_bash", r#"{"command": "false"}"#, false);
        tool.set_error("Command failed with exit code 1".to_string());
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("✗"), "Should show error icon: {:?}", plain);
        assert!(
            plain.contains("Command failed"),
            "Should show error message: {:?}",
            plain
        );
    }

    #[test]
    fn render_tool_call_collapses_short_result() {
        let tool = test_tool_with_output("unknown_tool", "{}", "OK", true);
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(
            plain.contains("→ OK"),
            "Short result should collapse to one line: {:?}",
            plain
        );
    }

    #[test]
    fn format_tool_args_unicode_truncation() {
        let long_jp = "日本語".repeat(20);
        let args = format!(r#"{{"content": "{}"}}"#, long_jp);
        let result = format_tool_args(&args);
        assert!(result.contains("…"), "Should truncate: {}", result);
        assert!(!result.is_empty());
    }

    #[test]
    fn unwrap_json_result_plain_json_string() {
        let json_string = r#""total 528\ndrwxr-xr-x""#;
        let result = unwrap_json_result(json_string);
        assert_eq!(result, "total 528\ndrwxr-xr-x");
        assert!(!result.starts_with('"'));
    }

    #[test]
    fn unwrap_json_result_wrapped_object() {
        let json_obj = r#"{"result": "file contents"}"#;
        let result = unwrap_json_result(json_obj);
        assert_eq!(result, "file contents");
    }

    #[test]
    fn unwrap_json_result_plain_text() {
        let plain = "just plain text";
        let result = unwrap_json_result(plain);
        assert_eq!(result, "just plain text");
    }

    #[test]
    fn tool_result_with_json_encoded_newlines() {
        let json_result = r#""line1\nline2\nline3""#;
        let tool = test_tool_with_output("mcp_bash", r#"{"command": "ls"}"#, json_result, true);
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(
            plain.contains("│ line1") || plain.contains("→"),
            "Should decode escaped newlines and show lines: {:?}",
            plain
        );
        assert!(
            !plain.contains(r#"\n"#),
            "Should not show literal backslash-n: {:?}",
            plain
        );
    }

    #[test]
    fn tool_with_multiline_output_no_blank_line() {
        let tool = test_tool_with_output(
            "mcp_bash",
            r#"{"command": "ls"}"#,
            "line1\nline2\nline3",
            true,
        );
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        let lines: Vec<&str> = plain.lines().collect();

        assert!(lines[0].contains("✓"), "First line should have checkmark");
        if lines.len() > 1 {
            assert!(
                !lines[1].trim().is_empty(),
                "No blank line between header and output: {:?}",
                lines
            );
        }
    }

    #[test]
    fn format_output_tail_no_leading_blank() {
        let node = format_output_tail("line1\nline2\nline3", "   ");
        let plain = render_to_plain_text(&node, 80);
        let lines: Vec<&str> = plain.lines().collect();
        assert!(
            !lines.is_empty() && !lines[0].trim().is_empty(),
            "First line should not be blank: {:?}",
            lines
        );
    }

    #[test]
    fn format_tool_result_no_leading_blank() {
        let node = format_tool_result("mcp_bash", "line1\nline2\nline3");
        let plain = render_to_plain_text(&node, 80);
        let lines: Vec<&str> = plain.lines().collect();
        assert!(
            !lines.is_empty() && !lines[0].trim().is_empty(),
            "First line should not be blank: {:?}",
            lines
        );
    }
}
