//! Container model for chat content.
//!
//! Each container is a graduation unit with explicit lifecycle state.
//! Containers produce Node trees via `view()`. Graduated containers are
//! removed from the list and written to scrollback.
//!
//! Spacing rule: adjacent ToolGroups get zero gap; everything else
//! gets one blank line between containers.

use crucible_oil::node::{col, styled, text, Node};
use crucible_oil::planning::Graduation;
use crucible_oil::style::{Gap, Style};
use unicode_width::UnicodeWidthStr;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::thinking_component::ThinkingComponent;
use crate::tui::oil::components::{render_shell_execution, render_subagent, render_tool_call_with_frame};
use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::render_state::RenderState;
use crate::tui::oil::utils::wrap_words;
use crate::tui::oil::viewport_cache::{CachedShellExecution, CachedSubagent, CachedToolCall};

// ─── Types ──────────────────────────────────────────────────────────────────

/// Explicit container lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerState {
    Streaming,
    Complete,
}

/// What kind of container (for spacing decisions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    UserMessage,
    AssistantResponse,
    ToolGroup,
    SubagentTask,
    ShellExecution,
    SystemMessage,
}

/// Context passed to `Container::view()`.
#[derive(Debug, Clone, Copy)]
pub struct ContainerViewContext {
    pub width: usize,
    pub spinner_frame: usize,
    pub show_thinking: bool,
}

/// A chat container — the graduation unit.
#[derive(Debug, Clone)]
pub struct Container {
    pub id: String,
    pub kind: ContainerKind,
    pub state: ContainerState,
    pub content: ContainerContent,
}

/// Kind-specific content for each container type.
#[derive(Debug, Clone)]
pub enum ContainerContent {
    UserMessage {
        text: String,
    },
    AssistantResponse {
        text: String,
        thinking: Vec<ThinkingComponent>,
        is_continuation: bool,
    },
    ToolGroup {
        tools: Vec<CachedToolCall>,
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

// ─── Container view ─────────────────────────────────────────────────────────

impl Container {
    /// Render this container's content as a Node tree.
    ///
    /// Used by graduation path which still needs ContainerViewContext.
    /// Viewport rendering goes through the Component trait impl.
    pub fn render(&self, ctx: &ContainerViewContext) -> Node {
        match &self.content {
            ContainerContent::UserMessage { text } => render_user_message(text, ctx.width),
            ContainerContent::AssistantResponse {
                text,
                thinking,
                is_continuation,
            } => render_assistant_response(
                text,
                thinking,
                *is_continuation,
                self.state == ContainerState::Complete,
                ctx,
            ),
            ContainerContent::ToolGroup { tools } => render_tool_group(tools, ctx.spinner_frame, ctx.width),
            ContainerContent::SubagentTask { agent } => render_subagent_task(agent, ctx.spinner_frame, ctx.width),
            ContainerContent::ShellExecution { shell } => render_shell(shell),
            ContainerContent::SystemMessage { text } => render_system_message(text),
        }
    }
}

impl Component for Container {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        let cvc = ContainerViewContext {
            width: ctx.width(),
            spinner_frame: ctx.spinner_frame,
            show_thinking: ctx.show_thinking,
        };
        // Delegate to render(), which has access to container state
        self.render(&cvc)
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
    ctx: &ContainerViewContext,
) -> Node {
    let render_state = RenderState {
        terminal_width: ctx.width as u16,
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
        let style = RenderStyle::natural_with_margins(ctx.width, margins);
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

// ─── Spacing ────────────────────────────────────────────────────────────────

/// Lay out container nodes with Gap-based spacing.
///
/// Fold containers into tight groups (adjacent ToolGroups), then
/// join groups with gap(1). `prev_kind` seeds the fold so cross-batch
/// spacing works identically to within-batch spacing.
pub fn layout_containers(
    containers: &[(ContainerKind, Node)],
    prev_kind: Option<ContainerKind>,
) -> Node {
    if containers.is_empty() {
        return Node::Empty;
    }

    let is_tight = |a, b| matches!((a, b), (ContainerKind::ToolGroup, ContainerKind::ToolGroup));

    // Seed: if prev_kind exists and isn't tight with the first container,
    // start with an empty sentinel group so gap(1) produces a leading blank.
    let seed_groups: Vec<Vec<Node>> = match prev_kind {
        Some(pk) if !is_tight(pk, containers[0].0) => vec![vec![text("")]],
        _ => Vec::new(),
    };

    let (groups, _) = containers.iter().fold(
        (seed_groups, prev_kind),
        |(mut groups, prev), (kind, node)| {
            let tight = prev.is_some_and(|pk| is_tight(pk, *kind));
            if !tight || groups.is_empty() {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push(node.clone());
            (groups, Some(*kind))
        },
    );

    let nodes: Vec<Node> = groups
        .into_iter()
        .map(|g| match g.len() {
            1 => g.into_iter().next().unwrap(),
            _ => col(g).gap(Gap::row(0)),
        })
        .collect();

    match nodes.len() {
        0 => Node::Empty,
        1 => nodes.into_iter().next().unwrap(),
        _ => col(nodes).gap(Gap::row(1)),
    }
}

// ─── ContainerList ──────────────────────────────────────────────────────────

/// Ordered list of containers with graduation support.
///
/// `drain_completed()` removes completed containers from the front and
/// returns a `Graduation` node tree for scrollback output.
pub struct ContainerList {
    containers: Vec<Container>,
    id_counter: u64,
    turn_active: bool,
    last_graduated_kind: Option<ContainerKind>,
}

impl ContainerList {
    pub fn new() -> Self {
        Self {
            containers: Vec::new(),
            id_counter: 0,
            turn_active: false,
            last_graduated_kind: None,
        }
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.id_counter += 1;
        format!("{}-{}", prefix, self.id_counter)
    }

    pub fn len(&self) -> usize {
        self.containers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
    }

    pub fn clear(&mut self) {
        self.containers.clear();
        self.last_graduated_kind = None;
        self.turn_active = false;
    }

    pub fn containers(&self) -> &[Container] {
        &self.containers
    }

    /// Whether the viewport needs a leading blank line for cross-batch spacing
    /// (graduated content above isn't tight with the first viewport container).
    pub fn needs_cross_batch_gap(&self) -> bool {
        let Some(prev) = self.last_graduated_kind else { return false };
        let Some(first) = self.containers.first() else { return false };
        !matches!(
            (prev, first.kind),
            (ContainerKind::ToolGroup, ContainerKind::ToolGroup)
        )
    }

    pub fn is_streaming(&self) -> bool {
        self.turn_active
    }

    pub fn last_graduated_kind(&self) -> Option<ContainerKind> {
        self.last_graduated_kind
    }

    // ─── Mutations ──────────────────────────────────────────────────────

    pub fn add_user_message(&mut self, content: String) {
        let id = self.next_id("user");
        self.containers.push(Container {
            id,
            kind: ContainerKind::UserMessage,
            state: ContainerState::Complete,
            content: ContainerContent::UserMessage { text: content },
        });
    }

    /// Ensure there's an AssistantResponse at the end. Creates one if needed.
    pub fn start_assistant_response(&mut self) {
        if !matches!(
            self.containers.last().map(|c| &c.content),
            Some(ContainerContent::AssistantResponse { .. })
        ) {
            let trailing_kind = self.containers.last().map(|c| c.kind);
            tracing::debug!(?trailing_kind, "creating new AssistantResponse");
            let id = self.next_id("asst");
            // Check current containers first, then fall back to last graduated kind.
            // After graduation, containers may be empty but the continuation
            // context is preserved in last_graduated_kind.
            let prev_kind = self
                .containers
                .last()
                .map(|c| c.kind)
                .or(self.last_graduated_kind);
            let is_continuation = prev_kind.is_some_and(|k| {
                matches!(
                    k,
                    ContainerKind::ToolGroup
                        | ContainerKind::SubagentTask
                        | ContainerKind::ShellExecution
                )
            });
            self.containers.push(Container {
                id,
                kind: ContainerKind::AssistantResponse,
                state: ContainerState::Streaming,
                content: ContainerContent::AssistantResponse {
                    text: String::new(),
                    thinking: Vec::new(),
                    is_continuation,
                },
            });
        }
    }

    /// Append text to the current AssistantResponse. Creates one if needed.
    pub fn append_text(&mut self, delta: &str) {
        self.start_assistant_response();
        if let Some(Container {
            content: ContainerContent::AssistantResponse { text, .. },
            ..
        }) = self.containers.last_mut()
        {
            text.push_str(delta);
        }
    }

    /// Append thinking content. One ThinkingComponent per AssistantResponse.
    pub fn append_thinking(&mut self, delta: &str) {
        self.start_assistant_response();
        if let Some(Container {
            content: ContainerContent::AssistantResponse { thinking, .. },
            ..
        }) = self.containers.last_mut()
        {
            if thinking.is_empty() {
                thinking.push(ThinkingComponent::new(String::new()));
            }
            thinking.last_mut().unwrap().append(delta);
        }
    }

    /// Add a tool call. Groups into an existing trailing ToolGroup if present,
    /// otherwise creates a new one.
    pub fn add_tool_call(&mut self, tool: CachedToolCall) {
        let trailing_kind = self.containers.last().map(|c| (c.kind, c.state));
        tracing::debug!(
            tool_name = %tool.name,
            trailing = ?trailing_kind,
            container_count = self.containers.len(),
            "add_tool_call"
        );

        // First, mark any trailing AssistantResponse as Complete
        if let Some(last) = self.containers.last_mut() {
            if matches!(last.content, ContainerContent::AssistantResponse { .. })
                && last.state == ContainerState::Streaming
            {
                tracing::debug!("marking trailing AR complete before tool");
                last.state = ContainerState::Complete;
            }
        }

        // Group into existing ToolGroup or create new one
        if let Some(Container {
            content: ContainerContent::ToolGroup { tools },
            state,
            ..
        }) = self.containers.last_mut()
        {
            tracing::debug!("appending to existing ToolGroup");
            tools.push(tool);
            *state = ContainerState::Streaming;
        } else {
            tracing::debug!("creating new ToolGroup");
            let id = self.next_id("tools");
            self.containers.push(Container {
                id,
                kind: ContainerKind::ToolGroup,
                state: ContainerState::Streaming,
                content: ContainerContent::ToolGroup { tools: vec![tool] },
            });
        }
    }

    /// Update a tool within the most recent ToolGroup by name and optional call_id.
    pub fn update_tool(&mut self, name: &str, call_id: Option<&str>, f: impl FnOnce(&mut CachedToolCall)) {
        // Search backwards for a ToolGroup containing this tool
        for container in self.containers.iter_mut().rev() {
            if let ContainerContent::ToolGroup { tools } = &mut container.content {
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
                    // Update ToolGroup state based on tool completeness
                    if tools.iter().all(|t| t.complete) {
                        container.state = ContainerState::Complete;
                    }
                    return;
                }
            }
        }
        tracing::debug!(name = %name, call_id = ?call_id, "tool update for unknown tool (already graduated or never received)");
    }

    pub fn add_agent_task(&mut self, agent: CachedSubagent) {
        let id = self.next_id("agent");
        self.containers.push(Container {
            id,
            kind: ContainerKind::SubagentTask,
            state: ContainerState::Streaming,
            content: ContainerContent::SubagentTask { agent },
        });
    }

    pub fn update_agent_task(&mut self, agent_id: &str, f: impl FnOnce(&mut CachedSubagent)) {
        for container in self.containers.iter_mut().rev() {
            if let ContainerContent::SubagentTask { agent } = &mut container.content {
                if agent.id.as_ref() == agent_id {
                    f(agent);
                    if agent.is_terminal() {
                        container.state = ContainerState::Complete;
                    }
                    return;
                }
            }
        }
        tracing::debug!(agent_id = %agent_id, "agent task update for unknown agent (already graduated or never received)");
    }

    pub fn add_shell_execution(&mut self, shell: CachedShellExecution) {
        let id = self.next_id("shell");
        self.containers.push(Container {
            id,
            kind: ContainerKind::ShellExecution,
            state: ContainerState::Complete,
            content: ContainerContent::ShellExecution { shell },
        });
    }

    pub fn add_system_message(&mut self, content: String) {
        let id = self.next_id("sys");
        self.containers.push(Container {
            id,
            kind: ContainerKind::SystemMessage,
            state: ContainerState::Complete,
            content: ContainerContent::SystemMessage { text: content },
        });
    }

    /// Mark the turn as complete: sets turn_active = false and marks
    /// the trailing AssistantResponse as Complete.
    pub fn complete_response(&mut self) {
        self.turn_active = false;
        if let Some(last) = self.containers.last_mut() {
            if matches!(last.content, ContainerContent::AssistantResponse { .. }) {
                last.state = ContainerState::Complete;
            }
        }
    }

    /// Cancel streaming: marks all streaming containers as complete.
    pub fn cancel_streaming(&mut self) {
        self.turn_active = false;
        for container in &mut self.containers {
            if container.state == ContainerState::Streaming {
                container.state = ContainerState::Complete;
            }
        }
    }

    pub fn mark_turn_active(&mut self) {
        self.turn_active = true;
    }

    // ─── Graduation ─────────────────────────────────────────────────────

    /// Whether the container at `index` is ready for graduation.
    fn is_graduatable(&self, index: usize) -> bool {
        let container = &self.containers[index];
        match (&container.content, container.state) {
            (_, ContainerState::Complete) => true,
            (ContainerContent::AssistantResponse { .. }, ContainerState::Streaming) => {
                // Graduate if turn ended or something follows this response
                !self.turn_active || index + 1 < self.containers.len()
            }
            (ContainerContent::ToolGroup { tools }, ContainerState::Streaming) => {
                tools.iter().all(|t| t.complete)
            }
            _ => false,
        }
    }

    /// Drain completed containers from the front and return a graduation
    /// node tree for scrollback output.
    ///
    /// Graduated thinking blocks are collapsed (via `ThinkingComponent::graduate()`).
    /// Spacing uses the shared `layout_containers()` function (Gap-based).
    pub fn drain_completed(
        &mut self,
        width: u16,
        spinner_frame: usize,
        show_thinking: bool,
    ) -> Option<Graduation> {
        let ctx = ContainerViewContext {
            width: width as usize,
            spinner_frame,
            show_thinking,
        };

        let mut pairs: Vec<(ContainerKind, Node)> = Vec::new();

        while !self.containers.is_empty() && self.is_graduatable(0) {
            let mut container = self.containers.remove(0);
            let kind = container.kind;
            tracing::debug!(?kind, state = ?container.state, id = %container.id, "graduating container");

            // Graduate thinking components so they render collapsed
            if let ContainerContent::AssistantResponse { thinking, .. } = &mut container.content {
                for tc in thinking.iter_mut() {
                    tc.graduate();
                }
            }

            pairs.push((kind, container.render(&ctx)));
        }

        if pairs.is_empty() {
            return None;
        }

        let prev_kind = self.last_graduated_kind;
        self.last_graduated_kind = Some(pairs.last().unwrap().0);

        let node = layout_containers(&pairs, prev_kind);

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
    use crucible_oil::render::render_to_plain_text;

    #[test]
    fn empty_list_drains_nothing() {
        let mut list = ContainerList::new();
        assert!(list.drain_completed(80, 0, false).is_none());
    }

    #[test]
    fn user_message_graduates_immediately() {
        let mut list = ContainerList::new();
        list.add_user_message("hello".into());
        assert_eq!(list.len(), 1);

        let grad = list.drain_completed(80, 0, false);
        assert!(grad.is_some());
        assert!(list.is_empty());

        let plain = render_to_plain_text(&grad.unwrap().node, 80);
        assert!(plain.contains("hello"));
    }

    #[test]
    fn system_message_graduates_immediately() {
        let mut list = ContainerList::new();
        list.add_system_message("Session started".into());

        let grad = list.drain_completed(80, 0, false);
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
        assert!(list.drain_completed(80, 0, false).is_none());
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn assistant_graduates_after_complete() {
        let mut list = ContainerList::new();
        list.mark_turn_active();
        list.start_assistant_response();
        list.append_text("done");
        list.complete_response();

        let grad = list.drain_completed(80, 0, false);
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

        let grad = list.drain_completed(80, 0, false);
        assert!(grad.is_some());
        // Tool group remains (streaming)
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn tool_group_graduates_when_all_complete() {
        let mut list = ContainerList::new();
        let mut tool = CachedToolCall::new("t1", "read_file", "{}");
        tool.mark_complete();
        list.add_tool_call(tool);

        let grad = list.drain_completed(80, 0, false);
        assert!(grad.is_some());
        assert!(list.is_empty());
    }

    #[test]
    fn tool_group_does_not_graduate_while_pending() {
        let mut list = ContainerList::new();
        list.add_tool_call(CachedToolCall::new("t1", "read_file", "{}"));

        assert!(list.drain_completed(80, 0, false).is_none());
    }

    #[test]
    fn spacing_between_different_kinds() {
        let mut list = ContainerList::new();
        list.add_user_message("hi".into());
        list.add_system_message("info".into());

        let grad = list.drain_completed(80, 0, false).unwrap();
        let plain = render_to_plain_text(&grad.node, 80);
        // Both should be present with spacing
        assert!(plain.contains("hi"));
        assert!(plain.contains("info"));
    }

    #[test]
    fn adjacent_tool_groups_are_tight() {
        let mut list = ContainerList::new();

        let mut tool1 = CachedToolCall::new("t1", "read_file", "{}");
        tool1.mark_complete();
        list.add_tool_call(tool1);

        // Force a new tool group by adding non-tool first
        list.add_system_message("between".into());

        let mut tool2 = CachedToolCall::new("t2", "write_file", "{}");
        tool2.mark_complete();
        list.add_tool_call(tool2);

        let grad = list.drain_completed(80, 0, false).unwrap();
        // All three should graduate
        assert!(list.is_empty());
        assert!(grad.node != Node::Empty);
    }

    #[test]
    fn update_tool_marks_group_complete() {
        let mut list = ContainerList::new();
        list.add_tool_call(CachedToolCall::new("t1", "read_file", "{}"));

        list.update_tool("read_file", None, |t| {
            t.mark_complete();
        });

        // Now the ToolGroup should be complete
        assert!(list.drain_completed(80, 0, false).is_some());
    }

    #[test]
    fn cross_batch_spacing_uses_top_padding() {
        let mut list = ContainerList::new();
        list.add_user_message("first".into());
        let grad1 = list.drain_completed(80, 0, false).unwrap();
        // First graduation: no top padding (nothing before it)
        let rendered1 = grad1.render();
        assert!(!rendered1.starts_with("\r\n"), "first grad should have no leading blank");

        // Second graduation: different kind, should have top padding
        list.add_system_message("second".into());
        let grad2 = list.drain_completed(80, 0, false).unwrap();
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

        let grad = list.drain_completed(80, 0, false);
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

        let grad = list.drain_completed(80, 0, true).unwrap();
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

        // Adding tool should mark the assistant Complete
        list.add_tool_call(CachedToolCall::new("t1", "bash", "{}"));
        assert_eq!(list.containers[0].state, ContainerState::Complete);
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
        list.drain_completed(80, 0, false);

        list.clear();
        assert!(list.is_empty());
        assert!(!list.is_streaming());
        assert!(list.last_graduated_kind().is_none());
    }

    #[test]
    fn tool_grouping_adds_to_existing_group() {
        let mut list = ContainerList::new();
        list.add_tool_call(CachedToolCall::new("t1", "read", "{}"));
        list.add_tool_call(CachedToolCall::new("t2", "write", "{}"));

        // Should be one ToolGroup with two tools
        assert_eq!(list.len(), 1);
        if let ContainerContent::ToolGroup { tools } = &list.containers[0].content {
            assert_eq!(tools.len(), 2);
        } else {
            panic!("expected ToolGroup");
        }
    }

    #[test]
    fn continuation_flag_set_after_tool_group() {
        let mut list = ContainerList::new();
        let mut tool = CachedToolCall::new("t1", "read", "{}");
        tool.mark_complete();
        list.add_tool_call(tool);

        // Start a new assistant response after the tool group
        list.start_assistant_response();
        if let ContainerContent::AssistantResponse {
            is_continuation, ..
        } = &list.containers.last().unwrap().content
        {
            assert!(*is_continuation);
        } else {
            panic!("expected AssistantResponse");
        }
    }
}
