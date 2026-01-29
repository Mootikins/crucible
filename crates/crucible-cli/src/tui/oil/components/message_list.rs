use std::borrow::Cow;

use crate::tui::oil::chat_app::Role;
use crate::tui::oil::component::Component;
use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::node::{
    col, row, scrollback, spinner_with_frames, styled, text, Node, BRAILLE_SPINNER_FRAMES,
};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::{colors, styles};
use crate::tui::oil::viewport_cache::{
    CachedChatItem, CachedMessage, CachedShellExecution, CachedSubagent, CachedToolCall,
    SubagentStatus,
};
use crate::tui::oil::ViewContext;
use std::time::Duration;

pub struct MessageList<'a> {
    items: &'a [&'a CachedChatItem],
    width: usize,
    last_thinking: Option<&'a ThinkingBlock>,
}

pub struct ThinkingBlock {
    pub message_id: String,
    pub content: String,
    pub token_count: usize,
}

impl<'a> MessageList<'a> {
    pub fn new(items: &'a [&'a CachedChatItem], width: usize) -> Self {
        Self {
            items,
            width,
            last_thinking: None,
        }
    }

    pub fn with_thinking(mut self, thinking: Option<&'a ThinkingBlock>) -> Self {
        self.last_thinking = thinking;
        self
    }

    fn render_item_sequence(&self) -> Node {
        let mut nodes = Vec::with_capacity(self.items.len());

        for item in self.items {
            let node = match item {
                CachedChatItem::Message(msg) => self.render_message(msg),
                CachedChatItem::ToolCall(tool) => render_tool_call(tool),
                CachedChatItem::ShellExecution(shell) => render_shell_execution(shell),
                CachedChatItem::Subagent(subagent) => render_subagent(subagent, 0),
            };
            nodes.push(node);
        }

        col(nodes)
    }

    fn render_message(&self, msg: &CachedMessage) -> Node {
        let content_node = match msg.role {
            Role::User => render_user_prompt(msg.content(), self.width),
            Role::Assistant => {
                let style = RenderStyle::natural_with_margins(self.width, Margins::assistant());
                let md_node = markdown_to_node_styled(msg.content(), style);

                let thinking_for_this_msg = self.last_thinking.filter(|tb| tb.message_id == msg.id);

                match thinking_for_this_msg {
                    Some(tb) => {
                        let thinking_node =
                            render_thinking_block(&tb.content, tb.token_count, self.width);
                        col([text(""), thinking_node, md_node, text("")])
                    }
                    None => col([text(""), md_node, text("")]),
                }
            }
            Role::System => col([
                text(""),
                styled(format!(" * {} ", msg.content()), styles::system_message()),
            ]),
        };
        scrollback(&msg.id, [content_node])
    }
}

impl Component for MessageList<'_> {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        self.render_item_sequence()
    }
}

pub fn render_user_prompt(content: &str, width: usize) -> Node {
    let top_edge = styled("▄".repeat(width), Style::new().fg(colors::INPUT_BG));
    let bottom_edge = styled("▀".repeat(width), Style::new().fg(colors::INPUT_BG));

    let prefix = " > ";
    let continuation_prefix = "   ";
    let content_width = width.saturating_sub(prefix.len() + 1);
    let lines = wrap_content(content, content_width);

    let mut rows: Vec<Node> = Vec::with_capacity(lines.len() + 3);
    rows.push(text(""));
    rows.push(top_edge);

    for (i, line) in lines.iter().enumerate() {
        let line_len = line.chars().count();
        let line_padding = " ".repeat(content_width.saturating_sub(line_len) + 1);
        let line_prefix = if i == 0 { prefix } else { continuation_prefix };
        rows.push(styled(
            format!("{}{}{}", line_prefix, line, line_padding),
            Style::new().bg(colors::INPUT_BG),
        ));
    }

    rows.push(bottom_edge);
    rows.push(text(""));
    col(rows)
}

pub fn render_thinking_block(content: &str, token_count: usize, width: usize) -> Node {
    let header = styled(
        format!("  ┌─ thinking ({} tokens)", token_count),
        styles::thinking_header(),
    );

    let display_content: Cow<'_, str> = if content.len() > 1200 {
        let start = content.len() - 1200;
        let boundary = content[start..]
            .find(char::is_whitespace)
            .map(|i| start + i + 1)
            .unwrap_or(start);
        Cow::Owned(format!("…{}", &content[boundary..]))
    } else {
        Cow::Borrowed(content)
    };

    let md_style = RenderStyle::viewport_with_margins(
        width.saturating_sub(4),
        Margins {
            left: 4,
            right: 0,
            show_bullet: false,
        },
    );
    let content_node = markdown_to_node_styled(&display_content, md_style);

    col([header, content_node, text("")])
}

fn display_tool_name(name: &str) -> &str {
    name.strip_prefix("mcp_").unwrap_or(name)
}

fn format_elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else {
        format!("{}m{}s", secs / 60, secs % 60)
    }
}

pub fn render_tool_call(tool: &CachedToolCall) -> Node {
    render_tool_call_with_frame(tool, 0)
}

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
    // No scrollback_tool wrapper - the container handles graduation at group level
    row([
        styled(" ✗ ", Style::new().fg(colors::ERROR)),
        styled(display_name, Style::new().fg(colors::TEXT_DIM)),
        styled(format!("({}) ", args_formatted), styles::dim()),
        styled(format!("→ {}", truncate_line(error, 50)), styles::error()),
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

    let arrow_suffix = if let Some(ref path) = tool.output_path {
        styled(format!("→ {}", path.display()), styles::muted())
    } else if let Some(ref s) = collapsed {
        styled(format!("→ {}", s), styles::muted())
    } else {
        Node::Empty
    };

    let header = row([
        styled(" ✓ ", Style::new().fg(colors::SUCCESS)),
        styled(display_name, Style::new().fg(colors::TEXT_DIM)),
        if args_formatted.is_empty() {
            Node::Empty
        } else if has_arrow_suffix {
            styled(format!("({}) ", args_formatted), styles::dim())
        } else {
            styled(format!("({})", args_formatted), styles::dim())
        },
        arrow_suffix,
    ]);

    let result_node = if has_arrow_suffix || result_str.is_empty() {
        Node::Empty
    } else {
        format_tool_result(&tool.name, result_str)
    };

    // No scrollback_tool wrapper - the container handles graduation at group level
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

    let header = row([
        styled(" ", Style::new()),
        spinner_with_frames(
            spinner_frame,
            Style::new().fg(colors::TEXT_DIM),
            BRAILLE_SPINNER_FRAMES,
        ),
        styled(" ", Style::new()),
        styled(display_name, Style::new().fg(colors::TEXT_DIM)),
        styled(format!("({})", args_formatted), styles::dim()),
        if show_elapsed {
            styled(format!("  {}", format_elapsed(elapsed)), styles::dim())
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

fn truncate_line(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.len() <= max {
        first_line.to_string()
    } else {
        format!("{}…", &first_line[..max.saturating_sub(1)])
    }
}

pub fn render_shell_execution(shell: &CachedShellExecution) -> Node {
    let exit_style = if shell.exit_code == 0 {
        styles::success()
    } else {
        styles::error()
    };

    let header = row([
        styled(" $ ", styles::muted()),
        styled(
            shell.command.as_ref(),
            Style::new().fg(colors::TEXT_PRIMARY),
        ),
        styled(format!("  exit {}", shell.exit_code), exit_style.dim()),
    ]);

    let tail_nodes: Vec<Node> = shell
        .output_tail
        .iter()
        .map(|line| styled(format!("   {}", line), styles::dim()))
        .collect();

    let path_node = shell
        .output_path
        .as_ref()
        .map(|p| styled(format!("   → {}", p.display()), styles::dim()))
        .unwrap_or(Node::Empty);

    let content = col(std::iter::once(header)
        .chain(tail_nodes)
        .chain(std::iter::once(path_node)));
    scrollback(&shell.id, [content])
}

pub fn render_subagent(subagent: &CachedSubagent, spinner_frame: usize) -> Node {
    let (icon, icon_style) = match subagent.status {
        SubagentStatus::Running => {
            let frame = BRAILLE_SPINNER_FRAMES[spinner_frame % BRAILLE_SPINNER_FRAMES.len()];
            (format!(" {} ", frame), Style::new().fg(colors::TEXT_ACCENT))
        }
        SubagentStatus::Completed => (" ✓ ".to_string(), Style::new().fg(colors::SUCCESS)),
        SubagentStatus::Failed => (" ✗ ".to_string(), Style::new().fg(colors::ERROR)),
    };

    let prompt_preview = truncate_line(&subagent.prompt, 60);

    let status_text = match subagent.status {
        SubagentStatus::Running => {
            let elapsed = subagent.elapsed();
            format!("  {}", format_elapsed(elapsed))
        }
        SubagentStatus::Completed => subagent
            .summary
            .as_ref()
            .map(|s| format!(" → {}", truncate_line(s, 50)))
            .unwrap_or_default(),
        SubagentStatus::Failed => subagent
            .error
            .as_ref()
            .map(|e| format!(" → {}", truncate_line(e, 50)))
            .unwrap_or_default(),
    };

    let status_style = match subagent.status {
        SubagentStatus::Running => styles::dim(),
        SubagentStatus::Completed => styles::muted(),
        SubagentStatus::Failed => styles::error(),
    };

    let header = row([
        styled(icon, icon_style),
        styled("subagent", Style::new().fg(colors::TEXT_PRIMARY)),
        styled(format!(" {}", prompt_preview), styles::muted()),
        styled(status_text, status_style),
    ]);

    scrollback(subagent.id.to_string(), [col([text(""), header])])
}

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
                                format!("\"{}…\"", truncate_chars(&collapsed, 27))
                            } else {
                                format!("\"{}\"", collapsed)
                            }
                        }
                        other => {
                            let s = other.to_string();
                            if s.chars().count() > 30 {
                                format!("{}…", truncate_chars(&s, 27))
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
        format!("{}…", truncate_chars(&oneline, 57))
    }
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

pub fn format_tool_result(name: &str, result: &str) -> Node {
    if let Some(summary) = summarize_tool_result(name, result) {
        return styled(format!("   {}", summary), styles::muted());
    }
    let inner = unwrap_json_result(result);
    format_output_tail(&inner, "   ")
}

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

pub fn format_streaming_output(output: &str) -> Node {
    let unwrapped = unwrap_json_result(output);
    format_output_tail(&unwrapped, "     ")
}

fn term_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

pub fn format_output_tail(output: &str, prefix: &str) -> Node {
    let width = term_width();
    let all_lines: Vec<&str> = output.lines().collect();
    let lines: Vec<&str> = all_lines.iter().rev().take(3).rev().copied().collect();
    let hidden_count = all_lines.len().saturating_sub(3);
    let bar_prefix = format!("{}│ ", prefix);
    let truncate_at = width.saturating_sub(bar_prefix.len() + 1);

    col(std::iter::once(if hidden_count > 0 {
        styled(
            format!("{}({} more lines)", bar_prefix, hidden_count),
            styles::tool_result(),
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
        styled(display, styles::tool_result())
    })))
}

/// Unwraps JSON-encoded strings and `{"result": "..."}` objects.
///
/// This is defense-in-depth: the daemon-client should already unwrap,
/// but we handle it here too in case of:
/// - Direct tool execution (bypassing daemon)
/// - Future format changes
/// - Data from cached/persisted sources
fn unwrap_json_result(result: &str) -> String {
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

fn wrap_content(content: &str, width: usize) -> Vec<String> {
    use textwrap::{wrap, Options, WordSplitter};

    if width == 0 {
        return vec![content.to_string()];
    }

    let options = Options::new(width).word_splitter(WordSplitter::NoHyphenation);
    wrap(content, options)
        .into_iter()
        .map(|cow| cow.into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
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
    fn render_shell_execution_success() {
        let shell =
            CachedShellExecution::new("shell-1", "echo hello", 0, vec!["hello".to_string()], None);
        let node = render_shell_execution(&shell);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("$"));
        assert!(plain.contains("echo hello"));
        assert!(plain.contains("exit 0"));
    }

    #[test]
    fn render_shell_execution_failure() {
        let shell = CachedShellExecution::new("shell-1", "false", 1, vec![], None);
        let node = render_shell_execution(&shell);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("exit 1"));
    }

    #[test]
    fn render_user_prompt_single_line() {
        let node = render_user_prompt("Hello world", 80);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains(">"));
        assert!(plain.contains("Hello world"));
    }

    #[test]
    fn render_user_prompt_multiline() {
        let node = render_user_prompt("Line one\nLine two", 80);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Line one"));
        assert!(plain.contains("Line two"));
    }

    #[test]
    fn message_list_renders_items() {
        let msg = CachedMessage::new("msg-1", Role::User, "Hello");
        let items: Vec<CachedChatItem> = vec![CachedChatItem::Message(msg)];
        let refs: Vec<&CachedChatItem> = items.iter().collect();

        let h = ComponentHarness::new(80, 24);
        let list = MessageList::new(&refs, 80);
        let node = list.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Hello"));
    }

    #[test]
    fn message_list_renders_tool_calls() {
        let tool = test_tool("test_tool", "{}", false);
        let items: Vec<CachedChatItem> = vec![CachedChatItem::ToolCall(tool)];
        let refs: Vec<&CachedChatItem> = items.iter().collect();

        let h = ComponentHarness::new(80, 24);
        let list = MessageList::new(&refs, 80);
        let node = list.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("test_tool"));
    }

    #[test]
    fn render_thinking_block_boundary_1200_chars() {
        let content_exactly_1200 = "a".repeat(1200);
        let node = render_thinking_block(&content_exactly_1200, 100, 80);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("thinking"));
        assert!(!plain.contains("…"));
    }

    #[test]
    fn render_thinking_block_over_1200_chars() {
        let content_over_1200 = "a".repeat(1201);
        let node = render_thinking_block(&content_over_1200, 100, 80);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("thinking"));
        assert!(plain.contains("…"));
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
    fn col_of_row_and_col_no_blank_between() {
        use crate::tui::oil::node::{col, row, styled};
        use crate::tui::oil::style::Style;

        let header = row([styled("header", Style::default())]);
        let result = col([
            styled("line1", Style::default()),
            styled("line2", Style::default()),
        ]);
        let combined = col([header, result]);

        let plain = render_to_plain_text(&combined, 80);
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(
            lines,
            vec!["header", "line1", "line2"],
            "Should have no blank line"
        );
    }

    #[test]
    fn nested_col_with_empty_first_no_blank_line() {
        use crate::tui::oil::node::{col, row, text, Node};
        use crate::tui::oil::render::render_to_string;

        let inner_row = row([text("header")]);
        let inner_col = col([Node::Empty, text("line1"), text("line2")]);
        let outer = col([inner_row, inner_col]);
        let outer_raw = render_to_string(&outer, 80);
        let plain = crate::tui::oil::ansi::strip_ansi(&outer_raw);
        let lines: Vec<&str> = plain.lines().collect();
        assert_eq!(
            lines,
            vec!["header", "line1", "line2"],
            "Nested col starting with Node::Empty should not add blank line"
        );
    }
}
