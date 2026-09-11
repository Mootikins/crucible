//! Chat node model for viewport content.
//!
//! Each `ChatNode` is a graduation unit with explicit lifecycle state.
//! `ChatNode::render()` produces Node trees. Graduated nodes are
//! removed from the list and written to scrollback.
//!
//! Spacing: uniform `gap(1)` between all nodes.

use crucible_oil::node::{col, row, styled, Node};
use crucible_oil::style::{Gap, Style};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::components::thinking_component::ThinkingComponent;
use crate::tui::oil::components::{render_shell_execution, render_subagent};
use crate::tui::oil::markdown::{
    markdown_to_node_streaming, markdown_to_node_styled, Margins, RenderStyle,
};
use crate::tui::oil::render_state::RenderState;
use crate::tui::oil::utils::wrap_words;
use crate::tui::oil::viewport_cache::{CachedShellExecution, CachedSubagent, CachedToolCall};

// ─── Types ──────────────────────────────────────────────────────────────────

/// A chat node — the graduation unit and rendering primitive.
#[derive(Debug, Clone)]
pub enum ChatNode {
    UserMessage {
        text: String,
    },
    AssistantResponse {
        text: String,
        thinking: Vec<ThinkingComponent>,
        complete: bool,
    },
    ToolGroup {
        tools: Vec<CachedToolCall>,
    },
    /// The end of a tool that outran the split threshold.
    ///
    /// The start is the frozen card in the tool's own group, where the call
    /// was made. This node is appended when the call finishes, so the
    /// transcript reads in the order the events happened and no earlier node
    /// moves. `ran_for` is fixed here; a clock read at render time would make
    /// the row change on every frame.
    BackgroundToolFinished {
        tool: CachedToolCall,
        ran_for: Duration,
    },
    SubagentTask {
        agent: CachedSubagent,
    },
    ShellExecution {
        shell: CachedShellExecution,
    },
    SystemMessage {
        text: String,
    },
}

impl ChatNode {
    pub fn is_complete(&self) -> bool {
        match self {
            Self::UserMessage { .. }
            | Self::SystemMessage { .. }
            | Self::ShellExecution { .. }
            | Self::BackgroundToolFinished { .. } => true,
            Self::AssistantResponse { complete, .. } => *complete,
            // A backgrounded card is frozen, so it counts as settled even
            // though the call still runs.
            Self::ToolGroup { tools } => tools.iter().all(|t| t.complete || t.backgrounded),
            Self::SubagentTask { agent } => agent.is_terminal(),
        }
    }

    /// Render this node. `is_continuation` is derived from `prev`.
    pub fn render(&self, prev: Option<&ChatNode>, ctx: &ViewContext<'_>) -> Node {
        match self {
            Self::UserMessage { text } => Self::render_user_message(text, ctx.width()),
            Self::AssistantResponse {
                text,
                thinking,
                complete,
            } => {
                let is_continuation = matches!(
                    prev,
                    Some(
                        Self::ToolGroup { .. }
                            | Self::SubagentTask { .. }
                            | Self::ShellExecution { .. }
                    )
                );
                Self::render_assistant_response(text, thinking, is_continuation, *complete, ctx)
            }
            Self::ToolGroup { tools } => Self::render_tool_group(
                tools,
                ctx.frame_time,
                ctx.spinner_frame,
                ctx.width(),
                ctx.show_diffs,
            ),
            Self::SubagentTask { agent } => {
                render_subagent(agent, ctx.spinner_frame, ctx.frame_time, ctx.width())
            }
            Self::ShellExecution { shell } => render_shell_execution(shell),
            Self::BackgroundToolFinished { tool, ran_for } => {
                Self::render_background_finished(tool, *ran_for, ctx)
            }
            Self::SystemMessage { text } => Self::render_system_message(text),
        }
    }

    /// The end of a split tool.
    ///
    /// It does not animate and it holds no clock read. `ran_for` was fixed
    /// when the node was written, so the row is the same on every frame and
    /// it survives a scroll out of the repaintable window.
    fn render_background_finished(
        tool: &CachedToolCall,
        ran_for: Duration,
        ctx: &ViewContext<'_>,
    ) -> Node {
        let theme = ctx.theme;
        let dim = Style::new().fg(theme.resolve_color(theme.colors.text_dim));
        let muted = Style::new().fg(theme.resolve_color(theme.colors.text_muted));
        let summary = if let Some(error) = tool.error.as_ref() {
            format!(" failed after {:.1}s: {error}", ran_for.as_secs_f32())
        } else {
            format!(" finished after {:.1}s", ran_for.as_secs_f32())
        };
        row([
            styled(" \u{25AA} ", dim),
            styled(tool.name.to_string(), dim),
            styled(summary, muted),
        ])
    }

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
        let render_state = RenderState::from(ctx);

        let has_thinking = !thinking.is_empty();
        let margins = if is_continuation || has_thinking {
            Margins::assistant_continuation()
        } else {
            Margins::assistant()
        };

        let mut items: Vec<Node> = Vec::new();

        // Thinking renders inline as it streams. The component itself picks
        // the right view (expanded when `show_thinking`, collapsed
        // "Thinking… (N words)" otherwise) and handles the complete vs
        // in-progress distinction via the `is_complete` flag below.
        let thinking_complete = !content.is_empty() || is_complete;
        for tc in thinking {
            let node = tc.render(&render_state, thinking_complete);
            if !matches!(node, Node::Empty) {
                items.push(node);
            }
        }

        // Then markdown content. While the message streams, a trailing table
        // stays as source lines: laying it out again on every delta reshapes
        // rows that may already sit above the repaintable window.
        if !content.is_empty() {
            let style = RenderStyle::natural_with_margins(ctx.width(), margins);
            let md_node = if is_complete {
                markdown_to_node_styled(content, style)
            } else {
                markdown_to_node_streaming(content, style)
            };
            items.push(md_node);
        }

        match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap(),
            _ => col(items).gap(Gap::row(1)),
        }
    }

    /// Tool group: renders each tool via the existing tool renderer.
    fn render_tool_group(
        tools: &[CachedToolCall],
        now: Instant,
        spinner_frame: usize,
        width: usize,
        show_diffs: bool,
    ) -> Node {
        let items: Vec<Node> = tools
            .iter()
            .map(|tool| tool.render_compact_with(now, spinner_frame, width, show_diffs))
            .filter(|n| !matches!(n, Node::Empty))
            .collect();

        match items.len() {
            0 => Node::Empty,
            1 => items.into_iter().next().unwrap(),
            _ => col(items).gap(Gap::row(0)),
        }
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
}

// ─── ContainerList ──────────────────────────────────────────────────────────

/// Ordered list of chat nodes.
///
/// The list keeps every node for the life of the session. The renderer emits
/// the whole transcript each frame and the terminal owns the scroll, so a node
/// is never handed off and never dropped. A resize reprints the transcript
/// from these nodes at the new width.
pub struct ContainerList {
    nodes: Vec<ChatNode>,
    turn_active: bool,
    /// Tools that outran the split threshold, keyed by `CachedToolCall::id`.
    ///
    /// Their output keeps arriving after the split, so the mutable copy lives
    /// here rather than in a transcript node. The status line reports how many
    /// are in flight; the transcript shows only the immutable start and finish.
    background: Vec<CachedToolCall>,
}

impl ContainerList {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            turn_active: false,
            background: Vec::new(),
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
        self.background.clear();
        self.turn_active = false;
    }

    pub fn nodes(&self) -> &[ChatNode] {
        &self.nodes
    }

    pub fn is_streaming(&self) -> bool {
        self.turn_active
    }

    // ─── Mutations ──────────────────────────────────────────────────────

    pub fn add_user_message(&mut self, content: String) {
        self.nodes.push(ChatNode::UserMessage { text: content });
    }

    /// Ensure there's an AssistantResponse at the end. Creates one if needed.
    pub fn start_assistant_response(&mut self) {
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
    /// Always lands inside an AssistantResponse (creating an empty one if
    /// none exists yet) so the thinking renders inline as it streams. The
    /// AR's empty `text` won't graduate on its own — graduation only fires
    /// once the AR is complete or another node lands after it.
    pub fn append_thinking(&mut self, delta: &str) {
        self.start_assistant_response();
        if let Some(ChatNode::AssistantResponse { thinking, .. }) = self.nodes.last_mut() {
            if thinking.is_empty() {
                thinking.push(ThinkingComponent::new(String::new()));
            }
            thinking.last_mut().unwrap().append(delta);
        }
    }

    /// Add a tool call. Groups into an existing trailing ToolGroup if present,
    /// otherwise creates a new one.
    pub fn add_tool_call(&mut self, tool: CachedToolCall) {
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

    /// Freeze any tool that has run past `threshold`, in the place it was
    /// called.
    ///
    /// The card stops there: no spinner, no elapsed time, no streamed output.
    /// The live copy moves off the transcript into `self.background`, and the
    /// finish node is appended when the call ends. So a slow tool costs two
    /// immutable rows and no node above the tail ever moves.
    ///
    /// The freeze is the one change in place, and it happens `threshold`
    /// after the call, while the card still sits near the tail where the
    /// renderer can address it. Nothing else rewrites a written row. That is
    /// what the terminal requires: a row that scrolls above the screen
    /// belongs to the terminal, and a later repaint cannot reach it.
    ///
    /// `now` is the frame clock. A replay that never advances it never
    /// freezes, so a slow test machine renders the same transcript as a fast
    /// one.
    ///
    /// Returns whether anything froze, so the caller can request a frame.
    pub fn split_slow_tools(&mut self, now: Instant, threshold: Duration) -> bool {
        let mut froze = false;
        for node in self.nodes.iter_mut() {
            let ChatNode::ToolGroup { tools } = node else {
                continue;
            };
            for tool in tools.iter_mut() {
                let slow =
                    !tool.complete && !tool.backgrounded && tool.elapsed_at(now) >= threshold;
                if !slow {
                    continue;
                }
                tool.backgrounded = true;
                self.background.push(tool.clone());
                froze = true;
            }
        }
        froze
    }

    /// Apply an update to a split tool, if this call belongs to one.
    ///
    /// Returns whether the update was handled here. A split tool is no longer
    /// in any group, so the normal `update_tool` path would miss it.
    pub fn update_background_tool(
        &mut self,
        name: &str,
        call_id: Option<&str>,
        f: impl FnOnce(&mut CachedToolCall),
    ) -> bool {
        let found = match call_id {
            Some(cid) => self
                .background
                .iter_mut()
                .rev()
                .find(|t| t.call_id.as_deref() == Some(cid)),
            None => self
                .background
                .iter_mut()
                .rev()
                .find(|t| t.name.as_ref() == name),
        };
        let Some(tool) = found else {
            return false;
        };
        f(tool);
        true
    }

    /// Write the finish node for a split tool and drop its live copy.
    ///
    /// `now` is the frame clock; the finish node records the run time from it.
    ///
    /// Returns whether this call belonged to a split tool.
    pub fn finish_background_tool(
        &mut self,
        name: &str,
        call_id: Option<&str>,
        now: Instant,
    ) -> bool {
        let position = match call_id {
            Some(cid) => self
                .background
                .iter()
                .rposition(|t| t.call_id.as_deref() == Some(cid)),
            None => self
                .background
                .iter()
                .rposition(|t| t.name.as_ref() == name),
        };
        let Some(position) = position else {
            return false;
        };
        let mut tool = self.background.remove(position);
        tool.complete = true;
        let ran_for = tool.elapsed_at(now);
        self.nodes
            .push(ChatNode::BackgroundToolFinished { tool, ran_for });
        true
    }

    /// How many split tools are still running.
    ///
    /// The status line reports this; the transcript deliberately does not.
    pub fn background_task_count(&self) -> usize {
        self.background.len()
    }

    /// Update a tool within the most recent ToolGroup by name and optional call_id.
    pub fn update_tool(
        &mut self,
        name: &str,
        call_id: Option<&str>,
        f: impl FnOnce(&mut CachedToolCall),
    ) {
        // Search backwards for a ToolGroup containing this tool
        for node in self.nodes.iter_mut().rev() {
            if let ChatNode::ToolGroup { tools } = node {
                // Match by call_id first, then by name
                // A frozen card never changes again. Its live copy is in
                // `self.background`, which `update_background_tool` reaches.
                let found = if let Some(cid) = call_id {
                    tools
                        .iter_mut()
                        .rev()
                        .find(|t| !t.backgrounded && t.call_id.as_deref() == Some(cid))
                } else {
                    tools
                        .iter_mut()
                        .rev()
                        .find(|t| !t.backgrounded && t.name.as_ref() == name)
                };
                if let Some(tool) = found {
                    f(tool);
                    return;
                }
            }
        }
        tracing::warn!(
            name = %name,
            call_id = ?call_id,
            "tool update missed all live ToolGroups — tool already graduated to scrollback, or its call was never received"
        );
    }

    /// Update the most recent tool with the given call_id (without
    /// requiring the tool name). Used for ACP `tool_call_diff_update`
    /// events that key only on call_id.
    pub fn update_tool_by_call_id(&mut self, call_id: &str, f: impl FnOnce(&mut CachedToolCall)) {
        for node in self.nodes.iter_mut().rev() {
            if let ChatNode::ToolGroup { tools } = node {
                if let Some(tool) = tools
                    .iter_mut()
                    .rev()
                    .find(|t| !t.backgrounded && t.call_id.as_deref() == Some(call_id))
                {
                    f(tool);
                    return;
                }
            }
        }
        tracing::warn!(
            call_id = %call_id,
            "tool update by call_id missed all live ToolGroups — tool already graduated to scrollback, or its call was never received"
        );
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
        self.turn_active = false;
        if let Some(ChatNode::AssistantResponse { complete, .. }) = self.nodes.last_mut() {
            *complete = true;
        }
    }

    /// Cancel streaming: marks all streaming nodes as complete.
    pub fn cancel_streaming(&mut self) {
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

    fn render_node(node: &ChatNode, show_thinking: bool) -> String {
        let focus = FocusContext::default();
        let mut ctx = ViewContext::new(&focus);
        ctx.show_thinking = show_thinking;
        let rendered = node.render(None, &ctx);
        render_to_plain_text(&rendered, 80)
    }

    fn render_list(list: &ContainerList, show_thinking: bool) -> String {
        list.nodes()
            .iter()
            .map(|n| render_node(n, show_thinking))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_lines(list: &ContainerList) -> Vec<String> {
        render_list(list, false)
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// A streaming table must not reshape the rows above it. The terminal owns
    /// every transcript row that scrolled off the screen, so the transcript
    /// only appends there. A table lays out from the widest cell in the whole
    /// table, so each new row can rewrite every row before it.
    #[test]
    fn a_streaming_table_appends_lines_and_lays_out_when_complete() {
        let deltas = [
            "Here are the commands.\n\n",
            "| Command | What it does |\n",
            "|---|---|\n",
            "| `cru chat` | Start a chat |\n",
            "| `cru session list` | List every session in this project |\n",
        ];

        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.start_assistant_response();

        let mut previous: Vec<String> = Vec::new();
        for delta in deltas {
            list.append_text(delta);
            let lines = render_lines(&list);
            assert!(
                lines.starts_with(previous.as_slice()),
                "a delta rewrote earlier lines\nbefore: {previous:#?}\nafter: {lines:#?}"
            );
            previous = lines;
        }

        assert!(
            !previous.iter().any(|l| l.contains('┌')),
            "the streaming table must stay as source lines: {previous:#?}"
        );

        list.complete_response();
        let complete = render_lines(&list);
        assert!(
            complete.iter().any(|l| l.contains('┌')),
            "the finished table must be laid out: {complete:#?}"
        );
    }

    // ─── Live thinking rendering ───────────────────────────────────────

    #[test]
    fn streaming_thinking_first_turn_renders_inline() {
        // First-turn case: no prior AR yet, but thinking starts streaming.
        // The user must see thinking content immediately, not wait for the
        // first text delta to materialize an AR.
        let mut list = ContainerList::new();
        list.add_user_message("hi".into());
        list.mark_turn_active();
        list.append_thinking("Working it out");
        let plain = render_list(&list, true);
        assert!(
            plain.contains("Working it out"),
            "first-turn thinking must render live: {:?}",
            plain
        );
    }

    #[test]
    fn streaming_thinking_renders_inline_during_streaming() {
        // Regression: thinking should appear in the viewport as it streams,
        // not stay invisible until text arrives or the turn ends.
        let mut list = ContainerList::new();
        list.add_user_message("hi".into());
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_thinking("Reasoning about the question");
        let plain = render_list(&list, true);
        assert!(
            plain.contains("Reasoning about the question"),
            "thinking content must render live: {:?}",
            plain
        );
    }

    #[test]
    fn streaming_thinking_collapsed_shows_word_count_live() {
        // With show_thinking=false, the live render should at least show a
        // "Thinking… (N words)" placeholder so users see progress.
        let mut list = ContainerList::new();
        list.add_user_message("hi".into());
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_thinking("alpha beta gamma");
        let plain = render_list(&list, false);
        assert!(
            plain.contains("3 words") || plain.contains("Thinking"),
            "collapsed live thinking must indicate progress: {:?}",
            plain
        );
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
    fn user_then_thinking_then_tool_call_preserves_thinking() {
        // Sequence: user message → thinking delta arrives → tool call lands.
        // The append_thinking call creates an AR that holds the thinking
        // content. add_tool_call marks the AR complete and creates a fresh
        // ToolGroup. Both nodes must render meaningful content; the AR is
        // not empty (it carries the thinking) so it must NOT be silently
        // swallowed by render-time Node::Empty short-circuiting.
        let mut list = ContainerList::new();
        list.add_user_message("question".into());
        list.mark_turn_active();
        list.append_thinking("considering options");
        list.add_tool_call(CachedToolCall::new("t1", "bash", "{}"));

        let nodes = list.nodes();
        // Expect [user, AR(thinking), ToolGroup] — three distinct nodes.
        assert_eq!(
            nodes.len(),
            3,
            "expected 3 nodes, got {} ({:?})",
            nodes.len(),
            nodes
                .iter()
                .map(|n| match n {
                    ChatNode::UserMessage { .. } => "user",
                    ChatNode::AssistantResponse { .. } => "ar",
                    ChatNode::ToolGroup { .. } => "tools",
                    ChatNode::SystemMessage { .. } => "sys",
                    _ => "other",
                })
                .collect::<Vec<_>>()
        );
        match &nodes[1] {
            ChatNode::AssistantResponse {
                text,
                thinking,
                complete,
            } => {
                assert!(*complete, "AR before ToolGroup must be complete");
                assert!(text.is_empty(), "no text yet — only thinking");
                assert_eq!(thinking.len(), 1, "thinking component should exist");
                let plain = render_node(&nodes[1], true);
                assert!(
                    plain.contains("considering options"),
                    "thinking content must render: {:?}",
                    plain
                );
            }
            other => panic!("expected AssistantResponse, got {:?}", other),
        }
        assert!(matches!(&nodes[2], ChatNode::ToolGroup { .. }));
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

    // ─── Transcript retention ───────────────────────────────────────────
    //
    // The renderer emits the whole transcript each frame and the terminal owns
    // the scroll. Nothing is handed off, so nothing may be dropped: a resize
    // reprints from these nodes, and a node that left the list could not come
    // back.

    #[test]
    fn transcript_keeps_every_node_across_a_full_turn() {
        let mut list = ContainerList::new();
        list.add_user_message("do the thing".into());
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "read_file", "{}"));
        list.update_tool("read_file", None, |t| t.mark_complete());
        list.start_assistant_response();
        list.append_text("done");
        list.complete_response();

        assert_eq!(
            list.len(),
            3,
            "user message, tool group and response must all remain"
        );
        assert!(matches!(list.nodes()[0], ChatNode::UserMessage { .. }));
        assert!(matches!(list.nodes()[1], ChatNode::ToolGroup { .. }));
        assert!(matches!(
            list.nodes()[2],
            ChatNode::AssistantResponse { .. }
        ));
    }

    #[test]
    fn transcript_keeps_nodes_across_several_turns() {
        let mut list = ContainerList::new();
        for turn in 0..3 {
            list.add_user_message(format!("question {turn}"));
            list.mark_turn_active();
            list.start_assistant_response();
            list.append_text("answer");
            list.complete_response();
        }

        assert_eq!(list.len(), 6, "three turns leave three pairs of nodes");
        let rendered = render_list(&list, /*show_thinking*/ false);
        for turn in 0..3 {
            assert!(
                rendered.contains(&format!("question {turn}")),
                "turn {turn} must still render: {rendered:?}"
            );
        }
    }

    // ─── Slow-tool split ────────────────────────────────────────────────

    #[test]
    fn a_fast_tool_is_never_split() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "read_file", "{}"));
        list.update_tool("read_file", None, |t| t.mark_complete());

        assert!(!list.split_slow_tools(Instant::now(), Duration::from_millis(500)));
        assert_eq!(list.len(), 1, "the tool stays one grouped node");
        assert_eq!(list.background_task_count(), 0);
    }

    #[test]
    fn a_slow_tool_freezes_where_it_was_called() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "bash", "{}"));

        // Zero threshold: the tool is already past it.
        assert!(list.split_slow_tools(Instant::now(), Duration::ZERO));
        assert_eq!(list.len(), 1, "the card stays in its own group");
        let ChatNode::ToolGroup { tools } = &list.nodes()[0] else {
            panic!("the tool must stay in its group: {:?}", list.nodes()[0]);
        };
        assert_eq!(tools.len(), 1, "the group keeps its card");
        assert!(tools[0].backgrounded, "the card is frozen");
        assert_eq!(list.background_task_count(), 1);
    }

    /// The bug this replaced: the freeze used to remove the card from its
    /// group, so every line below it moved up. A row that already scrolled
    /// above the screen belongs to the terminal and no repaint can reach it,
    /// so the repaint wrote the wrong text over the seam.
    ///
    /// The assertion is on the rendered lines, not on the node list. Line
    /// positions are what the renderer diffs, and a node list can keep its
    /// length while its rows move.
    #[test]
    fn a_freeze_moves_no_rendered_line() {
        let mut list = ContainerList::new();
        list.add_user_message("run it".into());
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "bash", "{}"));
        list.start_assistant_response();
        list.append_text("the answer");

        let before: Vec<String> = render_list(&list, false)
            .lines()
            .map(str::to_owned)
            .collect();
        assert!(list.split_slow_tools(Instant::now(), Duration::ZERO));
        let after: Vec<String> = render_list(&list, false)
            .lines()
            .map(str::to_owned)
            .collect();

        assert_eq!(
            before.len(),
            after.len(),
            "the freeze must not change the line count.\nbefore:\n{before:#?}\nafter:\n{after:#?}"
        );
        let answer_before = before.iter().position(|l| l.contains("the answer"));
        let answer_after = after.iter().position(|l| l.contains("the answer"));
        assert_eq!(
            answer_before, answer_after,
            "the answer must stay on its own line.\nbefore:\n{before:#?}\nafter:\n{after:#?}"
        );
        assert!(
            after
                .iter()
                .any(|l| l.contains("started in the background")),
            "the frozen card must say so: {after:#?}"
        );
    }

    #[test]
    fn finishing_a_split_tool_appends_a_second_node() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "bash", "{}"));
        list.split_slow_tools(Instant::now(), Duration::ZERO);

        assert!(list.finish_background_tool("bash", None, Instant::now()));
        assert_eq!(list.len(), 2, "the finish node is appended below the card");
        assert!(matches!(
            list.nodes()[1],
            ChatNode::BackgroundToolFinished { .. }
        ));
        assert_eq!(
            list.background_task_count(),
            0,
            "a finished tool leaves the live set"
        );
    }

    #[test]
    fn a_split_tool_keeps_taking_output_off_the_transcript() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "bash", "{}"));
        list.split_slow_tools(Instant::now(), Duration::ZERO);

        // Output arriving after the freeze must not reach the frozen card.
        assert!(list.update_background_tool("bash", None, |t| t.append_output("line\n")));
        assert_eq!(list.len(), 1, "no node was added by output alone");
        let ChatNode::ToolGroup { tools } = &list.nodes()[0] else {
            panic!("the card must stay in its group");
        };
        assert!(
            tools[0].output_tail.is_empty(),
            "the frozen card must take no output"
        );

        // The normal update path must also miss it.
        list.update_tool("bash", None, |t| t.append_output("other\n"));
        let ChatNode::ToolGroup { tools } = &list.nodes()[0] else {
            unreachable!()
        };
        assert!(
            tools[0].output_tail.is_empty(),
            "update_tool must skip a frozen card"
        );
    }

    #[test]
    fn background_nodes_are_complete_so_they_never_mutate() {
        // Every transcript node must be immutable, because a node can scroll
        // out of the repaintable window at any time.
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_tool_call(CachedToolCall::new("t1", "bash", "{}"));
        list.split_slow_tools(Instant::now(), Duration::ZERO);
        list.finish_background_tool("bash", None, Instant::now());

        for node in list.nodes() {
            assert!(node.is_complete(), "background nodes must be immutable");
        }
    }

    #[test]
    fn clear_resets_everything() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.add_user_message("hi".into());

        list.clear();
        assert!(list.is_empty());
        assert!(!list.is_streaming());
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
        let node = nodes[1].render(prev, &ctx);
        let plain = render_to_plain_text(&node, 80);
        // Continuation text should not have the assistant bullet
        assert!(
            !plain.contains("●"),
            "Continuation should not have bullet: {}",
            plain
        );
    }
}
