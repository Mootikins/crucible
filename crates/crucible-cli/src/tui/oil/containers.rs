//! Chat node model for viewport content.
//!
//! Each `ChatNode` is a graduation unit with explicit lifecycle state.
//! `render_chat_node()` produces Node trees. Graduated nodes are
//! removed from the list and written to scrollback.
//!
//! Spacing: uniform `gap(1)` between all nodes.

use crucible_oil::node::{col, styled, Node};
use crucible_oil::planning::Graduation;
use crucible_oil::style::{Gap, Padding, Style};
use unicode_width::UnicodeWidthStr;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::components::thinking_component::ThinkingComponent;
use crate::tui::oil::components::{render_shell_execution, render_subagent, render_tool_call_with_frame};
use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::render_state::RenderState;
use crate::tui::oil::utils::wrap_words;
use crate::tui::oil::viewport_cache::{CachedShellExecution, CachedSubagent, CachedToolCall};

// ─── Types ──────────────────────────────────────────────────────────────────

/// A chat node — the graduation unit and rendering primitive.
#[derive(Debug, Clone)]
pub enum ChatNode {
    UserMessage { text: String },
    AssistantResponse { text: String, thinking: Vec<ThinkingComponent>, complete: bool },
    ToolGroup { tools: Vec<CachedToolCall> },
    SubagentTask { agent: CachedSubagent },
    ShellExecution { shell: CachedShellExecution },
    SystemMessage { text: String },
}

impl ChatNode {
    pub fn is_complete(&self) -> bool {
        match self {
            Self::UserMessage { .. } | Self::SystemMessage { .. } | Self::ShellExecution { .. } => true,
            Self::AssistantResponse { complete, .. } => *complete,
            Self::ToolGroup { tools } => tools.iter().all(|t| t.complete),
            Self::SubagentTask { agent } => agent.is_terminal(),
        }
    }
}

// ─── Top-level renderer ─────────────────────────────────────────────────────

/// Render a chat node. `is_continuation` derived from preceding node.
pub fn render_chat_node(node: &ChatNode, prev: Option<&ChatNode>, ctx: &ViewContext<'_>) -> Node {
    match node {
        ChatNode::UserMessage { text } => render_user_message(text, ctx.width()),
        ChatNode::AssistantResponse { text, thinking, complete } => {
            let is_continuation = matches!(
                prev,
                Some(ChatNode::ToolGroup { .. } | ChatNode::SubagentTask { .. } | ChatNode::ShellExecution { .. })
            );
            render_assistant_response(text, thinking, is_continuation, *complete, ctx)
        }
        ChatNode::ToolGroup { tools } => render_tool_group(tools, ctx.spinner_frame, ctx.width()),
        ChatNode::SubagentTask { agent } => render_subagent_task(agent, ctx.spinner_frame, ctx.width()),
        ChatNode::ShellExecution { shell } => render_shell(shell),
        ChatNode::SystemMessage { text } => render_system_message(text),
    }
}

// ─── Render functions ───────────────────────────────────────────────────────

/// User message with colored top/bottom bars.
///
/// ```text
/// ▄▄▄▄▄▄▄▄▄▄▄▄ (user_message color background)
///  > user text
/// ▀▀▀▀▀▀▀▀▀▀▀▀ (user_message color background)
/// ```
fn render_user_message(content: &str, width: usize) -> Node {
    let t = crate::tui::oil::theme::active();
    let bg = t.resolve_color(t.colors.background);

    let prefix = " > ";
    let continuation_prefix = "   ";
    let content_width = width.saturating_sub(prefix.len() + 1);
    let lines = wrap_words(content, content_width);

    let top_edge = styled(
        t.decorations.half_block_bottom.to_string().repeat(width),
        Style::new().fg(bg),
    );
    let bottom_edge = styled(
        t.decorations.half_block_top.to_string().repeat(width),
        Style::new().fg(bg),
    );

    let mut rows: Vec<Node> = Vec::with_capacity(lines.len() + 2);
    rows.push(top_edge);

    for (i, line) in lines.iter().enumerate() {
        let line_len = line.width();
        let line_padding = " ".repeat(content_width.saturating_sub(line_len) + 1);
        let line_prefix = if i == 0 { prefix } else { continuation_prefix };
        rows.push(styled(
            format!("{}{}{}", line_prefix, line, line_padding),
            Style::new().bg(bg),
        ));
    }

    rows.push(bottom_edge);
    col(rows)
}

/// Assistant response with optional thinking blocks and markdown content.
fn render_assistant_response(
    content: &str,
    thinking: &[ThinkingComponent],
    is_continuation: bool,
    is_complete: bool,
    ctx: &ViewContext<'_>,
) -> Node {
    let render_state = RenderState {
        terminal_width: ctx.terminal_size.0,
        spinner_frame: ctx.spinner_frame,
        show_thinking: ctx.show_thinking,
    };

    let has_thinking = !thinking.is_empty();
    let margins = if is_continuation || has_thinking {
        Margins::assistant_continuation()
    } else {
        Margins::assistant()
    };

    let mut items: Vec<Node> = Vec::new();

    // Thinking renders when: text has started, container is complete, or graduated.
    // While actively streaming with no text yet, the turn indicator shows
    // "◐ Thinking… (N words)" — content stays empty to avoid duplication.
    let thinking_finalized = !content.is_empty() || is_complete;
    for tc in thinking {
        if tc.is_graduated() || thinking_finalized {
            let node = tc.render(&render_state, true);
            if !matches!(node, Node::Empty) {
                items.push(node);
            }
        }
    }

    // Then markdown content
    if !content.is_empty() {
        let style = RenderStyle::natural_with_margins(ctx.width(), margins);
        let md_node = markdown_to_node_styled(content, style);
        items.push(md_node);
    }

    match items.len() {
        0 => Node::Empty,
        1 => items.pop().unwrap(),
        _ => col(items).gap(Gap::row(1)),
    }
}

/// Tool group: renders each tool via the existing tool renderer.
fn render_tool_group(tools: &[CachedToolCall], spinner_frame: usize, width: usize) -> Node {
    let items: Vec<Node> = tools
        .iter()
        .map(|tool| render_tool_call_with_frame(tool, spinner_frame, width))
        .filter(|n| !matches!(n, Node::Empty))
        .collect();

    match items.len() {
        0 => Node::Empty,
        1 => items.into_iter().next().unwrap(),
        _ => col(items).gap(Gap::row(0)),
    }
}

/// Subagent task: delegates to existing subagent renderer.
fn render_subagent_task(agent: &CachedSubagent, spinner_frame: usize, width: usize) -> Node {
    render_subagent(agent, spinner_frame, width)
}

/// Shell execution: delegates to existing shell renderer.
fn render_shell(shell: &CachedShellExecution) -> Node {
    render_shell_execution(shell)
}

/// System message: italicized, muted, with asterisk prefix.
fn render_system_message(content: &str) -> Node {
    let t = crate::tui::oil::theme::active();
    styled(
        format!(" * {} ", content),
        Style::new()
            .fg(t.resolve_color(t.colors.system_message))
            .italic(),
    )
}

// ─── ContainerList ──────────────────────────────────────────────────────────

/// Ordered list of chat nodes with graduation support.
///
/// `drain_completed()` removes completed nodes from the front and
/// returns a `Graduation` node tree for scrollback output.
pub struct ContainerList {
    nodes: Vec<ChatNode>,
    turn_active: bool,
    has_graduated: bool,
    /// Thinking content that hasn't been attached to an AR yet.
    /// Flushed into an AR when text, a tool call, or completion arrives.
    pending_thinking: Option<ThinkingComponent>,
}

impl ContainerList {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            turn_active: false,
            has_graduated: false,
            pending_thinking: None,
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.has_graduated = false;
        self.turn_active = false;
        self.pending_thinking = None;
    }

    pub fn nodes(&self) -> &[ChatNode] {
        &self.nodes
    }

    /// Whether the viewport needs a leading blank line for cross-batch spacing
    /// (graduated content above isn't tight with the first viewport node).
    pub fn needs_cross_batch_gap(&self) -> bool {
        self.has_graduated && !self.nodes.is_empty()
    }

    pub fn is_streaming(&self) -> bool {
        self.turn_active
    }

    /// Word count of pending (unbuffered) thinking, if any.
    pub fn pending_thinking_words(&self) -> Option<usize> {
        self.pending_thinking.as_ref().map(|tc| tc.word_count())
    }

    pub fn has_graduated(&self) -> bool {
        self.has_graduated
    }

    // ─── Mutations ──────────────────────────────────────────────────────

    pub fn add_user_message(&mut self, content: String) {
        self.nodes.push(ChatNode::UserMessage { text: content });
    }

    /// Flush pending thinking into the trailing AR (creating one if needed).
    fn flush_pending_thinking(&mut self) {
        if let Some(tc) = self.pending_thinking.take() {
            if !matches!(self.nodes.last(), Some(ChatNode::AssistantResponse { .. })) {
                self.nodes.push(ChatNode::AssistantResponse {
                    text: String::new(),
                    thinking: Vec::new(),
                    complete: false,
                });
            }
            if let Some(ChatNode::AssistantResponse { thinking, .. }) = self.nodes.last_mut() {
                thinking.push(tc);
            }
        }
    }

    /// Ensure there's an AssistantResponse at the end. Creates one if needed.
    /// Flushes any pending thinking first.
    pub fn start_assistant_response(&mut self) {
        self.flush_pending_thinking();
        if !matches!(self.nodes.last(), Some(ChatNode::AssistantResponse { .. })) {
            self.nodes.push(ChatNode::AssistantResponse {
                text: String::new(),
                thinking: Vec::new(),
                complete: false,
            });
        }
    }

    /// Append text to the current AssistantResponse. Creates one if needed.
    pub fn append_text(&mut self, delta: &str) {
        self.start_assistant_response();
        if let Some(ChatNode::AssistantResponse { text, .. }) = self.nodes.last_mut() {
            text.push_str(delta);
        }
    }

    /// Append thinking content.
    ///
    /// If a trailing AR exists, appends directly to it (thinking belongs
    /// to the current response). Otherwise buffers in `pending_thinking`
    /// so we don't create phantom AR nodes that graduate as empty batches.
    pub fn append_thinking(&mut self, delta: &str) {
        if let Some(ChatNode::AssistantResponse { thinking, .. }) = self.nodes.last_mut() {
            if thinking.is_empty() {
                thinking.push(ThinkingComponent::new(String::new()));
            }
            thinking.last_mut().unwrap().append(delta);
        } else {
            self.pending_thinking
                .get_or_insert_with(|| ThinkingComponent::new(String::new()))
                .append(delta);
        }
    }

    /// Add a tool call. Groups into an existing trailing ToolGroup if present,
    /// otherwise creates a new one.
    pub fn add_tool_call(&mut self, tool: CachedToolCall) {
        self.flush_pending_thinking();

        tracing::debug!(
            tool_name = %tool.name,
            node_count = self.nodes.len(),
            "add_tool_call"
        );

        // First, mark any trailing AssistantResponse as complete
        if let Some(ChatNode::AssistantResponse { complete, .. }) = self.nodes.last_mut() {
            if !*complete {
                tracing::debug!("marking trailing AR complete before tool");
                *complete = true;
            }
        }

        // Group into existing ToolGroup or create new one
        if let Some(ChatNode::ToolGroup { tools }) = self.nodes.last_mut() {
            tracing::debug!("appending to existing ToolGroup");
            tools.push(tool);
        } else {
            tracing::debug!("creating new ToolGroup");
            self.nodes.push(ChatNode::ToolGroup { tools: vec![tool] });
        }
    }

    /// Update a tool within the most recent ToolGroup by name and optional call_id.
    pub fn update_tool(&mut self, name: &str, call_id: Option<&str>, f: impl FnOnce(&mut CachedToolCall)) {
        // Search backwards for a ToolGroup containing this tool
        for node in self.nodes.iter_mut().rev() {
            if let ChatNode::ToolGroup { tools } = node {
                // Match by call_id first, then by name
                let found = if let Some(cid) = call_id {
                    tools.iter_mut().rev().find(|t| {
                        t.call_id.as_deref() == Some(cid)
                    })
                } else {
                    tools.iter_mut().rev().find(|t| t.name.as_ref() == name)
                };
                if let Some(tool) = found {
                    f(tool);
                    return;
                }
            }
        }
        tracing::debug!(name = %name, call_id = ?call_id, "tool update for unknown tool (already graduated or never received)");
    }

    pub fn add_agent_task(&mut self, agent: CachedSubagent) {
        self.nodes.push(ChatNode::SubagentTask { agent });
    }

    pub fn update_agent_task(&mut self, agent_id: &str, f: impl FnOnce(&mut CachedSubagent)) {
        for node in self.nodes.iter_mut().rev() {
            if let ChatNode::SubagentTask { agent } = node {
                if agent.id.as_ref() == agent_id {
                    f(agent);
                    return;
                }
            }
        }
        tracing::debug!(agent_id = %agent_id, "agent task update for unknown agent (already graduated or never received)");
    }

    pub fn add_shell_execution(&mut self, shell: CachedShellExecution) {
        self.nodes.push(ChatNode::ShellExecution { shell });
    }

    pub fn add_system_message(&mut self, content: String) {
        self.nodes.push(ChatNode::SystemMessage { text: content });
    }

    /// Mark the turn as complete: sets turn_active = false and marks
    /// the trailing AssistantResponse as Complete.
    pub fn complete_response(&mut self) {
        self.flush_pending_thinking();
        self.turn_active = false;
        if let Some(ChatNode::AssistantResponse { complete, .. }) = self.nodes.last_mut() {
            *complete = true;
        }
    }

    /// Cancel streaming: marks all streaming nodes as complete.
    pub fn cancel_streaming(&mut self) {
        self.flush_pending_thinking();
        self.turn_active = false;
        for node in &mut self.nodes {
            if let ChatNode::AssistantResponse { complete, .. } = node {
                *complete = true;
            }
        }
    }

    pub fn mark_turn_active(&mut self) {
        self.turn_active = true;
    }

    // ─── Graduation ─────────────────────────────────────────────────────

    /// Whether the node at `index` is ready for graduation.
    ///
    /// A node graduates when:
    /// - The turn is over (`!turn_active`), OR
    /// - A successor node exists (the node is no longer the tail), OR
    /// - The node is a self-graduating type (UserMessage, SystemMessage, etc.)
    /// - The node is a complete AssistantResponse
    ///
    /// ToolGroups and SubagentTasks never graduate during streaming — they
    /// stay in viewport until superseded or turn ends.
    fn is_graduatable(&self, index: usize) -> bool {
        if !self.turn_active {
            return true;
        }
        // During streaming, ToolGroups never graduate — more tools may
        // arrive and they must stay in one group to avoid cross-batch gaps.
        if matches!(&self.nodes[index], ChatNode::ToolGroup { .. }) {
            return false;
        }
        if index + 1 < self.nodes.len() {
            return true;
        }
        // Last node, turn active: only graduate types that explicitly complete
        match &self.nodes[index] {
            ChatNode::UserMessage { .. } | ChatNode::SystemMessage { .. } | ChatNode::ShellExecution { .. } => true,
            ChatNode::AssistantResponse { complete, .. } => *complete,
            ChatNode::SubagentTask { .. } | ChatNode::ToolGroup { .. } => false,
        }
    }

    /// Drain completed nodes from the front and return a graduation
    /// node tree for scrollback output.
    ///
    /// Graduated thinking blocks are collapsed (via `ThinkingComponent::graduate()`).
    pub fn drain_completed(&mut self, ctx: &ViewContext<'_>) -> Option<Graduation> {
        let mut rendered: Vec<Node> = Vec::new();
        let mut prev: Option<ChatNode> = None;

        while !self.nodes.is_empty() && self.is_graduatable(0) {
            let mut node = self.nodes.remove(0);
            tracing::debug!("graduating node");

            // Graduate thinking components so they render collapsed
            if let ChatNode::AssistantResponse { thinking, .. } = &mut node {
                for tc in thinking.iter_mut() {
                    tc.graduate();
                }
            }

            rendered.push(render_chat_node(&node, prev.as_ref(), ctx));
            prev = Some(node);
        }

        if rendered.is_empty() {
            return None;
        }

        let top_margin = if self.has_graduated { 1 } else { 0 };
        self.has_graduated = true;

        let width = ctx.terminal_size.0;
        let inner = col(rendered).gap(Gap::row(1))
            .with_margin(Padding { top: top_margin, ..Padding::all(0) });
        let node = col([inner]);

        Some(Graduation { node, width })
    }
}

impl Default for ContainerList {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::focus::FocusContext;
    use crucible_oil::render::render_to_plain_text;

    fn drain(list: &mut ContainerList) -> Option<Graduation> {
        let focus = FocusContext::default();
        let ctx = ViewContext::new(&focus);
        list.drain_completed(&ctx)
    }

    fn drain_with_thinking(list: &mut ContainerList) -> Option<Graduation> {
        let focus = FocusContext::default();
        let mut ctx = ViewContext::new(&focus);
        ctx.show_thinking = true;
        list.drain_completed(&ctx)
    }

    #[test]
    fn empty_list_drains_nothing() {
        let mut list = ContainerList::new();
        assert!(drain(&mut list).is_none());
    }

    #[test]
    fn user_message_graduates_immediately() {
        let mut list = ContainerList::new();
        list.add_user_message("hello".into());
        assert_eq!(list.len(), 1);

        let grad = drain(&mut list);
        assert!(grad.is_some());
        assert!(list.is_empty());

        let plain = render_to_plain_text(&grad.unwrap().node, 80);
        assert!(plain.contains("hello"));
    }

    #[test]
    fn system_message_graduates_immediately() {
        let mut list = ContainerList::new();
        list.add_system_message("Session started".into());

        let grad = drain(&mut list);
        assert!(grad.is_some());

        let plain = render_to_plain_text(&grad.unwrap().node, 80);
        assert!(plain.contains("Session started"));
    }

    #[test]
    fn streaming_assistant_does_not_graduate_while_turn_active() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_text("hello world");

        // Still streaming, nothing follows — should not graduate
        assert!(drain(&mut list).is_none());
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn assistant_graduates_after_complete() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_text("done");
        list.complete_response();

        let grad = drain(&mut list);
        assert!(grad.is_some());
        assert!(list.is_empty());
    }

    #[test]
    fn assistant_graduates_when_followed_by_tool() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_text("let me check");

        // Tool call follows — assistant should graduate
        list.add_tool_call(CachedToolCall::new("t1", "read_file", "{}"));

        let grad = drain(&mut list);
        assert!(grad.is_some());
        // Tool group remains (streaming)
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn tool_group_graduates_when_turn_ends() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "read_file", "{}"));

        // Turn active, tool not followed — should not graduate
        assert!(drain(&mut list).is_none());

        // End the turn
        list.complete_response();

        let grad = drain(&mut list);
        assert!(grad.is_some());
        assert!(list.is_empty());
    }

    #[test]
    fn tool_group_stays_in_viewport_mid_turn() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "read_file", "{}"));

        // Mark tool complete via update
        list.update_tool("read_file", None, |t| {
            t.mark_complete();
        });

        // Still mid-turn, tool group should NOT graduate
        assert!(drain(&mut list).is_none());
    }

    #[test]
    fn spacing_between_different_kinds() {
        let mut list = ContainerList::new();
        list.add_user_message("hi".into());
        list.add_system_message("info".into());

        let grad = drain(&mut list).unwrap();
        let plain = render_to_plain_text(&grad.node, 80);
        // Both should be present with spacing
        assert!(plain.contains("hi"));
        assert!(plain.contains("info"));
    }

    #[test]
    fn all_containers_graduate_with_gap() {
        let mut list = ContainerList::new();

        let mut tool1 = CachedToolCall::new("t1", "read_file", "{}");
        tool1.mark_complete();
        list.add_tool_call(tool1);

        // Force a new tool group by adding non-tool first
        list.add_system_message("between".into());

        let mut tool2 = CachedToolCall::new("t2", "write_file", "{}");
        tool2.mark_complete();
        list.add_tool_call(tool2);

        let grad = drain(&mut list).unwrap();
        // All three should graduate
        assert!(list.is_empty());
        assert!(grad.node != Node::Empty);
    }

    #[test]
    fn update_tool_does_not_auto_complete_group() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "read_file", "{}"));

        list.update_tool("read_file", None, |t| {
            t.mark_complete();
        });

        // Tool complete but turn active — should NOT graduate
        assert!(drain(&mut list).is_none());

        // End the turn
        list.complete_response();

        // Now it should graduate
        assert!(drain(&mut list).is_some());
    }

    #[test]
    fn cross_batch_spacing_uses_has_graduated() {
        let mut list = ContainerList::new();
        list.add_user_message("first".into());
        let grad1 = drain(&mut list).unwrap();
        // First graduation: no top padding (nothing before it)
        let rendered1 = grad1.render();
        assert!(!rendered1.starts_with("\r\n"), "first grad should have no leading blank");

        // Second graduation: should have top padding
        list.add_system_message("second".into());
        let grad2 = drain(&mut list).unwrap();
        let rendered2 = grad2.render();
        // The node tree should include top margin, producing a leading blank line
        assert!(
            rendered2.starts_with("\r\n") || rendered2.starts_with("\n"),
            "cross-batch spacing should produce leading blank: {:?}",
            &rendered2[..rendered2.len().min(40)]
        );
    }

    #[test]
    fn cancel_streaming_allows_graduation() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_text("partial");
        list.cancel_streaming();

        let grad = drain(&mut list);
        assert!(grad.is_some());
    }

    #[test]
    fn thinking_graduated_renders_collapsed() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_thinking("deep analysis of the problem");
        list.append_text("conclusion");
        list.complete_response();

        let grad = drain_with_thinking(&mut list).unwrap();
        let plain = render_to_plain_text(&grad.node, 80);
        // After graduation, thinking should be collapsed
        assert!(plain.contains("Thought"));
        assert!(plain.contains("words)"));
        assert!(plain.contains("conclusion"));
    }

    #[test]
    fn add_tool_call_marks_trailing_assistant_complete() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_text("let me use a tool");

        // Adding tool should mark the assistant complete
        list.add_tool_call(CachedToolCall::new("t1", "bash", "{}"));
        if let Some(ChatNode::AssistantResponse { complete, .. }) = list.nodes().first() {
            assert!(*complete);
        } else {
            panic!("expected AssistantResponse as first node");
        }
    }

    #[test]
    fn is_streaming_reflects_turn_active() {
        let mut list = ContainerList::new();
        assert!(!list.is_streaming());
        list.mark_turn_active();
        assert!(list.is_streaming());
        list.complete_response();
        assert!(!list.is_streaming());
    }

    #[test]
    fn clear_resets_everything() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_user_message("hi".into());
        drain(&mut list);

        list.clear();
        assert!(list.is_empty());
        assert!(!list.is_streaming());
        assert!(!list.has_graduated());
    }

    #[test]
    fn tool_grouping_adds_to_existing_group() {
        let mut list = ContainerList::new();
        list.add_tool_call(CachedToolCall::new("t1", "read", "{}"));
        list.add_tool_call(CachedToolCall::new("t2", "write", "{}"));

        // Should be one ToolGroup with two tools
        assert_eq!(list.len(), 1);
        if let ChatNode::ToolGroup { tools } = &list.nodes()[0] {
            assert_eq!(tools.len(), 2);
        } else {
            panic!("expected ToolGroup");
        }
    }

    #[test]
    fn continuation_derived_from_preceding_tool_group() {
        let mut list = ContainerList::new();
        let mut tool = CachedToolCall::new("t1", "read", "{}");
        tool.mark_complete();
        list.add_tool_call(tool);

        // Start a new assistant response after the tool group
        list.start_assistant_response();
        list.append_text("continuation text");
        list.complete_response();

        // Verify continuation is derived at render time
        let focus = FocusContext::default();
        let ctx = ViewContext::new(&focus);
        let nodes = list.nodes();
        let prev = Some(&nodes[0]);
        let node = render_chat_node(&nodes[1], prev, &ctx);
        let plain = render_to_plain_text(&node, 80);
        // Continuation text should not have the assistant bullet
        assert!(!plain.contains("●"), "Continuation should not have bullet: {}", plain);
    }
}
