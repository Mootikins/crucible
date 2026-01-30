//! Message list component for rendering chat history.
//!
//! Renders a sequence of chat items including messages, tool calls,
//! shell executions, and subagent invocations.

use std::borrow::Cow;

use crate::tui::oil::chat_app::Role;
use crate::tui::oil::component::Component;
use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::node::{col, scrollback, styled, text, Node};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::viewport_cache::{CachedChatItem, CachedMessage};
use crate::tui::oil::ViewContext;

use super::shell_render::render_shell_execution;
use super::subagent_render::render_subagent;
use super::tool_render::render_tool_call;

pub struct MessageList<'a> {
    items: &'a [&'a CachedChatItem],
    width: usize,
}

impl<'a> MessageList<'a> {
    pub fn new(items: &'a [&'a CachedChatItem], width: usize) -> Self {
        Self { items, width }
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
                col([text(""), md_node, text("")])
            }
            Role::System => col([
                text(""),
                styled(
                    format!(" * {} ", msg.content()),
                    ThemeTokens::default_ref().system_message(),
                ),
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

/// Render a user prompt with styled background.
pub fn render_user_prompt(content: &str, width: usize) -> Node {
    let theme = ThemeTokens::default_ref();
    let top_edge = styled("▄".repeat(width), Style::new().fg(theme.input_bg));
    let bottom_edge = styled("▀".repeat(width), Style::new().fg(theme.input_bg));

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
            Style::new().bg(theme.input_bg),
        ));
    }

    rows.push(bottom_edge);
    rows.push(text(""));
    col(rows)
}

/// Render a thinking block with token count header.
pub fn render_thinking_block(content: &str, token_count: usize, width: usize) -> Node {
    let header = styled(
        format!("  ┌─ thinking ({} tokens)", token_count),
        ThemeTokens::default_ref().thinking_header(),
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
        use crate::tui::oil::viewport_cache::CachedToolCall;

        let tool = CachedToolCall::new("tool-1", "test_tool", "{}");
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
