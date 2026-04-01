//! Tool call rendering component.
//!
//! Renders tool call states: running (with spinner), complete (with result summary),
//! and error (with error message).

use crucible_oil::node::{col, row, styled, Node, SpinnerNode, SpinnerStyle};
use crucible_oil::style::Style;
use crate::tui::oil::utils::{terminal_width, truncate_to_chars};
use crate::tui::oil::viewport_cache::CachedToolCall;
use crucible_oil::ansi::visible_width;
use crucible_oil::truncate_to_width;
use std::time::Duration;

/// Render a tool call with default spinner frame (0).
pub fn render_tool_call(tool: &CachedToolCall) -> Node {
    render_tool_call_with_frame(tool, 0)
}

/// Render a tool call with specified spinner frame for animation.
pub fn render_tool_call_with_frame(tool: &CachedToolCall, spinner_frame: usize) -> Node {
    if tool.superseded {
        return Node::Empty;
    }

    let display_name = display_tool_name(&tool.name);
    let auto_primary = format_primary_arg(&tool.args);
    let primary_arg: &str = tool
        .lua_primary_arg
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&auto_primary);
    let result_str = tool.result();

    let inner = if let Some(ref error) = tool.error {
        render_tool_error(tool, &display_name, primary_arg, error)
    } else if tool.complete {
        render_tool_complete(tool, &display_name, primary_arg, &result_str)
    } else {
        render_tool_running(tool, &display_name, primary_arg, &result_str, spinner_frame)
    };

    let description_node = render_tool_description(tool);
    if matches!(description_node, Node::Empty) {
        inner
    } else {
        col([inner, description_node])
    }
}

fn render_tool_description(tool: &CachedToolCall) -> Node {
    let desc = match tool.description.as_deref() {
        Some(d) if !d.is_empty() => d,
        _ => return Node::Empty,
    };
    let t = crate::tui::oil::theme::active();
    styled(
        format!("    {}", desc),
        Style::new().fg(t.resolve_color(t.colors.text_muted)).dim(),
    )
}

fn render_source_badge(tool: &CachedToolCall) -> Node {
    let source = match tool.source {
        Some(ref s) => s,
        None => return Node::Empty,
    };
    let t = crate::tui::oil::theme::active();
    styled(
        format!(" [{}]", source.label()),
        Style::new().fg(t.resolve_color(t.colors.text_muted)).dim(),
    )
}

fn render_tool_error(
    tool: &CachedToolCall,
    display_name: &str,
    primary_arg: &str,
    error: &str,
) -> Node {
    let t = crate::tui::oil::theme::active();
    let icon = format!(" {} ", t.decorations.tool_error_icon);
    let source_badge = render_source_badge(tool);
    let arg_part = if primary_arg.is_empty() {
        " ".to_string()
    } else {
        format!(" {} ", primary_arg)
    };
    let prefix_width =
        visible_width(&icon) + visible_width(display_name) + visible_width(&arg_part);
    let term_width = terminal_width();
    let remaining = term_width.saturating_sub(prefix_width + 2).max(10);
    let error_first_line = error.lines().next().unwrap_or(error);
    let error_visible = visible_width(error_first_line);
    if error_visible <= remaining {
        row([
            styled(icon, Style::new().fg(t.resolve_color(t.colors.error))),
            styled(
                display_name,
                Style::new().fg(t.resolve_color(t.colors.text_dim)),
            ),
            source_badge,
            styled(
                arg_part,
                Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
            ),
            styled(
                format!("\u{2192} {}", error_first_line),
                Style::new().fg(t.resolve_color(t.colors.error)).bold(),
            ),
        ])
    } else {
        let header = row([
            styled(icon, Style::new().fg(t.resolve_color(t.colors.error))),
            styled(
                display_name,
                Style::new().fg(t.resolve_color(t.colors.text_dim)),
            ),
            source_badge,
            styled(
                arg_part,
                Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
            ),
        ]);
        let error_node = styled(
            format!("  \u{2192} {}", error_first_line),
            Style::new().fg(t.resolve_color(t.colors.error)).bold(),
        );
        col([header, error_node])
    }
}

fn render_tool_complete(
    tool: &CachedToolCall,
    display_name: &str,
    primary_arg: &str,
    result_str: &str,
) -> Node {
    let result_summary = if !result_str.is_empty() {
        summarize_tool_result(&tool.name, result_str)
    } else {
        None
    };

    let collapsed = collapse_result(&tool.name, result_str, result_summary.as_deref());
    let has_arrow_suffix = collapsed.is_some();

    let t = crate::tui::oil::theme::active();
    let arrow_suffix = if let Some(ref s) = collapsed {
        styled(
            format!("→ {}", s),
            Style::new().fg(t.resolve_color(t.colors.text_muted)),
        )
    } else {
        Node::Empty
    };

    let source_badge = render_source_badge(tool);
    let arg_node = if primary_arg.is_empty() {
        if has_arrow_suffix {
            styled(" ", Style::new())
        } else {
            Node::Empty
        }
    } else if has_arrow_suffix {
        styled(
            format!(" {} ", primary_arg),
            Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
        )
    } else {
        styled(
            format!(" {}", primary_arg),
            Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
        )
    };
    let header = row([
        styled(
            format!(" {} ", t.decorations.tool_success_icon),
            Style::new().fg(t.resolve_color(t.colors.success)),
        ),
        styled(
            display_name,
            Style::new().fg(t.resolve_color(t.colors.text_dim)),
        ),
        source_badge,
        arg_node,
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
    primary_arg: &str,
    result_str: &str,
    spinner_frame: usize,
) -> Node {
    let elapsed = tool.elapsed();
    let show_elapsed = elapsed >= Duration::from_secs(2);

    let t = crate::tui::oil::theme::active();
    let spinner = Node::Spinner(
        SpinnerNode::new(spinner_frame)
            .style(Style::new().fg(t.resolve_color(t.colors.text_dim)))
            .style_variant(SpinnerStyle::Braille),
    );
    let source_badge = render_source_badge(tool);
    let arg_node = if primary_arg.is_empty() {
        Node::Empty
    } else {
        styled(
            format!(" {}", primary_arg),
            Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
        )
    };
    let header = row([
        styled(" ", Style::new()),
        spinner,
        styled(" ", Style::new()),
        styled(
            display_name,
            Style::new().fg(t.resolve_color(t.colors.text_dim)),
        ),
        source_badge,
        arg_node,
        if show_elapsed {
            styled(
                format!("  {}", format_elapsed(elapsed)),
                Style::new().fg(t.resolve_color(t.colors.text_dim)).dim(),
            )
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

fn display_tool_name(name: &str) -> String {
    // Use humanize_tool_title from crucible-acp for consistent formatting
    crucible_acp::streaming::humanize_tool_title(name)
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

const PRIMARY_ARG_KEYS: &[&str] = &[
    "path",
    "file_path",
    "filePath",
    "query",
    "command",
    "url",
    "pattern",
    "code",
    "content",
    "text",
    "prompt",
];

pub fn format_primary_arg(args: &str) -> String {
    if args.is_empty() || args == "{}" {
        return String::new();
    }

    let obj = serde_json::from_str::<serde_json::Value>(args)
        .ok()
        .and_then(|v| v.as_object().cloned());
    let obj = match obj {
        Some(o) => o,
        None => return String::new(),
    };

    let value = PRIMARY_ARG_KEYS
        .iter()
        .find_map(|key| obj.get(*key))
        .or_else(|| obj.values().next());

    let Some(value) = value else {
        return String::new();
    };

    let text = match value {
        serde_json::Value::String(s) => s.replace('\n', " ").replace('\r', ""),
        other => other.to_string(),
    };

    if text.chars().count() > 40 {
        format!("{}…", truncate_to_chars(&text, 39, false))
    } else {
        text
    }
}

/// Format tool result for display.
pub fn format_tool_result(name: &str, result: &str) -> Node {
    if let Some(summary) = summarize_tool_result(name, result) {
        let t = crate::tui::oil::theme::active();
        return styled(
            format!("   {}", summary),
            Style::new().fg(t.resolve_color(t.colors.text_muted)),
        );
    }
    let inner = unwrap_json_result(result);
    format_output_tail(&inner, "   ")
}

/// Summarize tool result into a short string.
pub fn summarize_tool_result(name: &str, result: &str) -> Option<String> {
    let inner = unwrap_json_result(result);
    match name {
        "read_file" | "mcp_read" => {
            // Extract short bracketed metadata (e.g., "[Directory Context: ...]") if present,
            // but not spill references or long content
            let bracket_summary = inner.rfind('[').and_then(|i| {
                let bracket = &inner[i..];
                if bracket.len() <= 60 && !bracket.contains("$CRU_SESSION_DIR") {
                    Some(bracket.to_string())
                } else {
                    None
                }
            });
            bracket_summary.or_else(|| Some(format!("{} lines", inner.lines().count())))
        }
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
    const MAX_TAIL: usize = 3;

    let width = terminal_width();
    let all_lines: Vec<&str> = output.lines().collect();
    let t = crate::tui::oil::theme::active();
    let bar_prefix = format!("{}{} ", prefix, t.decorations.separator_char);
    let truncate_at = width.saturating_sub(visible_width(&bar_prefix) + 1);
    let dim_style = Style::new().fg(t.resolve_color(t.colors.text_dim));

    let hidden_count = all_lines.len().saturating_sub(MAX_TAIL);
    let visible_lines = &all_lines[hidden_count..];

    let indicator = if hidden_count > 0 {
        styled(
            format!("{}({} more lines)", bar_prefix, hidden_count),
            dim_style,
        )
    } else {
        Node::Empty
    };

    let line_nodes = visible_lines.iter().map(|line| {
        let display = if visible_width(line) > truncate_at {
            format!(
                "{}{}…",
                bar_prefix,
                truncate_to_width(line, truncate_at, false)
            )
        } else {
            format!("{}{}", bar_prefix, line)
        };
        styled(display, dim_style)
    });

    col(std::iter::once(indicator).chain(line_nodes))
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
    (count > 1).then_some(count)
}

fn count_grep_matches(result: &str) -> Option<usize> {
    let count = result
        .lines()
        .filter(|l| l.contains(':') && !l.trim().is_empty())
        .count();
    (count > 0).then_some(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::render::render_to_plain_text;

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
    fn tool_result_bounded_overflow_indicator() {
        let long_output = (1..=10)
            .map(|i| format!("line{}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let node = format_output_tail(&long_output, "   ");
        let plain = render_to_plain_text(&node, 80);
        assert!(
            plain.contains("(7 more lines)"),
            "Long output should show overflow indicator: {:?}",
            plain
        );
        assert!(
            plain.contains("line8") && plain.contains("line9") && plain.contains("line10"),
            "Should show last 3 lines: {:?}",
            plain
        );
    }

    #[test]
    fn tool_result_short_no_cap() {
        let short_output = "line1\nline2\nline3";
        let node = format_output_tail(short_output, "   ");
        let plain = render_to_plain_text(&node, 80);
        assert!(
            !plain.contains("more lines"),
            "Short output should not show indicator: {:?}",
            plain
        );
        assert!(
            plain.contains("line1") && plain.contains("line2") && plain.contains("line3"),
            "All lines should be visible: {:?}",
            plain
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
            plain.contains("Read"),
            "Should show tool name (title-cased, without mcp_ prefix): {:?}",
            plain
        );
    }

    #[test]
    fn render_tool_call_in_progress() {
        let tool = test_tool("mcp_bash", r#"{"command": "ls"}"#, false);
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(
            plain.contains("Bash"),
            "Should show tool name (title-cased, without mcp_ prefix): {:?}",
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
    #[test]
    fn error_message_uses_terminal_width_not_hardcoded() {
        // Test that error messages respect terminal width, not hardcoded 50 chars
        let mut tool = test_tool("mcp_bash", r#"{"command": "test"}"#, false);
        let long_error = "a".repeat(120); // 120-char error message
        tool.set_error(long_error.clone());

        // Render at width=120 (wide terminal)
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 120);

        // The full error should be visible at width=120
        // With the bug (hardcoded 50), the error is truncated to 50 chars + ellipsis
        // With the fix, it should use the terminal width (120) and show the full error
        // Assert: the full 120-char error appears in output (not truncated to 50)
        assert!(
            plain.contains(&"a".repeat(100)),
            "Full error should be visible at width=120 (not truncated to 50): {}",
            plain
        );
    }

    #[test]
    fn error_message_fits_within_terminal_width() {
        // Test that error messages are not truncated to hardcoded 50 at width=80
        let mut tool = test_tool("mcp_bash", r#"{"command": "test"}"#, false);
        let long_error = "Connection failed: ".to_string() + &"x".repeat(100);
        tool.set_error(long_error.clone());

        // Render at width=80
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);

        // The error should NOT be truncated to hardcoded 50 chars
        // At width=80, we have room for more than 50 chars
        // So the error should show more than 50 chars (or the full error if it fits)
        // With the bug, it's truncated to 50 + ellipsis
        // With the fix, it should use the terminal width (80)
        assert!(
            plain.contains(&"x".repeat(50)),
            "Error should show more than 50 chars at width=80 (not hardcoded truncation): {}",
            plain
        );
    }

    #[test]
    fn error_with_cjk_no_panic() {
        // Test that CJK error messages don't panic and are not truncated to hardcoded 50
        let mut tool = test_tool("mcp_bash", r#"{"command": "test"}"#, false);
        let cjk_error =
            "错误：连接超时，请检查网络设置并重试操作。这是一个很长的错误消息用于测试。";
        tool.set_error(cjk_error.to_string());

        // Render at width=80 — should not panic
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);

        // Verify every line fits within width
        for line in plain.lines() {
            let width = crucible_oil::ansi::visible_width(line);
            assert!(
                width <= 80,
                "CJK line exceeds terminal width (80): {} chars: {}",
                width,
                line
            );
        }

        // Verify the full CJK error is visible (not truncated to hardcoded 50)
        // Extract the error portion (after the arrow) and check it's longer than 50 chars
        let error_line = plain.lines().find(|l: &&str| l.contains("→")).unwrap_or("");
        let error_portion = error_line.split("→").nth(1).unwrap_or("");
        let error_visible_width = crucible_oil::ansi::visible_width(error_portion);
        assert!(
            error_visible_width > 50,
            "CJK error should show more than 50 chars (not hardcoded truncation). Got width: {}: {}",
            error_visible_width,
            plain
        );
    }

    #[test]
    fn short_error_fully_visible_at_wide_terminal() {
        let mut tool = test_tool("mcp_bash", r#"{"command": "test"}"#, false);
        let error = "Connection refused: port 8080 is already in use by another process running on this machine";
        tool.set_error(error.to_string());

        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 120);

        assert!(
            plain.contains("Connection refused"),
            "Error start should be visible: {}",
            plain
        );
        assert!(
            plain.contains("running on this machine"),
            "Error end should be visible (not truncated to 50): {}",
            plain
        );
    }

    #[test]
    fn format_primary_arg_empty() {
        assert_eq!(format_primary_arg(""), "");
        assert_eq!(format_primary_arg("{}"), "");
    }

    #[test]
    fn format_primary_arg_path() {
        let args = r#"{"path": "src/lib.rs"}"#;
        assert_eq!(format_primary_arg(args), "src/lib.rs");
    }

    #[test]
    fn format_primary_arg_file_path_camel() {
        let args = r#"{"filePath": "/home/user/test.rs"}"#;
        assert_eq!(format_primary_arg(args), "/home/user/test.rs");
    }

    #[test]
    fn format_primary_arg_command() {
        let args = r#"{"command": "ls -la", "timeout": 5000}"#;
        assert_eq!(format_primary_arg(args), "ls -la");
    }

    #[test]
    fn format_primary_arg_query() {
        let args = r#"{"query": "auth patterns", "limit": 10}"#;
        assert_eq!(format_primary_arg(args), "auth patterns");
    }

    #[test]
    fn format_primary_arg_priority_over_first_key() {
        let args = r#"{"limit": 10, "path": "src/main.rs"}"#;
        assert_eq!(format_primary_arg(args), "src/main.rs");
    }

    #[test]
    fn format_primary_arg_fallback_to_first_value() {
        let args = r#"{"repo": "crucible"}"#;
        assert_eq!(format_primary_arg(args), "crucible");
    }

    #[test]
    fn format_primary_arg_truncates_long_value() {
        let long_path = "a".repeat(60);
        let args = format!(r#"{{"path": "{}"}}"#, long_path);
        let result = format_primary_arg(&args);
        assert!(result.contains("…"), "Should truncate: {}", result);
        assert!(result.chars().count() <= 41);
    }

    #[test]
    fn format_primary_arg_non_string_value() {
        let args = r#"{"count": 42}"#;
        assert_eq!(format_primary_arg(args), "42");
    }

    #[test]
    fn compact_read_file_shows_path() {
        let tool = test_tool_with_output("mcp_read", r#"{"path": "src/lib.rs"}"#, "content", true);
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("✓"), "Should show checkmark: {:?}", plain);
        assert!(plain.contains("Read"), "Should show tool name: {:?}", plain);
        assert!(
            plain.contains("src/lib.rs"),
            "Should show path inline: {:?}",
            plain
        );
        assert!(
            !plain.contains("path="),
            "Should NOT show key=value format: {:?}",
            plain
        );
        assert!(
            !plain.contains('(') || !plain.contains(')'),
            "Should NOT have parens around args: {:?}",
            plain
        );
    }

    #[test]
    fn compact_bash_shows_command() {
        let tool =
            test_tool_with_output("mcp_bash", r#"{"command": "ls -la"}"#, "file1\nfile2", true);
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Bash"), "Should show tool name: {:?}", plain);
        assert!(
            plain.contains("ls -la"),
            "Should show command inline: {:?}",
            plain
        );
        assert!(
            !plain.contains("command="),
            "Should NOT show key=value: {:?}",
            plain
        );
    }

    #[test]
    fn compact_no_args_no_parens() {
        let tool = test_tool_with_output("get_kiln_info", "{}", "kiln data", true);
        let node = render_tool_call(&tool);
        let plain = render_to_plain_text(&node, 80);
        assert!(
            plain.contains("Get Kiln Info"),
            "Should show tool name: {:?}",
            plain
        );
        assert!(
            !plain.contains("()"),
            "Should NOT have empty parens: {:?}",
            plain
        );
    }

    #[test]
    fn summarize_read_file_counts_lines_correctly() {
        // read_file results should show actual line count, not "1 lines"
        let content = "line1\nline2\nline3\nline4\nline5";
        let result = summarize_tool_result("read_file", content);
        assert_eq!(result, Some("5 lines".to_string()));
    }

    #[test]
    fn summarize_read_file_does_not_extract_spill_reference_as_summary() {
        // If a spill reference somehow gets to summarize, it should not be shown as-is
        let spill_ref = "[200 lines, 15KB — full output in $CRU_SESSION_DIR/tools/read-file-1.txt]";
        let result = summarize_tool_result("read_file", spill_ref);
        // Should not contain the full spill path
        assert!(
            !result
                .as_ref()
                .is_some_and(|s| s.contains("$CRU_SESSION_DIR")),
            "Should not show spill path in summary: {:?}",
            result
        );
    }

    #[test]
    fn summarize_bash_spill_reference_not_shown_raw() {
        let spill_ref = "[500 lines, 25KB — full output in $CRU_SESSION_DIR/tools/bash-1.txt]";
        let result = summarize_tool_result("bash", spill_ref);
        // Spill references are multi-line or >60 chars, so bash should return None
        assert!(
            result.is_none() || !result.as_ref().unwrap().contains("$CRU_SESSION_DIR"),
            "Bash spill ref should not be shown as summary: {:?}",
            result
        );
    }
}
