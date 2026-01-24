use std::borrow::Cow;

use crate::tui::oil::chat_app::Role;
use crate::tui::oil::component::Component;
use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::node::{col, row, scrollback, styled, text, Node};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::{colors, styles};
use crate::tui::oil::viewport_cache::{
    CachedChatItem, CachedMessage, CachedShellExecution, CachedToolCall,
};
use crate::tui::oil::ViewContext;

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
        let mut prev_was_tool = false;

        for item in self.items {
            let is_tool = matches!(item, CachedChatItem::ToolCall(_));
            let node = match item {
                CachedChatItem::Message(msg) => self.render_message(msg),
                CachedChatItem::ToolCall(tool) => render_tool_call(tool, !prev_was_tool),
                CachedChatItem::ShellExecution(shell) => render_shell_execution(shell),
            };
            nodes.push(node);
            prev_was_tool = is_tool;
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

pub fn render_tool_call(tool: &CachedToolCall, first_in_sequence: bool) -> Node {
    let (status_icon, status_color) = if tool.complete {
        ("✓", colors::SUCCESS)
    } else {
        ("…", colors::TEXT_PRIMARY)
    };

    let args_formatted = format_tool_args(&tool.args);
    let result_summary = if tool.complete && !tool.result.is_empty() {
        summarize_tool_result(&tool.name, &tool.result)
    } else {
        None
    };

    let has_summary = result_summary.is_some();
    let header = if let Some(summary) = result_summary {
        row([
            styled(format!(" {} ", status_icon), Style::new().fg(status_color)),
            styled(tool.name.as_ref(), Style::new().fg(colors::TEXT_PRIMARY)),
            styled(format!("({}) ", args_formatted), styles::muted()),
            styled(format!("→ {}", summary), styles::muted()),
        ])
    } else {
        row([
            styled(format!(" {} ", status_icon), Style::new().fg(status_color)),
            styled(tool.name.as_ref(), Style::new().fg(colors::TEXT_PRIMARY)),
            styled(format!("({})", args_formatted), styles::muted()),
        ])
    };

    let result_node = if tool.result.is_empty() || has_summary {
        Node::Empty
    } else if tool.complete {
        format_tool_result(&tool.name, &tool.result)
    } else {
        format_streaming_output(&tool.result)
    };

    let content = if first_in_sequence {
        col([text(""), header, result_node])
    } else {
        col([header, result_node])
    };

    if tool.complete {
        scrollback(&tool.id, [content])
    } else {
        content
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
    format_output_tail(&inner, "   ", 77)
}

pub fn summarize_tool_result(name: &str, result: &str) -> Option<String> {
    let inner = unwrap_json_result(result);
    match name {
        "read_file" | "mcp_read" => inner
            .rfind('[')
            .map(|i| inner[i..].trim_end_matches(']').to_string())
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
    format_output_tail(output, "     ", 72)
}

pub fn format_output_tail(output: &str, prefix: &str, max_line_len: usize) -> Node {
    let all_lines: Vec<&str> = output.lines().collect();
    let lines: Vec<&str> = all_lines.iter().rev().take(3).rev().copied().collect();
    let truncated = all_lines.len() > 3;
    let truncate_at = max_line_len.saturating_sub(prefix.len() + 1);

    col(std::iter::once(if truncated {
        styled(format!("{}…", prefix), styles::muted())
    } else {
        Node::Empty
    })
    .chain(lines.iter().map(|line| {
        let display = if line.len() > truncate_at {
            format!("{}{}…", prefix, &line[..truncate_at])
        } else {
            format!("{}{}", prefix, line)
        };
        styled(display, styles::muted())
    })))
}

fn unwrap_json_result(result: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(result) {
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
        let node = format_output_tail("line1\nline2", "  ", 80);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("line1"));
        assert!(plain.contains("line2"));
        assert!(!plain.contains("…"));
    }

    #[test]
    fn format_output_tail_truncates_long_output() {
        let node = format_output_tail("line1\nline2\nline3\nline4\nline5", "  ", 80);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("…"));
        assert!(plain.contains("line5"));
    }

    #[test]
    fn render_tool_call_complete() {
        let tool = CachedToolCall {
            id: "tool-1".to_string(),
            name: "mcp_read".into(),
            args: r#"{"path": "test.rs"}"#.into(),
            result: "content".to_string(),
            complete: true,
        };
        let node = render_tool_call(&tool, true);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("✓"));
        assert!(plain.contains("mcp_read"));
    }

    #[test]
    fn render_tool_call_in_progress() {
        let tool = CachedToolCall {
            id: "tool-1".to_string(),
            name: "mcp_bash".into(),
            args: r#"{"command": "ls"}"#.into(),
            result: String::new(),
            complete: false,
        };
        let node = render_tool_call(&tool, true);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("…"));
        assert!(plain.contains("mcp_bash"));
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
        let tool = CachedToolCall {
            id: "tool-1".to_string(),
            name: "test_tool".into(),
            args: "{}".into(),
            result: String::new(),
            complete: false,
        };
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
}
