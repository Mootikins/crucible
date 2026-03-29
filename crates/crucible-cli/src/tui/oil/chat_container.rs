//! Semantic containers for chat content.
//!
//! This module provides a layer of abstraction between raw cache items and the node tree.
//! Each container represents a logical unit of content that graduates together:
//! - UserMessage: A single user prompt
//! - AssistantResponse: Text blocks + optional thinking (may span multiple deltas)
//! - ToolGroup: Consecutive tool calls grouped together
//! - SystemMessage: System-level messages

use crate::tui::oil::components::{
    render_shell_execution, render_subagent, render_thinking_block, render_tool_call_with_frame,
    render_user_prompt,
};
use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::node::{col, row, scrollback, spinner, styled, text, Direction, Node};
use crate::tui::oil::render_state::RenderState;
use crate::tui::oil::style::{Padding, Style};

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
}

/// A block of thinking content with token count.
#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    pub content: String,
    pub token_count: usize,
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
        })
    }

    /// Render this container to a Node tree.
    ///
    /// For most containers, content is wrapped in scrollback with the container's ID.
    /// For AssistantResponse, each completed block gets its own scrollback to enable
    /// incremental graduation during streaming.
    pub fn view_with_params(&self, params: &ViewParams) -> Node {
        match self {
            Self::UserMessage { id, content } => {
                let content_node = render_user_prompt(content, params.render_state.width());
                scrollback(id.clone(), [content_node])
            }

            Self::AssistantResponse {
                id,
                text,
                thinking,
                ..
            } => render_assistant_blocks_with_graduation(
                &RenderBlocksParams {
                    container_id: id,
                    text,
                    thinking,
                    complete: params.is_complete,
                    is_continuation: params.is_continuation,
                },
                &params.render_state,
            ),

            Self::ToolGroup { id, tools } => {
                // Each tool call graduates individually when complete.
                // Running tools stay in viewport so spinners animate.
                let tool_nodes: Vec<Node> = tools
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let node =
                            render_tool_call_with_frame(t, params.render_state.spinner_frame);
                        if t.complete {
                            scrollback(format!("{id}-tool-{i}"), [node])
                        } else {
                            node
                        }
                    })
                    .collect();

                col(tool_nodes).with_margin(Padding {
                    top: 1,
                    ..Default::default()
                })
            }

            Self::AgentTask { id, agent } => {
                render_subagent_container(id, agent, params.render_state.spinner_frame)
            }

            Self::ShellExecution { id, shell } => {
                let content = render_shell_execution(shell);
                scrollback(id.clone(), [content])
            }

            Self::SystemMessage { id, content } => {
                let content_node = render_system_message(content);
                scrollback(id.clone(), [content_node])
            }
        }
    }
}
/// Parameters for rendering assistant text with graduation support.
#[derive(Debug, Clone)]
struct RenderBlocksParams<'a> {
    pub container_id: &'a str,
    pub text: &'a str,
    pub thinking: &'a [ThinkingBlock],
    pub complete: bool,
    pub is_continuation: bool,
}

/// Render assistant text+thinking with graduation support.
///
/// Thinking blocks render before or interleaved with text (depending on
/// show_thinking). Text is parsed as a single markdown string; top-level
/// AST nodes are decomposed and graduated individually.
fn render_assistant_blocks_with_graduation(
    params: &RenderBlocksParams,
    render_state: &RenderState,
) -> Node {
    let mut nodes = if render_state.show_thinking {
        render_blocks_full_thinking(params, render_state)
    } else {
        render_blocks_collapsed_thinking(params, render_state)
    };

    // Streaming spinners (shared logic)
    let has_text = !params.text.is_empty();
    let has_thinking_summary = !render_state.show_thinking && !params.thinking.is_empty();

    if !params.complete && !has_text && !has_thinking_summary {
        let t = crate::tui::oil::theme::active();
        nodes.push(
            row([
                text(" "),
                spinner(None, render_state.spinner_frame)
                    .with_style(Style::new().fg(t.resolve_color(t.colors.text))),
            ])
            .with_margin(Padding {
                top: 1,
                ..Default::default()
            }),
        );
    } else if !params.complete && has_text {
        let t = crate::tui::oil::theme::active();
        nodes.push(row([
            text(" "),
            spinner(None, render_state.spinner_frame)
                .with_style(Style::new().fg(t.resolve_color(t.colors.text))),
        ]));
    }

    col(nodes)
}

/// Decompose a markdown Node into graduatable block groups.
///
/// The markdown renderer produces a `col([...])` where children include:
/// - Content nodes (text lines, styled spans, boxes for code/tables)
/// - Spacer nodes (empty text `""` between top-level blocks)
///
/// A single paragraph that word-wraps becomes multiple consecutive text
/// nodes (one per line). We split ONLY at spacer boundaries to keep
/// wrapped paragraphs and list items together.
fn decompose_top_level_blocks(node: Node) -> Vec<Node> {
    match node {
        Node::Box(b) if b.direction == Direction::Column && b.children.len() > 1 => {
            let mut groups: Vec<Vec<Node>> = vec![Vec::new()];

            for child in b.children {
                let is_spacer = matches!(&child, Node::Text(t) if t.content.is_empty());
                if is_spacer {
                    // Start a new group (include the spacer in the next group
                    // so inter-block spacing is preserved)
                    if !groups.last().map_or(true, |g| g.is_empty()) {
                        groups.push(vec![child]);
                    }
                } else {
                    groups.last_mut().unwrap().push(child);
                }
            }

            groups
                .into_iter()
                .filter(|g| !g.is_empty())
                .map(|g| {
                    if g.len() == 1 {
                        g.into_iter().next().unwrap()
                    } else {
                        col(g)
                    }
                })
                .collect()
        }
        Node::Empty => vec![],
        other => vec![other],
    }
}

/// Render the accumulated text string with per-block graduation.
///
/// Parses the full text through the markdown renderer, decomposes the AST
/// into top-level blocks, and wraps completed blocks in scrollback nodes.
/// During streaming only the last block is unstable; all preceding blocks
/// are syntactically closed and safe to graduate.
fn render_text_graduated(
    text: &str,
    container_id: &str,
    complete: bool,
    is_continuation: bool,
    has_thinking: bool,
    render_state: &RenderState,
) -> Vec<Node> {
    if text.is_empty() {
        return vec![];
    }

    let margins = if is_continuation || has_thinking {
        Margins::assistant_continuation()
    } else {
        Margins::assistant()
    };
    let style = RenderStyle::natural_with_margins(render_state.width(), margins);
    let md_node = markdown_to_node_styled(text, style);
    let children = decompose_top_level_blocks(md_node);
    let len = children.len();

    children
        .into_iter()
        .enumerate()
        .map(|(i, child)| {
            // Only the first block gets top margin (separation from preceding
            // thinking/user content). Only the last block gets bottom margin.
            // Intermediate blocks have no extra margin — the markdown parser
            // already handles inter-block spacing.
            let padding = match (i == 0, i + 1 == len) {
                (true, true) => Padding::xy(0, 1),   // single block: top + bottom
                (true, false) => Padding { top: 1, ..Default::default() },
                (false, true) => Padding { bottom: 1, ..Default::default() },
                (false, false) => Padding::default(), // middle block: no extra margin
            };
            let block_node = if padding == Padding::default() {
                child
            } else {
                child.with_margin(padding)
            };

            // During streaming, all-but-last are stable (graduated).
            // When complete, all are stable.
            let is_stable = complete || i + 1 < len;
            if is_stable {
                scrollback(
                    format!("{container_id}-md-{i}"),
                    [block_node],
                )
            } else {
                block_node
            }
        })
        .collect()
}

/// Render with full thinking content visible (show_thinking=true).
fn render_blocks_full_thinking(
    params: &RenderBlocksParams,
    render_state: &RenderState,
) -> Vec<Node> {
    let mut nodes = Vec::new();

    // Render thinking blocks first
    for (i, tb) in params.thinking.iter().enumerate() {
        let thinking_node = render_thinking_block(
            &tb.content,
            tb.token_count,
            render_state.width(),
            params.complete,
        )
        .with_margin(Padding {
            top: 1,
            ..Default::default()
        });
        // Use "-thinking-summary" for the first block so toggling
        // show_thinking doesn't leave a ghost graduated node —
        // both renderers share the same key.
        let key = if i == 0 {
            format!("{}-thinking-summary", params.container_id)
        } else {
            format!("{}-thinking-{i}", params.container_id)
        };
        nodes.push(scrollback(key, [thinking_node]));
    }

    // Render text
    let has_thinking = !params.thinking.is_empty();
    nodes.extend(render_text_graduated(
        params.text,
        params.container_id,
        params.complete,
        params.is_continuation,
        has_thinking,
        render_state,
    ));

    nodes
}

/// Render with thinking collapsed to a one-line summary (show_thinking=false).
fn render_blocks_collapsed_thinking(
    params: &RenderBlocksParams,
    render_state: &RenderState,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let has_thinking = !params.thinking.is_empty();

    // Emit thinking summary first — before text.
    if has_thinking {
        let has_text = !params.text.is_empty();
        let summary_node = build_thinking_summary(params, render_state);
        if params.complete || has_text {
            nodes.push(scrollback(
                format!("{}-thinking-summary", params.container_id),
                [summary_node],
            ));
        } else {
            nodes.push(summary_node);
        }
    }

    // Render text
    nodes.extend(render_text_graduated(
        params.text,
        params.container_id,
        params.complete,
        params.is_continuation,
        has_thinking,
        render_state,
    ));

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

fn render_subagent_container(id: &str, subagent: &CachedSubagent, spinner_frame: usize) -> Node {
    let content = render_subagent(subagent, spinner_frame);
    if subagent.is_terminal() {
        scrollback(id.to_owned(), [content])
    } else {
        content
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
    .with_margin(Padding {
        top: 1,
        ..Default::default()
    })
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
        // Remove empty trailing AssistantResponse to avoid graduation gaps.
        self.remove_empty_trailing_response();

        // Append to existing tool group if the last container is one.
        // Since graduated containers are already drained, any remaining
        // container is live and safe to append to.
        let can_append = matches!(
            self.containers.last(),
            Some(ChatContainer::ToolGroup { .. })
        );

        if can_append {
            if let Some(ChatContainer::ToolGroup { tools, .. }) = self.containers.last_mut() {
                tools.push(tool);
            }
        } else {
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

    /// Drop graduated containers from the front.
    ///
    /// Graduated IDs form a monotonic prefix — count how many leading
    /// containers are graduated, then drain them in a single operation.
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

    /// Get all live (non-graduated) containers.
    pub fn containers(&self) -> &[ChatContainer] {
        &self.containers
    }

    /// Whether any containers have ever graduated to stdout.
    pub fn has_graduated(&self) -> bool {
        self.has_graduated
    }

    /// Check if there are any containers.
    pub fn is_empty(&self) -> bool {
        self.containers.is_empty()
    }

    /// Get container count.
    pub fn len(&self) -> usize {
        self.containers.len()
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
                    ChatContainer::AssistantResponse { id, text, thinking, .. } => {
                        eprintln!("{}: Asst({}): text={:?} thinking={}", i, id, text, thinking.len());
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
                ChatContainer::AssistantResponse { id, text, thinking, .. } => {
                    eprintln!("{}: Asst({}): text={:?} thinking={}", i, id, text, thinking.len());
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
            container_id: "test-1",
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

        let node = super::render_assistant_blocks_with_graduation(&params, &render_state);
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
            container_id: "test-2",
            text: "Response text",
            thinking: &thinking,
            complete: true,
            is_continuation: false,
        };

        let full = super::render_assistant_blocks_with_graduation(
            &params,
            &super::RenderState {
                terminal_width: 80,
                spinner_frame: 0,
                show_thinking: true,
            },
        );
        let full_output = render_to_plain_text(&full, 80);

        let bounded_node = super::render_assistant_blocks_with_graduation(
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
            container_id: "test-spinner",
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

        let node = super::render_assistant_blocks_with_graduation(&params, &render_state);
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
            container_id: "test-icon",
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

        let node = super::render_assistant_blocks_with_graduation(&params, &render_state);
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

    /// Graduation decomposes markdown into top-level blocks: stable blocks
    /// (all but last during streaming) get scrollback wrappers.
    #[test]
    fn graduation_wraps_stable_blocks_in_scrollback() {
        let params = super::RenderBlocksParams {
            container_id: "grad-test",
            text: "Paragraph one.\n\nParagraph two.\n\nParagraph three.",
            thinking: &[],
            complete: false,
            is_continuation: false,
        };
        let render_state = super::RenderState {
            terminal_width: 80,
            spinner_frame: 0,
            show_thinking: false,
        };

        let node = super::render_assistant_blocks_with_graduation(&params, &render_state);

        // The result should be a col with the graduated text blocks + spinner
        fn count_scrollback(node: &Node) -> usize {
            match node {
                Node::Static(_) => 1,
                Node::Box(b) => b.children.iter().map(count_scrollback).sum(),
                _ => 0,
            }
        }

        let sb_count = count_scrollback(&node);
        // During streaming: 2 of 3 paragraphs should be wrapped in scrollback
        assert!(
            sb_count >= 2,
            "Expected at least 2 scrollback wrappers during streaming, got {sb_count}"
        );
    }

    /// When complete, all top-level blocks get scrollback wrappers.
    #[test]
    fn graduation_wraps_all_blocks_when_complete() {
        let params = super::RenderBlocksParams {
            container_id: "grad-complete",
            text: "Paragraph one.\n\nParagraph two.",
            thinking: &[],
            complete: true,
            is_continuation: false,
        };
        let render_state = super::RenderState {
            terminal_width: 80,
            spinner_frame: 0,
            show_thinking: false,
        };

        let node = super::render_assistant_blocks_with_graduation(&params, &render_state);

        fn count_scrollback(node: &Node) -> usize {
            match node {
                Node::Static(_) => 1,
                Node::Box(b) => b.children.iter().map(count_scrollback).sum(),
                _ => 0,
            }
        }

        let sb_count = count_scrollback(&node);
        // When complete: both paragraphs should be wrapped
        assert!(
            sb_count >= 2,
            "Expected at least 2 scrollback wrappers when complete, got {sb_count}"
        );
    }
}
