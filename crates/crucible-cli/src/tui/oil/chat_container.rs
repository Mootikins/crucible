//! Semantic containers for chat content.
//!
//! Each container represents a logical unit of content that graduates together.
//! Graduation is drain-based: completed containers are rendered, written to stdout,
//! and removed from the list. No Node::Static or key tracking needed.

use crate::tui::oil::components::{
    render_shell_execution, render_subagent, render_thinking_block, render_tool_call_with_frame,
    render_user_prompt,
};
use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::node::{col, row, spinner, styled, text, Node};
use crate::tui::oil::render_state::RenderState;
use crate::tui::oil::style::{Gap, Style};

use crate::tui::oil::viewport_cache::{CachedShellExecution, CachedSubagent, CachedToolCall};

/// Parameters for rendering a container view.
///
/// Bundles layout context and derived state that containers need for rendering.
#[derive(Debug, Clone, Copy)]
pub struct ViewParams {
    pub render_state: RenderState,
    /// Whether this container is a continuation after a tool call (no bullet shown).
    pub is_continuation: bool,
    /// Whether this response is complete (derived from turn state + position).
    pub is_complete: bool,
    /// Whether the previous container was a ToolGroup (for tight tool-to-tool graduation).
    pub prev_is_tool_group: bool,
}

/// A block of thinking content with token count.
#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    pub content: String,
    pub token_count: usize,
}

/// What kind of container this is, for spacing decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerKind {
    UserMessage,
    AssistantResponse,
    ToolGroup,
    AgentTask,
    ShellExecution,
    SystemMessage,
}

/// Whether a blank line is needed between two adjacent container kinds.
///
/// Consecutive tool groups are tight (no blank line). Everything else
/// gets a blank line for visual separation.
pub fn needs_spacing(prev: ContainerKind, next: ContainerKind) -> bool {
    !matches!(
        (prev, next),
        (ContainerKind::ToolGroup, ContainerKind::ToolGroup)
    )
}

/// Group (kind, node) pairs into a col with correct spacing.
///
/// Same logic as `render_containers()` in rendering.rs: consecutive ToolGroups
/// are wrapped in a tight sub-col (`gap(0)`), everything else is separated by
/// the outer `gap(1)`. Shared by both the graduation path and viewport path.
pub(crate) fn group_container_nodes(items: Vec<(ContainerKind, Node)>) -> Node {
    let mut groups: Vec<Node> = Vec::new();
    let mut tight_run: Vec<Node> = Vec::new();
    let mut run_kind: Option<ContainerKind> = None;

    for (kind, node) in items {
        let should_break = run_kind
            .map(|prev| needs_spacing(prev, kind))
            .unwrap_or(false);

        if should_break {
            if tight_run.len() == 1 {
                groups.push(tight_run.pop().unwrap());
            } else if !tight_run.is_empty() {
                groups.push(col(tight_run.drain(..)).gap(Gap::row(0)));
            }
        }

        tight_run.push(node);
        run_kind = Some(kind);
    }
    // Flush remaining
    if tight_run.len() == 1 {
        groups.push(tight_run.pop().unwrap());
    } else if !tight_run.is_empty() {
        groups.push(col(tight_run).gap(Gap::row(0)));
    }

    if groups.len() == 1 {
        groups.pop().unwrap()
    } else {
        col(groups).gap(Gap::row(1))
    }
}

/// Semantic container for chat content.
///
/// Each container is a graduation unit - it graduates and drops as a whole.
/// Container IDs remain stable throughout their lifecycle, eliminating
/// the need for `pre_graduate_keys` tracking.
#[derive(Debug, Clone)]
pub enum ChatContainer {
    /// Single user message with input prompt styling
    UserMessage { id: String, content: String },

    /// Assistant response: accumulated text + thinking blocks.
    /// Text is a single string (no splitting); the markdown parser handles
    /// paragraph decomposition at render time.
    AssistantResponse {
        id: String,
        text: String,
        thinking: Vec<ThinkingBlock>,
        /// Whether this response follows a tool/subagent/delegation/shell container,
        /// meaning the assistant is continuing after an interruption rather than
        /// starting fresh. Stored at creation time because the preceding container
        /// may be dropped by graduation before rendering.
        is_continuation: bool,
    },

    /// Group of consecutive tool calls (rendered compactly)
    ToolGroup {
        id: String,
        tools: Vec<CachedToolCall>,
    },

    /// Agent task execution (subagent or delegation)
    AgentTask { id: String, agent: CachedSubagent },

    /// Shell command execution
    ShellExecution {
        id: String,
        shell: CachedShellExecution,
    },

    /// System-level message (info, warnings)
    SystemMessage { id: String, content: String },
}

impl ChatContainer {
    /// The kind of this container (used for spacing decisions).
    pub fn kind(&self) -> ContainerKind {
        match self {
            Self::UserMessage { .. } => ContainerKind::UserMessage,
            Self::AssistantResponse { .. } => ContainerKind::AssistantResponse,
            Self::ToolGroup { .. } => ContainerKind::ToolGroup,
            Self::AgentTask { .. } => ContainerKind::AgentTask,
            Self::ShellExecution { .. } => ContainerKind::ShellExecution,
            Self::SystemMessage { .. } => ContainerKind::SystemMessage,
        }
    }

    /// Unique ID for this container (used for graduation)
    pub fn id(&self) -> &str {
        match self {
            Self::UserMessage { id, .. } => id,
            Self::AssistantResponse { id, .. } => id,
            Self::ToolGroup { id, .. } => id,
            Self::AgentTask { id, .. } => id,
            Self::ShellExecution { id, .. } => id,
            Self::SystemMessage { id, .. } => id,
        }
    }

    /// Whether this container is inherently complete and can graduate.
    ///
    /// For AssistantResponse, completeness depends on the turn state and
    /// position in the container list — use `ContainerList::is_response_complete()`.
    pub fn is_complete(&self) -> bool {
        match self {
            Self::UserMessage { .. } => true,
            Self::AssistantResponse { .. } => false,
            Self::ToolGroup { tools, .. } => tools.iter().all(|t| t.complete),
            Self::AgentTask { agent, .. } => agent.is_terminal(),
            Self::ShellExecution { .. } => true,
            Self::SystemMessage { .. } => true,
        }
    }

    /// Render this container to a Node tree using individual parameters.
    ///
    /// Prefer using `view_with_params()` for new code.
    pub fn view(
        &self,
        width: usize,
        spinner_frame: usize,
        show_thinking: bool,
        is_continuation: bool,
        is_complete: bool,
    ) -> Node {
        self.view_with_params(&ViewParams {
            render_state: RenderState {
                terminal_width: width as u16,
                spinner_frame,
                show_thinking,
            },
            is_continuation,
            is_complete,
            prev_is_tool_group: false,
        })
    }

    /// Render this container to a Node tree.
    ///
    /// Containers return plain content nodes — no scrollback wrappers.
    /// Graduation is handled by draining completed containers at the app layer.
    pub fn view_with_params(&self, params: &ViewParams) -> Node {
        match self {
            Self::UserMessage { content, .. } => {
                render_user_prompt(content, params.render_state.width())
            }

            Self::AssistantResponse { text, thinking, .. } => render_assistant_blocks(
                &RenderBlocksParams {
                    text,
                    thinking,
                    complete: params.is_complete,
                    is_continuation: params.is_continuation,
                },
                &params.render_state,
            ),

            Self::ToolGroup { tools, .. } => {
                let tool_nodes: Vec<Node> = tools
                    .iter()
                    .map(|t| render_tool_call_with_frame(t, params.render_state.spinner_frame))
                    .collect();

                col(tool_nodes)
            }

            Self::AgentTask { agent, .. } => {
                render_subagent(agent, params.render_state.spinner_frame)
            }

            Self::ShellExecution { shell, .. } => render_shell_execution(shell),

            Self::SystemMessage { content, .. } => render_system_message(content),
        }
    }
}
/// Parameters for rendering assistant text.
#[derive(Debug, Clone)]
struct RenderBlocksParams<'a> {
    pub text: &'a str,
    pub thinking: &'a [ThinkingBlock],
    pub complete: bool,
    pub is_continuation: bool,
}

/// Render assistant text+thinking.
fn render_assistant_blocks(params: &RenderBlocksParams, render_state: &RenderState) -> Node {
    let mut nodes = if render_state.show_thinking {
        render_blocks_full_thinking(params, render_state)
    } else {
        render_blocks_collapsed_thinking(params, render_state)
    };

    // Streaming spinner: show when not complete, unless only the collapsed
    // thinking summary is visible (it has its own spinner).
    if !params.complete {
        let has_text = !params.text.is_empty();
        let has_thinking_summary = !render_state.show_thinking && !params.thinking.is_empty();
        if has_text || !has_thinking_summary {
            let t = crate::tui::oil::theme::active();
            nodes.push(row([
                text(" "),
                spinner(None, render_state.spinner_frame)
                    .with_style(Style::new().fg(t.resolve_color(t.colors.text))),
            ]));
        }
    }

    // Gap between thinking summary/block and text content
    let has_thinking = !params.thinking.is_empty();
    let has_text = !params.text.is_empty();
    let gap = if has_thinking && has_text {
        Gap::row(1)
    } else {
        Gap::row(0)
    };
    col(nodes).gap(gap)
}

/// Render text as markdown.
fn render_text(
    text_content: &str,
    is_continuation: bool,
    has_thinking: bool,
    render_state: &RenderState,
) -> Node {
    if text_content.is_empty() {
        return Node::Empty;
    }

    let margins = if is_continuation || has_thinking {
        Margins::assistant_continuation()
    } else {
        Margins::assistant()
    };
    let style = RenderStyle::natural_with_margins(render_state.width(), margins);
    markdown_to_node_styled(text_content, style)
}

/// Render with full thinking content visible (show_thinking=true).
fn render_blocks_full_thinking(
    params: &RenderBlocksParams,
    render_state: &RenderState,
) -> Vec<Node> {
    let mut nodes = Vec::new();

    for tb in params.thinking.iter() {
        nodes.push(render_thinking_block(
            &tb.content,
            tb.token_count,
            render_state.width(),
            params.complete,
        ));
    }

    let has_thinking = !params.thinking.is_empty();
    let text_node = render_text(
        params.text,
        params.is_continuation,
        has_thinking,
        render_state,
    );
    if text_node != Node::Empty {
        nodes.push(text_node);
    }

    nodes
}

/// Render with thinking collapsed to a one-line summary (show_thinking=false).
fn render_blocks_collapsed_thinking(
    params: &RenderBlocksParams,
    render_state: &RenderState,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let has_thinking = !params.thinking.is_empty();

    if has_thinking {
        nodes.push(build_thinking_summary(params, render_state));
    }

    let text_node = render_text(
        params.text,
        params.is_continuation,
        has_thinking,
        render_state,
    );
    if text_node != Node::Empty {
        nodes.push(text_node);
    }

    nodes
}

/// Build the collapsed thinking summary node.
fn build_thinking_summary(params: &RenderBlocksParams, render_state: &RenderState) -> Node {
    let t = crate::tui::oil::theme::active();
    let total_words: usize = params
        .thinking
        .iter()
        .map(|tb| tb.content.split_whitespace().count())
        .sum();

    let has_text = !params.text.is_empty();

    let muted = Style::new()
        .fg(t.resolve_color(t.colors.text_muted))
        .italic();

    if !params.complete && total_words == 0 {
        // Just started thinking, no words yet
        row([
            text(" "),
            spinner(None, render_state.spinner_frame)
                .with_style(Style::new().fg(t.resolve_color(t.colors.text))),
            text(" Thinking…").with_style(muted),
        ])
    } else if params.complete || has_text {
        // Thinking is done (either turn complete or text has started streaming).
        // Use ◇ aligned with other chat node icons (✓, ✗, ●) at col 1.
        // Contrast hierarchy: icon + label in text_dim, word count in text_muted + italic.
        let dim = Style::new().fg(t.resolve_color(t.colors.text_dim));
        row([
            styled(" \u{25C7} ", dim),
            styled("Thought", dim),
            styled(format!(" ({} words)", total_words), muted),
        ])
    } else {
        // Still thinking, accumulating words, no text yet
        row([
            text(" "),
            spinner(None, render_state.spinner_frame)
                .with_style(Style::new().fg(t.resolve_color(t.colors.text))),
            styled(format!(" Thinking… ({} words)", total_words), muted),
        ])
    }
}

/// Render a system message.
fn render_system_message(content: &str) -> Node {
    use crate::tui::oil::node::styled;
    use crate::tui::oil::style::Style;
    let t = crate::tui::oil::theme::active();

    styled(
        format!(" * {} ", content),
        Style::new()
            .fg(t.resolve_color(t.colors.system_message))
            .italic(),
    )
}

/// Manages the list of chat containers.
///
/// Containers are the live (non-graduated) content. When content graduates
/// to stdout, the backing containers are drained from the front immediately.
pub struct ContainerList {
    containers: Vec<ChatContainer>,
    /// Whether any containers have ever graduated (for spacer logic).
    has_graduated: bool,
    /// Counter for generating unique IDs
    id_counter: u64,
    /// Whether the assistant turn is active (streaming). This stays true
    /// across tool calls until explicitly completed or cancelled.
    turn_active: bool,
    /// Kind of the last container graduated to stdout. Used for cross-frame
    /// spacing: the next batch's first container needs this to decide whether
    /// a blank line separator is needed before it.
    last_graduated_kind: Option<ContainerKind>,
}

impl Default for ContainerList {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerList {
    pub fn new() -> Self {
        Self {
            containers: Vec::new(),
            has_graduated: false,
            id_counter: 0,
            turn_active: false,
            last_graduated_kind: None,
        }
    }

    /// Generate a unique container ID with the given prefix.
    fn next_id(&mut self, prefix: &str) -> String {
        let id = format!("{}-{}", prefix, self.id_counter);
        self.id_counter += 1;
        id
    }

    /// Add a user message.
    pub fn add_user_message(&mut self, content: String) {
        let id = self.next_id("user");
        self.containers
            .push(ChatContainer::UserMessage { id, content });
    }

    /// Remove the last container if it's an empty AssistantResponse.
    /// This avoids "gap" containers that block graduation.
    fn remove_empty_trailing_response(&mut self) {
        let should_remove = matches!(
            self.containers.last(),
            Some(ChatContainer::AssistantResponse {
                text,
                thinking,
                ..
            }) if text.is_empty() && thinking.is_empty()
        );
        if should_remove {
            tracing::debug!("[containers] Removing empty trailing AssistantResponse");
            self.containers.pop();
        }
    }

    /// Whether the last container implies the next AssistantResponse is a continuation.
    fn last_implies_continuation(&self) -> bool {
        matches!(
            self.containers.last(),
            Some(
                ChatContainer::ToolGroup { .. }
                    | ChatContainer::AgentTask { .. }
                    | ChatContainer::ShellExecution { .. }
            )
        )
    }

    /// Ensure an open AssistantResponse exists and return its index.
    ///
    /// If the last container is already an AssistantResponse and the turn is active,
    /// returns its index. Otherwise creates a new one.
    fn ensure_open_response(&mut self) -> usize {
        if self.turn_active {
            if let Some(ChatContainer::AssistantResponse { .. }) = self.containers.last() {
                return self.containers.len() - 1;
            }
        }
        let is_continuation = self.last_implies_continuation();
        let last_kind = self.containers.last().map(|c| format!("{:?}", c.kind()));
        tracing::debug!(
            last_container = ?last_kind,
            is_continuation,
            total_containers = self.containers.len(),
            "[containers] Creating NEW AssistantResponse"
        );
        let id = self.next_id("assistant");
        self.containers.push(ChatContainer::AssistantResponse {
            id,
            text: String::new(),
            thinking: Vec::new(),
            is_continuation,
        });
        self.turn_active = true;
        self.containers.len() - 1
    }

    /// Start a new assistant response (called when streaming begins).
    pub fn start_assistant_response(&mut self) -> &str {
        let is_continuation = self.last_implies_continuation();
        let id = self.next_id("assistant");
        self.containers.push(ChatContainer::AssistantResponse {
            id,
            text: String::new(),
            thinking: Vec::new(),
            is_continuation,
        });
        self.turn_active = true;
        self.containers
            .last()
            .map(ChatContainer::id)
            .unwrap_or_default()
    }

    /// Append text to the current assistant response.
    /// Creates a new response if none exists (via backward scan).
    pub fn append_text(&mut self, delta: &str) {
        let idx = self.ensure_open_response();
        if let ChatContainer::AssistantResponse { text, .. } = &mut self.containers[idx] {
            // Full-text resend detection: some backends re-emit the entire
            // response as a single delta. Skip if the existing text already
            // ends with this content.
            let incoming = delta.trim_start_matches('\n');
            if incoming.len() > 50 && !text.is_empty() && text.ends_with(incoming) {
                tracing::debug!(
                    incoming_len = delta.len(),
                    existing_len = text.len(),
                    "Skipping duplicate full-text delta"
                );
                return;
            }
            text.push_str(delta);
        }
    }

    /// Set thinking content for the current assistant response.
    /// Replaces the first thinking block if one exists, otherwise pushes a new one.
    pub fn set_thinking(&mut self, content: String, token_count: usize) {
        let idx = self.ensure_open_response();
        if let Some(ChatContainer::AssistantResponse { thinking, .. }) =
            self.containers.get_mut(idx)
        {
            if let Some(first) = thinking.first_mut() {
                first.content = content;
                first.token_count = token_count;
            } else {
                thinking.push(ThinkingBlock {
                    content,
                    token_count,
                });
            }
        }
    }

    /// Append thinking content to the current assistant response.
    /// Coalesces with the last thinking block if one exists.
    pub fn append_thinking(&mut self, delta: &str) {
        let idx = self.ensure_open_response();
        if let ChatContainer::AssistantResponse { thinking, .. } = &mut self.containers[idx] {
            if let Some(last) = thinking.last_mut() {
                last.content.push_str(delta);
                last.token_count += 1;
            } else {
                thinking.push(ThinkingBlock {
                    content: delta.to_string(),
                    token_count: 1,
                });
            }
        }
    }

    /// Mark the current assistant response as complete and end the turn.
    pub fn complete_response(&mut self) {
        self.turn_active = false;
    }

    /// Add a tool call.
    /// Groups with previous tool group if one exists and is incomplete.
    /// Add a tool call.
    /// Removes empty trailing responses to avoid graduation gaps.
    pub fn add_tool_call(&mut self, tool: CachedToolCall) {
        let tool_name = tool.name.clone();
        let last_kind = self.containers.last().map(|c| format!("{:?}", c.kind()));

        // Remove empty trailing AssistantResponse to avoid graduation gaps.
        self.remove_empty_trailing_response();

        let last_kind_after = self.containers.last().map(|c| format!("{:?}", c.kind()));

        // Append to existing tool group if the last container is one.
        // Since graduated containers are already drained, any remaining
        // container is live and safe to append to.
        let can_append = matches!(
            self.containers.last(),
            Some(ChatContainer::ToolGroup { .. })
        );

        if can_append {
            if let Some(ChatContainer::ToolGroup { tools, .. }) = self.containers.last_mut() {
                tracing::debug!(
                    tool = %tool_name,
                    group_size = tools.len() + 1,
                    "[containers] Appending tool to existing ToolGroup"
                );
                tools.push(tool);
            }
        } else {
            tracing::debug!(
                tool = %tool_name,
                last_before_cleanup = ?last_kind,
                last_after_cleanup = ?last_kind_after,
                total_containers = self.containers.len(),
                "[containers] Creating NEW ToolGroup (could not append)"
            );
            let id = self.next_id("tools");
            self.containers.push(ChatContainer::ToolGroup {
                id,
                tools: vec![tool],
            });
        }
    }

    /// Update a tool call, preferring call_id match over name match.
    ///
    /// When `call_id` is provided, finds the tool with that exact call_id.
    /// Falls back to name-based lookup (most recent matching tool) when
    /// call_id is None or not found.
    pub fn update_tool(
        &mut self,
        tool_name: &str,
        call_id: Option<&str>,
        f: impl FnOnce(&mut CachedToolCall),
    ) {
        // First try: match by call_id (exact, unambiguous)
        if let Some(cid) = call_id {
            for container in self.containers.iter_mut().rev() {
                if let ChatContainer::ToolGroup { tools, .. } = container {
                    if let Some(tool) = tools.iter_mut().find(|t| t.call_id.as_deref() == Some(cid))
                    {
                        f(tool);
                        return;
                    }
                }
            }
        }
        // Fallback: match by name (most recent, for backwards compatibility)
        for container in self.containers.iter_mut().rev() {
            if let ChatContainer::ToolGroup { tools, .. } = container {
                if let Some(tool) = tools
                    .iter_mut()
                    .rev()
                    .find(|t| t.name.as_ref() == tool_name)
                {
                    f(tool);
                    return;
                }
            }
        }
    }

    /// Add an agent task (subagent or delegation).
    /// Like tools, agent tasks break the current response.
    pub fn add_agent_task(&mut self, agent: CachedSubagent, id_prefix: &str) {
        self.remove_empty_trailing_response();

        let id = self.next_id(id_prefix);
        self.containers.push(ChatContainer::AgentTask { id, agent });
    }

    /// Update an agent task by its agent ID.
    pub fn update_agent_task(&mut self, agent_id: &str, f: impl FnOnce(&mut CachedSubagent)) {
        for container in self.containers.iter_mut().rev() {
            if let ChatContainer::AgentTask { agent, .. } = container {
                if agent.id.as_ref() == agent_id {
                    f(agent);
                    return;
                }
            }
        }
    }

    /// Add a system message.
    pub fn add_system_message(&mut self, content: String) {
        let id = self.next_id("system");
        self.containers
            .push(ChatContainer::SystemMessage { id, content });
    }

    /// Drain completed containers from the front and render them for stdout.
    ///
    /// All graduating containers are rendered together through ONE Taffy pass
    /// with the same grouping logic as the viewport (`gap(1)` between groups,
    /// `gap(0)` for consecutive tool groups). This ensures stdout spacing is
    /// identical to what Taffy would produce in the viewport.
    ///
    /// Cross-frame spacing uses `last_graduated_kind` — if the previous frame
    /// graduated a container, this frame's batch gets a leading blank line
    /// (unless both sides are ToolGroups).
    pub fn drain_completed(
        &mut self,
        width: u16,
        spinner_frame: usize,
        show_thinking: bool,
    ) -> String {
        use crucible_oil::render::render_to_string;

        let mut batch: Vec<(ContainerKind, Node)> = Vec::new();

        while !self.containers.is_empty() {
            if !self.is_container_graduatable(0) {
                break;
            }
            let container = self.containers.remove(0);
            let kind = container.kind();
            let node = container.view(width as usize, spinner_frame, show_thinking, false, true);
            batch.push((kind, node));
            self.has_graduated = true;
        }

        if batch.is_empty() {
            return String::new();
        }

        let first_kind = batch[0].0;
        let last_kind = batch[batch.len() - 1].0;
        let batch_kinds: Vec<_> = batch.iter().map(|(k, _)| format!("{:?}", k)).collect();
        let prev_kind = self.last_graduated_kind;
        let adds_spacing = prev_kind.map(|pk| needs_spacing(pk, first_kind)).unwrap_or(false);

        tracing::debug!(
            batch = ?batch_kinds,
            prev_graduated = ?prev_kind.map(|k| format!("{:?}", k)),
            adds_spacing,
            remaining_containers = self.containers.len(),
            "[graduation] Graduating batch"
        );

        // Group using the same logic as render_containers():
        // consecutive ToolGroups → tight sub-col (gap=0), everything else → gap(1)
        let grouped = group_container_nodes(batch);

        // One Taffy pass for the entire batch
        let rendered = render_to_string(&grouped, width as usize);

        // Cross-frame spacing: blank line between previous batch's last
        // container and this batch's first, unless both are ToolGroups.
        let mut output = String::new();
        if adds_spacing {
            output.push_str("\r\n");
        }
        output.push_str(&rendered);
        output.push_str("\r\n");

        self.last_graduated_kind = Some(last_kind);
        output
    }

    /// Whether the container at the given index can be graduated.
    fn is_container_graduatable(&self, index: usize) -> bool {
        match &self.containers[index] {
            ChatContainer::AssistantResponse { .. } => self.is_response_complete(index),
            other => other.is_complete(),
        }
    }

    /// Get all live (non-graduated) containers.
    pub fn containers(&self) -> &[ChatContainer] {
        &self.containers
    }

    /// Whether any containers have ever graduated to stdout.
    pub fn has_graduated(&self) -> bool {
        self.has_graduated
    }

    /// Kind of the last container that graduated to stdout.
    /// Used by `render_containers()` to seed `prev_kind` for the first
    /// viewport container's spacing/continuation decisions.
    pub fn last_graduated_kind(&self) -> Option<ContainerKind> {
        self.last_graduated_kind
    }

    /// Check if there are any containers.
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
    }

    /// Get container count.
    pub fn len(&self) -> usize {
        self.containers.len()
    }

    /// Drop containers from the front by ID (test helper).
    #[cfg(test)]
    pub fn graduate(&mut self, graduated_ids: &[String]) {
        let count = self
            .containers
            .iter()
            .take_while(|c| graduated_ids.iter().any(|id| id == c.id()))
            .count();
        if count > 0 {
            self.containers.drain(0..count);
            self.has_graduated = true;
        }
    }

    /// Whether the assistant turn is active (streaming, tool calls, etc.)
    pub fn is_streaming(&self) -> bool {
        self.turn_active
    }

    /// Derive whether an AssistantResponse at the given index is complete.
    ///
    /// A response is complete if the turn is not active, or if there is
    /// any container after it in the list.
    pub fn is_response_complete(&self, index: usize) -> bool {
        if !self.turn_active {
            return true;
        }
        // Complete if anything follows this response
        index + 1 < self.containers.len()
    }

    /// Whether the container list needs a turn-level spinner appended.
    ///
    /// Returns true when the turn is active but the last container doesn't
    /// already show a spinner (e.g. all tools are complete, no open
    /// AssistantResponse). This fills the gap between tool completion and
    /// the next event (TextDelta, another ToolCall, or StreamComplete).
    pub fn needs_turn_spinner(&self) -> bool {
        if !self.turn_active {
            return false;
        }
        match self.containers.last() {
            // AssistantResponse at the end of an active turn is incomplete
            // and already shows its own spinner
            Some(ChatContainer::AssistantResponse { .. }) => false,
            // ToolGroup with any pending tool already shows braille spinners
            Some(ChatContainer::ToolGroup { tools, .. }) => tools.iter().all(|t| t.complete),
            Some(ChatContainer::AgentTask { agent, .. }) => agent.is_terminal(),
            _ => true,
        }
    }

    /// Mark the turn as active without starting an assistant response.
    /// Used when a tool call or subagent starts before any text.
    pub fn mark_turn_active(&mut self) {
        self.turn_active = true;
    }

    /// Cancel streaming — sets turn_active to false, removes empty trailing response.
    pub fn cancel_streaming(&mut self) {
        self.remove_empty_trailing_response();
        self.turn_active = false;
    }

    /// Add a shell execution record.
    pub fn add_shell_execution(&mut self, shell: CachedShellExecution) {
        let id = self.next_id("shell");
        self.containers
            .push(ChatContainer::ShellExecution { id, shell });
    }

    /// Find the most recent tool with the given name.
    pub fn find_tool(&self, name: &str) -> Option<&CachedToolCall> {
        for container in self.containers.iter().rev() {
            if let ChatContainer::ToolGroup { tools, .. } = container {
                if let Some(tool) = tools.iter().rev().find(|t| t.name.as_ref() == name) {
                    return Some(tool);
                }
            }
        }
        None
    }

    /// Find the most recent tool with the given name (mutable).
    pub fn find_tool_mut(&mut self, name: &str) -> Option<&mut CachedToolCall> {
        for container in self.containers.iter_mut().rev() {
            if let ChatContainer::ToolGroup { tools, .. } = container {
                if let Some(tool) = tools.iter_mut().rev().find(|t| t.name.as_ref() == name) {
                    return Some(tool);
                }
            }
        }
        None
    }

    pub fn supersede_most_recent_tool(&mut self, name: &str) -> bool {
        for container in self.containers.iter_mut().rev() {
            if let ChatContainer::ToolGroup { tools, .. } = container {
                if let Some(tool) = tools
                    .iter_mut()
                    .rev()
                    .find(|t| t.name.as_ref() == name && !t.superseded)
                {
                    tool.superseded = true;
                    return true;
                }
            }
        }
        false
    }

    /// Get the full output of a tool by name.
    pub fn get_tool_output(&self, name: &str) -> Option<String> {
        self.find_tool(name).map(|t| t.result())
    }

    /// Set the output file path for a tool by name.
    pub fn set_tool_output_path(&mut self, name: &str, path: std::path::PathBuf) {
        if let Some(tool) = self.find_tool_mut(name) {
            tool.set_output_path(path);
        }
    }

    /// Clear all containers.
    pub fn clear(&mut self) {
        self.containers.clear();
        self.has_graduated = false;
        self.turn_active = false;
        self.last_graduated_kind = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_container_ids_stable() {
        let mut list = ContainerList::new();

        list.add_user_message("Hello".to_string());
        let user_id = list.containers[0].id().to_string();

        list.start_assistant_response();
        let asst_id = list.containers[1].id().to_string();

        list.append_text("Response");
        list.complete_response();

        // IDs should remain stable
        assert_eq!(list.containers[0].id(), user_id);
        assert_eq!(list.containers[1].id(), asst_id);
    }

    #[test]
    fn test_text_accumulates_as_single_string() {
        let mut list = ContainerList::new();

        list.start_assistant_response();
        list.append_text("Block 1\n\nBlock 2\n\nBlock 3");

        if let Some(ChatContainer::AssistantResponse { text, .. }) = list.containers.last() {
            assert_eq!(text, "Block 1\n\nBlock 2\n\nBlock 3");
        } else {
            panic!("Expected AssistantResponse");
        }
    }

    #[test]
    fn test_incremental_text_append() {
        let mut list = ContainerList::new();

        list.start_assistant_response();
        list.append_text("First ");
        list.append_text("part\n\n");
        list.append_text("Second part");

        if let Some(ChatContainer::AssistantResponse { text, .. }) = list.containers.last() {
            assert_eq!(text, "First part\n\nSecond part");
        } else {
            panic!("Expected AssistantResponse");
        }
    }

    #[test]
    fn thinking_coalesces() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_thinking("plan ");
        list.append_thinking("more");

        let thinking = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { thinking, .. }) => thinking,
            _ => panic!("Expected AssistantResponse"),
        };

        assert_eq!(thinking.len(), 1);
        assert_eq!(thinking[0].content, "plan more");
        assert_eq!(thinking[0].token_count, 2);
    }

    #[test]
    fn thinking_and_text_are_separate_fields() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("first");
        list.append_thinking("thought");
        list.append_text("\n\nsecond");

        match list.containers.last() {
            Some(ChatContainer::AssistantResponse { text, thinking, .. }) => {
                assert_eq!(text, "first\n\nsecond");
                assert_eq!(thinking.len(), 1);
                assert_eq!(thinking[0].content, "thought");
            }
            _ => panic!("Expected AssistantResponse"),
        }
    }

    #[test]
    fn append_thinking_after_text() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("Here is my answer");
        list.append_thinking("let me reason about this");

        match list.containers.last() {
            Some(ChatContainer::AssistantResponse { text, thinking, .. }) => {
                assert_eq!(text, "Here is my answer");
                assert_eq!(thinking.len(), 1);
                assert_eq!(thinking[0].content, "let me reason about this");
            }
            _ => panic!("Expected AssistantResponse"),
        }
    }

    #[test]
    fn set_thinking_replaces_existing() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_thinking("initial");
        list.append_text("some text");
        list.set_thinking("replaced".to_string(), 50);

        match list.containers.last() {
            Some(ChatContainer::AssistantResponse { text, thinking, .. }) => {
                assert_eq!(text, "some text");
                assert_eq!(thinking.len(), 1);
                assert_eq!(thinking[0].content, "replaced");
                assert_eq!(thinking[0].token_count, 50);
            }
            _ => panic!("Expected AssistantResponse"),
        }
    }

    #[test]
    fn append_thinking_coalesces_across_text() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_thinking("phase one ");
        list.append_text("response");
        list.append_thinking("phase two");

        match list.containers.last() {
            Some(ChatContainer::AssistantResponse { text, thinking, .. }) => {
                assert_eq!(text, "response");
                assert_eq!(thinking.len(), 1);
                assert_eq!(thinking[0].content, "phase one phase two");
            }
            _ => panic!("Expected AssistantResponse"),
        }
    }

    #[test]
    fn text_coalesces() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("Hello");
        list.append_text(" world");

        match list.containers.last() {
            Some(ChatContainer::AssistantResponse { text, .. }) => {
                assert_eq!(text, "Hello world");
            }
            _ => panic!("Expected AssistantResponse"),
        }
    }

    /// Full-text resend detection: the third delta is the complete text re-sent.
    #[test]
    fn full_text_resend_is_deduplicated() {
        let mut list = ContainerList::new();
        list.add_user_message("Say hello in one sentence".to_string());
        list.start_assistant_response();

        list.append_text("\nHello \u{2014} I'm ");
        list.append_text(
            "here to help with the Crucible codebase or anything else you're working on.",
        );
        list.append_text("\nHello \u{2014} I'm here to help with the Crucible codebase or anything else you're working on.");

        let text = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { text, .. }) => text.as_str(),
            _ => panic!("Expected AssistantResponse"),
        };

        let greeting = "Hello \u{2014} I'm here to help with the Crucible codebase or anything else you're working on.";
        let count = text.matches(greeting).count();
        assert_eq!(
            count, 1,
            "Expected greeting to appear exactly once, but appeared {count} times.\nText: {text}"
        );
    }

    #[test]
    fn test_is_complete() {
        let mut list = ContainerList::new();

        // User messages are always complete
        list.add_user_message("Hello".to_string());
        assert!(list.containers[0].is_complete());

        // AssistantResponse.is_complete() always returns false (needs context)
        list.start_assistant_response();
        assert!(!list.containers[1].is_complete());

        // But ContainerList::is_response_complete() derives from turn state
        assert!(
            !list.is_response_complete(1),
            "Active turn, nothing after => incomplete"
        );

        list.complete_response();
        assert!(list.is_response_complete(1), "Turn ended => complete");
    }

    #[test]
    fn test_graduation() {
        let mut list = ContainerList::new();

        list.add_user_message("Hello".to_string());
        list.start_assistant_response();
        list.append_text("Response");
        list.complete_response();

        let user_id = list.containers[0].id().to_string();

        // Initially both containers present
        assert_eq!(list.containers().len(), 2);
        assert!(!list.has_graduated());

        // Graduate user message — it gets dropped
        list.graduate(&[user_id]);

        // Only assistant response remains
        assert_eq!(list.containers().len(), 1);
        assert!(list.has_graduated());
        assert!(matches!(
            &list.containers()[0],
            ChatContainer::AssistantResponse { .. }
        ));
    }

    #[test]
    fn test_container_view_produces_output() {
        let mut list = ContainerList::new();

        list.add_user_message("Test user message".to_string());
        list.start_assistant_response();
        list.append_text("Test assistant response");
        list.complete_response();

        // Render each container and verify it produces a non-empty node
        for container in list.containers() {
            let node = container.view(80, 0, false, false, true);
            assert!(
                !matches!(node, Node::Empty),
                "Container should render non-empty"
            );
        }
    }

    #[test]
    fn test_tool_group_renders() {
        use std::time::Instant;

        let mut list = ContainerList::new();

        // Create a minimal CachedToolCall
        let tool = CachedToolCall {
            id: "test-tool-1".to_string(),
            name: std::sync::Arc::from("read_file"),
            args: std::sync::Arc::from(r#"{"path": "/tmp/test.txt"}"#),
            call_id: None,
            output_tail: std::collections::VecDeque::new(),
            output_path: None,
            output_total_bytes: 0,
            error: None,
            started_at: Instant::now(),
            complete: true,
            superseded: false,
            description: None,
            source: None,
            lua_primary_arg: None,
        };

        list.add_tool_call(tool);

        let containers = list.containers();
        assert_eq!(containers.len(), 1);
        assert!(matches!(containers[0], ChatContainer::ToolGroup { .. }));

        // Verify rendering
        let node = containers[0].view(80, 0, false, false, true);
        assert!(!matches!(node, Node::Empty));
    }

    #[test]
    fn test_container_ids_used_in_view() {
        use crate::tui::oil::render::render_to_string;

        let mut list = ContainerList::new();

        list.add_user_message("Hello".to_string());
        list.start_assistant_response();
        list.append_text("World");
        list.complete_response();

        // Get container IDs
        let user_id = list.containers[0].id().to_string();
        let asst_id = list.containers[1].id().to_string();

        // Render containers
        let user_node = list.containers[0].view(80, 0, false, false, true);
        let asst_node = list.containers[1].view(80, 0, false, false, true);

        // Check that rendered nodes use the container IDs
        // We can verify this by rendering and seeing the output contains content
        let user_output = render_to_string(&user_node, 80);
        let asst_output = render_to_string(&asst_node, 80);

        assert!(
            user_output.contains("Hello"),
            "User content should be rendered"
        );
        assert!(
            asst_output.contains("World"),
            "Assistant content should be rendered"
        );

        // The IDs should be stable
        assert!(user_id.starts_with("user-"));
        assert!(asst_id.starts_with("assistant-"));
    }

    #[test]
    fn test_graduation_removes_from_viewport() {
        let mut list = ContainerList::new();

        list.add_user_message("First".to_string());
        list.add_user_message("Second".to_string());
        list.add_user_message("Third".to_string());

        assert_eq!(list.containers().len(), 3);

        // Graduate first two
        let ids: Vec<String> = list.containers[..2]
            .iter()
            .map(|c| c.id().to_string())
            .collect();
        list.graduate(&ids);

        // Only third should remain
        assert_eq!(list.containers().len(), 1);
        assert!(list.containers()[0].id().contains("user-"));
    }

    #[test]
    fn test_update_tool_by_name() {
        use std::time::Instant;

        let mut list = ContainerList::new();

        // Create a tool with id="tool-123" and name="read_file"
        let tool = CachedToolCall {
            id: "tool-123".to_string(),
            name: std::sync::Arc::from("read_file"),
            args: std::sync::Arc::from("{}"),
            call_id: None,
            output_tail: std::collections::VecDeque::new(),
            output_path: None,
            output_total_bytes: 0,
            error: None,
            started_at: Instant::now(),
            complete: false,
            superseded: false,
            description: None,
            source: None,
            lua_primary_arg: None,
        };

        list.add_tool_call(tool);

        // Verify tool is not complete
        if let Some(ChatContainer::ToolGroup { tools, .. }) = list.containers.last() {
            assert!(!tools[0].complete, "Tool should start incomplete");
        } else {
            panic!("Expected ToolGroup");
        }

        // Update by name (not id)
        list.update_tool("read_file", None, |t| {
            t.mark_complete();
        });

        // Verify tool is now complete
        if let Some(ChatContainer::ToolGroup { tools, .. }) = list.containers.last() {
            assert!(
                tools[0].complete,
                "Tool should be complete after update_tool by name"
            );
        } else {
            panic!("Expected ToolGroup");
        }
    }

    /// Test reproducing the property test minimal failing input
    #[test]
    fn test_tool_complete_renders_checkmark() {
        use crate::tui::oil::ansi::strip_ansi;
        use crate::tui::oil::app::{App, ViewContext};
        use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
        use crate::tui::oil::focus::FocusContext;
        use crate::tui::oil::TestRuntime;

        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Query".to_string()));
        app.on_message(ChatAppMsg::TextDelta("A".to_string()));
        app.on_message(ChatAppMsg::TextDelta("a".to_string()));
        app.on_message(ChatAppMsg::TextDelta("text".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "___".to_string(),
            args: r#"{"query": "test"}"#.to_string(),
            call_id: None,
            description: None,
            source: None,
            lua_primary_arg: None,
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "___".to_string(),
            delta: "result".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "___".to_string(),
            call_id: None,
        });
        app.on_message(ChatAppMsg::TextDelta("after text".to_string()));
        app.on_message(ChatAppMsg::StreamComplete);

        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let tree = app.view(&ctx);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        assert!(
            combined.contains("___"),
            "Tool name should appear in output:\n{}",
            combined
        );

        let checkmark_count = combined.matches('\u{2713}').count();
        assert!(
            checkmark_count >= 1,
            "Should have at least 1 checkmark for completed tool, found {}. Output:\n{}",
            checkmark_count,
            combined
        );
    }

    /// Same test but rendering after each event like the property test
    #[test]
    fn test_tool_complete_with_incremental_rendering() {
        use crate::tui::oil::ansi::strip_ansi;
        use crate::tui::oil::app::{App, ViewContext};
        use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
        use crate::tui::oil::focus::FocusContext;
        use crate::tui::oil::TestRuntime;

        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();
        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);

        app.on_message(ChatAppMsg::UserMessage("Query".to_string()));

        // Render after each event like the property test does
        let events = vec![
            ("TextDelta A", ChatAppMsg::TextDelta("A".to_string())),
            ("TextDelta a", ChatAppMsg::TextDelta("a".to_string())),
            ("TextDelta p...", ChatAppMsg::TextDelta("p,..,.K6,zg8 sb?L6,,,nV8th3ZYf.,6G,C,.".to_string())),
            ("ToolCall ___", ChatAppMsg::ToolCall {
                name: "___".to_string(),
                args: r#"{"query": "test"}"#.to_string(),
                call_id: None,
                description: None,
                source: None,
            lua_primary_arg: None,
            }),
            ("ToolResultDelta ___", ChatAppMsg::ToolResultDelta {
                name: "___".to_string(),
                delta: "Q_   __ge95.AYs 5sD_9.hQ._HD-1.K_I-N3L-0E  wL".to_string(),
                call_id: None,
            }),
            ("ToolResultComplete ___", ChatAppMsg::ToolResultComplete { name: "___".to_string(), call_id: None }),
            ("TextDelta ?i...", ChatAppMsg::TextDelta("?i !, U,!9 i0.vnKn Az?0!DQ7rt  Xp!u7cQ6ZrrtA ,Xyk?,J,,,4h,zw,bB7Mi,?j!Ay!1tx??,,?.?.bK,Z".to_string())),
        ];

        for (desc, event) in events {
            app.on_message(event);

            // Debug: print container state
            eprintln!("\n=== After {} ===", desc);
            for (i, c) in app.container_list().containers().iter().enumerate() {
                match c {
                    ChatContainer::UserMessage { id, content } => {
                        eprintln!("{}: User({}): {:.30}", i, id, content);
                    }
                    ChatContainer::AssistantResponse {
                        id, text, thinking, ..
                    } => {
                        eprintln!(
                            "{}: Asst({}): text={:?} thinking={}",
                            i,
                            id,
                            text,
                            thinking.len()
                        );
                    }
                    ChatContainer::ToolGroup { id, tools } => {
                        for t in tools {
                            eprintln!("{}: Tool({}, {}): complete={}", i, id, t.name, t.complete);
                        }
                    }
                    _ => {}
                }
            }

            let tree = app.view(&ctx);
            runtime.render(&tree);
        }

        app.on_message(ChatAppMsg::StreamComplete);

        eprintln!("\n=== After StreamComplete ===");
        for (i, c) in app.container_list().containers().iter().enumerate() {
            match c {
                ChatContainer::UserMessage { id, content } => {
                    eprintln!("{}: User({}): {:.30}", i, id, content);
                }
                ChatContainer::AssistantResponse {
                    id, text, thinking, ..
                } => {
                    eprintln!(
                        "{}: Asst({}): text={:?} thinking={}",
                        i,
                        id,
                        text,
                        thinking.len()
                    );
                }
                ChatContainer::ToolGroup { id, tools } => {
                    for t in tools {
                        eprintln!("{}: Tool({}, {}): complete={}", i, id, t.name, t.complete);
                    }
                }
                _ => {}
            }
        }

        let tree = app.view(&ctx);
        runtime.render(&tree);

        let stdout = strip_ansi(runtime.stdout_content());
        let viewport = strip_ansi(runtime.viewport_content());
        let combined = format!("{}{}", stdout, viewport);

        eprintln!("\n=== STDOUT ===\n{}", stdout);
        eprintln!("\n=== VIEWPORT ===\n{}", viewport);

        assert!(
            combined.contains("___"),
            "Tool name should appear in output:\n{}",
            combined
        );

        let checkmark_count = combined.matches('\u{2713}').count();
        assert!(
            checkmark_count >= 1,
            "Should have at least 1 checkmark for completed tool, found {}. Output:\n{}",
            checkmark_count,
            combined
        );
    }

    #[test]
    fn test_is_streaming() {
        let mut list = ContainerList::new();
        assert!(!list.is_streaming());

        list.start_assistant_response();
        assert!(list.is_streaming());

        list.complete_response();
        assert!(!list.is_streaming());
    }

    #[test]
    fn test_cancel_streaming() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        assert!(list.is_streaming());

        list.cancel_streaming();
        assert!(!list.is_streaming());
    }

    #[test]
    fn test_shell_execution() {
        let mut list = ContainerList::new();
        let shell = CachedShellExecution::new("s1", "ls -la", 0, vec!["file.rs".to_string()], None);
        list.add_shell_execution(shell);

        assert_eq!(list.len(), 1);
        assert!(matches!(
            &list.containers[0],
            ChatContainer::ShellExecution { .. }
        ));
        assert!(list.containers[0].is_complete());
    }

    #[test]
    fn test_find_tool() {
        use std::time::Instant;

        let mut list = ContainerList::new();
        let tool = CachedToolCall {
            id: "t1".to_string(),
            name: std::sync::Arc::from("read_file"),
            args: std::sync::Arc::from("{}"),
            call_id: None,
            output_tail: std::collections::VecDeque::new(),
            output_path: None,
            output_total_bytes: 0,
            error: None,
            started_at: Instant::now(),
            complete: false,
            superseded: false,
            description: None,
            source: None,
            lua_primary_arg: None,
        };
        list.add_tool_call(tool);

        assert!(list.find_tool("read_file").is_some());
        assert!(list.find_tool("write_file").is_none());
    }

    #[test]
    fn text_contains_ordered_list() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("1. First item\n\n2. Second item\n\n3. Third item");

        if let Some(ChatContainer::AssistantResponse { text, .. }) = list.containers.last() {
            assert!(text.contains("1. First item"));
            assert!(text.contains("2. Second item"));
            assert!(text.contains("3. Third item"));
        } else {
            panic!("Expected AssistantResponse");
        }
    }

    #[test]
    fn thinking_renders_before_text() {
        use crucible_oil::render::render_to_plain_text;

        let thinking = vec![ThinkingBlock {
            content: "my deep thought".to_string(),
            token_count: 5,
        }];

        let params = super::RenderBlocksParams {
            text: "First paragraph\n\nSecond paragraph",
            thinking: &thinking,
            complete: true,
            is_continuation: false,
        };
        let render_state = super::RenderState {
            terminal_width: 80,
            spinner_frame: 0,
            show_thinking: true,
        };

        let node = super::render_assistant_blocks(&params, &render_state);
        let output = render_to_plain_text(&node, 80);

        let think_pos = output.find("my deep thought").expect("thinking missing");
        let first_pos = output.find("First paragraph").expect("first text missing");
        let second_pos = output
            .find("Second paragraph")
            .expect("second text missing");

        // Thinking renders first, then text
        assert!(
            think_pos < first_pos,
            "thinking should appear before first text"
        );
        assert!(
            first_pos < second_pos,
            "first text should appear before second text"
        );
    }

    #[test]
    fn ctrl_t_toggles_display_density() {
        use crucible_oil::render::render_to_plain_text;

        let lines = [
            "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
            "juliet",
        ];
        let long_thinking = lines.join("\n");
        let thinking = vec![ThinkingBlock {
            content: long_thinking,
            token_count: 50,
        }];

        let params = super::RenderBlocksParams {
            text: "Response text",
            thinking: &thinking,
            complete: true,
            is_continuation: false,
        };

        let full = super::render_assistant_blocks(
            &params,
            &super::RenderState {
                terminal_width: 80,
                spinner_frame: 0,
                show_thinking: true,
            },
        );
        let full_output = render_to_plain_text(&full, 80);

        let bounded_node = super::render_assistant_blocks(
            &params,
            &super::RenderState {
                terminal_width: 80,
                spinner_frame: 0,
                show_thinking: false,
            },
        );
        let bounded_output = render_to_plain_text(&bounded_node, 80);

        assert!(
            full_output.contains("alpha"),
            "full mode should show all lines"
        );
        assert!(
            full_output.contains("juliet"),
            "full mode should show all lines"
        );

        assert!(
            !bounded_output.contains("alpha"),
            "summary mode should hide thinking content:\n{bounded_output}"
        );
        assert!(
            !bounded_output.contains("juliet"),
            "summary mode should hide thinking content:\n{bounded_output}"
        );
        assert!(
            bounded_output.contains("Thought"),
            "summary mode should show 'Thought (N words)' summary:\n{bounded_output}"
        );
        assert!(
            bounded_output.contains("words"),
            "summary mode should show word count:\n{bounded_output}"
        );
    }

    #[test]
    fn thinking_summary_spinner_stops_when_text_starts_streaming() {
        use crucible_oil::render::render_to_plain_text;

        let spinner_chars: Vec<char> = vec!['\u{25D0}', '\u{25D3}', '\u{25D1}', '\u{25D2}'];

        let thinking = vec![ThinkingBlock {
            content: "let me reason about this carefully".to_string(),
            token_count: 10,
        }];

        let params = super::RenderBlocksParams {
            text: "Here is my response so far",
            thinking: &thinking,
            complete: false,
            is_continuation: false,
        };
        let render_state = super::RenderState {
            terminal_width: 80,
            spinner_frame: 0,
            show_thinking: false,
        };

        let node = super::render_assistant_blocks(&params, &render_state);
        let output = render_to_plain_text(&node, 80);

        let thinking_line = output
            .lines()
            .find(|l| l.contains("Thinking") || l.contains("Thought"))
            .expect("should have thinking summary line");

        assert!(
            !spinner_chars.iter().any(|c| thinking_line.contains(*c)),
            "Thinking summary should not show spinner once text is streaming.\nLine: {thinking_line}\nFull output:\n{output}"
        );

        let has_trailing_spinner = output
            .lines()
            .last()
            .map(|l| spinner_chars.iter().any(|c| l.contains(*c)))
            .unwrap_or(false);
        assert!(
            has_trailing_spinner,
            "Should have a trailing spinner for streaming text.\nFull output:\n{output}"
        );
    }

    #[test]
    fn completed_thinking_summary_uses_thinking_icon_not_box_corner() {
        use crucible_oil::render::render_to_plain_text;

        let thinking = vec![ThinkingBlock {
            content: "deep thoughts about architecture".to_string(),
            token_count: 8,
        }];

        let params = super::RenderBlocksParams {
            text: "Here is my answer.",
            thinking: &thinking,
            complete: true,
            is_continuation: false,
        };
        let render_state = super::RenderState {
            terminal_width: 80,
            spinner_frame: 0,
            show_thinking: false,
        };

        let node = super::render_assistant_blocks(&params, &render_state);
        let output = render_to_plain_text(&node, 80);

        let thought_line = output
            .lines()
            .find(|l| l.contains("Thought"))
            .expect("should have 'Thought (N words)' line");

        assert!(
            !thought_line.contains('\u{250C}'),
            "Collapsed thinking summary should not use box-drawing corner\nLine: {thought_line}"
        );
    }

    // Tests graduation_wraps_stable_blocks_in_scrollback and
    // graduation_wraps_all_blocks_when_complete removed: they relied on
    // Node::Static which no longer exists. Graduation is now automatic
    // via drain_completed.
}
