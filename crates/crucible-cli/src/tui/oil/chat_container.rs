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
use crate::tui::oil::node::{col, row, scrollback, spinner, styled, text, Node};
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

#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text(String),
    Thinking(ThinkingBlock),
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

    /// Assistant response, may contain multiple text blocks and optional thinking
    AssistantResponse {
        id: String,
        blocks: Vec<ContentBlock>,
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

            Self::AssistantResponse { id, blocks, .. } => render_assistant_blocks_with_graduation(
                &RenderBlocksParams {
                    container_id: id,
                    blocks,
                    complete: params.is_complete,
                    is_continuation: params.is_continuation,
                },
                &params.render_state,
            ),

            Self::ToolGroup { id, tools } => {
                let content = render_tool_group(tools, &params.render_state);
                // Only wrap in scrollback (allow graduation) when all tools are
                // complete AND this container is "done" (turn ended or more content
                // follows). This prevents a completed ToolGroup from graduating
                // before the LLM sends the next tool call — which would cause the
                // second tool to appear in a separate group.
                let all_complete = tools.iter().all(|t| t.complete);
                if all_complete && params.is_complete {
                    scrollback(id.clone(), [content])
                } else {
                    content
                }
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
/// Parameters for rendering assistant text blocks with graduation support.
///
/// Bundles the parameters needed by `render_assistant_blocks_with_graduation`
/// to reduce function signature complexity.
#[derive(Debug, Clone)]
struct RenderBlocksParams<'a> {
    pub container_id: &'a str,
    pub blocks: &'a [ContentBlock],
    pub complete: bool,
    pub is_continuation: bool,
}

/// Render assistant blocks with graduation support.
///
/// Blocks are rendered in stream-arrival order: thinking blocks appear at their
/// natural position (between text blocks), not always at the top.
/// Ctrl+T toggles display density (full vs collapsed summary), not position.
///
/// Each completed block gets its own scrollback to enable incremental graduation.
/// The in-progress block stays in the viewport.
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
    let has_text = params
        .blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Text(s) if !s.is_empty()));
    let has_thinking_summary = !render_state.show_thinking
        && params
            .blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::Thinking(_)));

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

/// Render blocks with full thinking content visible (show_thinking=true).
fn render_blocks_full_thinking(
    params: &RenderBlocksParams,
    render_state: &RenderState,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let has_thinking = params
        .blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Thinking(_)));
    let (complete_text_count, _) = text_block_counts(params);
    let mut text_block_idx = 0usize;
    let mut thinking_block_idx = 0usize;

    for block in params.blocks {
        match block {
            ContentBlock::Thinking(tb) => {
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
                let key = if thinking_block_idx == 0 {
                    format!("{}-thinking-summary", params.container_id)
                } else {
                    format!("{}-thinking-{thinking_block_idx}", params.container_id)
                };
                nodes.push(scrollback(key, [thinking_node]));
                thinking_block_idx += 1;
            }
            ContentBlock::Text(content) => {
                if let Some(node) = render_text_block(
                    content,
                    params,
                    render_state,
                    text_block_idx,
                    complete_text_count,
                    has_thinking,
                ) {
                    nodes.push(node);
                }
                text_block_idx += 1;
            }
        }
    }

    nodes
}

/// Render blocks with thinking collapsed to a one-line summary (show_thinking=false).
/// The summary always renders first regardless of block order — thinking logically
/// precedes the text it produced, even if events arrived out of order.
fn render_blocks_collapsed_thinking(
    params: &RenderBlocksParams,
    render_state: &RenderState,
) -> Vec<Node> {
    let mut nodes = Vec::new();
    let has_thinking = params
        .blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Thinking(_)));
    let (complete_text_count, _) = text_block_counts(params);
    let mut text_block_idx = 0usize;

    // Emit thinking summary first — before any text blocks.
    // Graduate to scrollback once text starts streaming, so the summary
    // doesn't consume viewport space during text streaming.
    if has_thinking {
        let has_text = params
            .blocks
            .iter()
            .any(|b| matches!(b, ContentBlock::Text(s) if !s.is_empty()));
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

    // Then emit text blocks only (thinking blocks are already summarized above).
    for block in params.blocks {
        if let ContentBlock::Text(content) = block {
            if let Some(node) = render_text_block(
                content,
                params,
                render_state,
                text_block_idx,
                complete_text_count,
                has_thinking,
            ) {
                nodes.push(node);
            }
            text_block_idx += 1;
        }
    }

    nodes
}

/// Count text blocks and determine how many are complete (eligible for graduation).
fn text_block_counts(params: &RenderBlocksParams) -> (usize, usize) {
    let total: usize = params
        .blocks
        .iter()
        .filter(|b| matches!(b, ContentBlock::Text(s) if !s.is_empty()))
        .count();
    let complete = if params.complete || total == 0 {
        total
    } else {
        total.saturating_sub(1)
    };
    (complete, total)
}

/// Render a single text block with appropriate margins and graduation wrapping.
fn render_text_block(
    content: &str,
    params: &RenderBlocksParams,
    render_state: &RenderState,
    text_block_idx: usize,
    complete_text_count: usize,
    has_thinking: bool,
) -> Option<Node> {
    if content.is_empty() {
        return None;
    }

    let margins = if params.is_continuation || text_block_idx > 0 || has_thinking {
        Margins::assistant_continuation()
    } else {
        Margins::assistant()
    };
    let style = RenderStyle::natural_with_margins(render_state.width(), margins);
    let md_node = markdown_to_node_styled(content, style);

    let padding = if text_block_idx == 0 {
        Padding::xy(0, 1)
    } else {
        Padding {
            bottom: 1,
            ..Default::default()
        }
    };
    let block_node = md_node.with_margin(padding);

    if text_block_idx < complete_text_count {
        Some(scrollback(
            format!("{}-block-{text_block_idx}", params.container_id),
            [block_node],
        ))
    } else {
        Some(block_node)
    }
}

/// Build the collapsed thinking summary node.
fn build_thinking_summary(params: &RenderBlocksParams, render_state: &RenderState) -> Node {
    let t = crate::tui::oil::theme::active();
    let total_words: usize = params
        .blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Thinking(tb) => Some(tb.content.split_whitespace().count()),
            _ => None,
        })
        .sum();

    let has_text = params
        .blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Text(s) if !s.is_empty()));

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

/// Render a group of tool calls compactly.
fn render_tool_group(tools: &[CachedToolCall], render_state: &RenderState) -> Node {
    let tool_nodes: Vec<Node> = tools
        .iter()
        .map(|t| render_tool_call_with_frame(t, render_state.spinner_frame))
        .collect();

    // Tool calls are rendered tightly grouped (no gap)
    // Container has top margin for separation from previous content
    col(tool_nodes).with_margin(Padding {
        top: 1,
        ..Default::default()
    })
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

/// Check if a text part is a continuation of an ordered list (numbered item > 1).
/// Returns true for "2. foo", "3. bar", "10. baz" but not "1. start".
fn is_ordered_list_continuation(s: &str) -> bool {
    let trimmed = s.trim_start();
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || !bytes[0].is_ascii_digit() {
        return false;
    }
    // Find the ". " pattern after digits
    if let Some(dot_pos) = trimmed.find(". ") {
        // All chars before dot must be digits, and number must be > 1
        let prefix = &trimmed[..dot_pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = prefix.parse::<u32>() {
                return n > 1;
            }
        }
    }
    false
}

/// Check if a block ends with an ordered list item.
fn ends_with_ordered_list_item(s: &str) -> bool {
    // Check the last line
    if let Some(last_line) = s.lines().last() {
        let trimmed = last_line.trim_start();
        let bytes = trimmed.as_bytes();
        if !bytes.is_empty() && bytes[0].is_ascii_digit() {
            if let Some(dot_pos) = trimmed.find(". ") {
                let prefix = &trimmed[..dot_pos];
                return prefix.chars().all(|c| c.is_ascii_digit());
            }
        }
    }
    false
}

/// Check if a text block has an unclosed code fence.
///
/// Scans lines for fence markers (``` or ~~~). An odd count means the last fence
/// was an opening marker with no matching close — the block is mid-code-block.
fn has_unclosed_fence(s: &str) -> bool {
    let mut inside_fence = false;
    for line in s.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            inside_fence = !inside_fence;
        }
    }
    inside_fence
}

/// Split an incoming text delta into paragraph blocks, merging with existing blocks.
///
/// Handles:
/// Index where the trailing run of `ContentBlock::Text` blocks starts.
/// Returns `blocks.len()` if no trailing text (i.e., last block is non-text or empty).
fn trailing_text_start(blocks: &[ContentBlock]) -> usize {
    blocks
        .iter()
        .rposition(|b| !matches!(b, ContentBlock::Text(_)))
        .map_or(0, |i| i + 1)
}

/// - `\n\n` paragraph splitting
/// - Ordered list continuation merging (keeps "1. ...\n\n2. ..." in one block)
/// - Full-text resend detection (some LLM backends re-emit the full response)
/// - Empty placeholder management (trailing `\n\n` pushes a placeholder for next delta)
fn split_and_merge_text_delta(existing: &[String], delta: &str) -> Vec<String> {
    let mut blocks: Vec<String> = existing.to_vec();
    let parts: Vec<&str> = delta.split("\n\n").collect();

    if blocks.is_empty() {
        // First content — build blocks from parts
        if let Some((first, rest)) = parts.split_first() {
            if !first.is_empty() {
                blocks.push(first.to_string());
            }
            push_parts_merging_lists(&mut blocks, rest);
            // Trailing \n\n means next delta starts fresh — but not inside a code fence
            if delta.ends_with("\n\n")
                && !blocks.is_empty()
                && !blocks.last().map_or(true, |b| has_unclosed_fence(b) || b.is_empty())
            {
                blocks.push(String::new());
            }
        }
    } else if parts.len() == 1 {
        // No separator — append to current block
        if try_merge_list_across_placeholder(&mut blocks, delta) {
            // merged
        } else if let Some(last) = blocks.last_mut() {
            if last.is_empty() {
                *last = delta.to_string();
            } else if is_full_text_resend(last, delta) {
                tracing::debug!(
                    incoming_len = delta.len(),
                    existing_len = last.len(),
                    "Skipping duplicate full-text delta"
                );
            } else {
                last.push_str(delta);
            }
        }
    } else {
        // Has separator(s) — append first part to current, create new blocks for rest
        if let Some((first, rest)) = parts.split_first() {
            append_first_part(&mut blocks, first);
            push_parts_merging_lists(&mut blocks, rest);
            // Trailing \n\n means next delta starts fresh — but not inside a code fence
            if delta.ends_with("\n\n")
                && !blocks.is_empty()
                && !blocks.last().map_or(true, |b| has_unclosed_fence(b) || b.is_empty())
            {
                blocks.push(String::new());
            }
        }
    }

    blocks
}

/// Append parts to blocks, merging ordered list continuations and code fence
/// interiors with the previous block.
fn push_parts_merging_lists(blocks: &mut Vec<String>, parts: &[&str]) {
    for part in parts {
        if part.is_empty() {
            continue;
        }
        let should_merge = blocks.last().is_some_and(|prev| {
            // Merge ordered list continuations
            (is_ordered_list_continuation(part) && ends_with_ordered_list_item(prev))
            // Merge content that's inside an unclosed code fence
            || has_unclosed_fence(prev)
        });
        if should_merge {
            if let Some(last) = blocks.last_mut() {
                last.push_str("\n\n");
                last.push_str(part);
            }
        } else {
            blocks.push(part.to_string());
        }
    }
}

/// Try to merge across an empty placeholder for list continuations and unclosed fences.
/// Returns true if merged.
fn try_merge_list_across_placeholder(blocks: &mut Vec<String>, delta: &str) -> bool {
    if blocks.len() >= 2 && blocks.last().map(|b| b.is_empty()).unwrap_or(false) {
        let prev = &blocks[blocks.len() - 2];
        let should_merge = (is_ordered_list_continuation(delta)
            && ends_with_ordered_list_item(prev))
            || has_unclosed_fence(prev);
        if should_merge {
            blocks.pop();
            if let Some(prev) = blocks.last_mut() {
                prev.push_str("\n\n");
                prev.push_str(delta);
            }
            return true;
        }
    }
    false
}

/// Append the first part of a multi-part delta to the current blocks.
fn append_first_part(blocks: &mut Vec<String>, first: &str) {
    // Check if last block is an empty placeholder and first part should merge
    // (list continuation or unclosed code fence in the block before the placeholder)
    if blocks.len() >= 2
        && blocks.last().map(|b| b.is_empty()).unwrap_or(false)
        && !first.is_empty()
    {
        let prev = &blocks[blocks.len() - 2];
        let should_merge = (is_ordered_list_continuation(first)
            && ends_with_ordered_list_item(prev))
            || has_unclosed_fence(prev);
        if should_merge {
            blocks.pop();
            if let Some(prev) = blocks.last_mut() {
                prev.push_str("\n\n");
                prev.push_str(first);
            }
            return;
        }
    }

    if let Some(last) = blocks.last_mut() {
        // Inside an unclosed fence or continuing an ordered list: rejoin with \n\n
        if has_unclosed_fence(last)
            || (!first.is_empty()
                && is_ordered_list_continuation(first)
                && ends_with_ordered_list_item(last))
        {
            last.push_str("\n\n");
            last.push_str(first);
        } else {
            last.push_str(first);
        }
    }
}

/// Detect full-text re-sends from LLM backends.
fn is_full_text_resend(existing: &str, incoming: &str) -> bool {
    let incoming = incoming.trim_start_matches('\n');
    let existing = existing.trim_start_matches('\n');
    !incoming.is_empty() && incoming == existing
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
                blocks,
                ..
            }) if blocks.is_empty()
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
            blocks: Vec::new(),
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
            blocks: Vec::new(),
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
    pub fn append_text(&mut self, text: &str) {
        let response_idx = self.ensure_open_response();

        if let ChatContainer::AssistantResponse { blocks, .. } = &mut self.containers[response_idx]
        {
            let trailing_text_start = trailing_text_start(blocks);
            let existing: Vec<String> = blocks[trailing_text_start..]
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text(content) => Some(content.clone()),
                    ContentBlock::Thinking(_) => None,
                })
                .collect();

            let merged = split_and_merge_text_delta(&existing, text);

            blocks.truncate(trailing_text_start);
            blocks.extend(merged.into_iter().map(ContentBlock::Text));
        }
    }

    /// Set thinking content for the current assistant response.
    /// Position-aware: finds existing thinking block anywhere in blocks, or inserts
    /// before trailing text to maintain thinking-before-text invariant.
    pub fn set_thinking(&mut self, content: String, token_count: usize) {
        let idx = self.ensure_open_response();
        if let Some(ChatContainer::AssistantResponse { blocks, .. }) = self.containers.get_mut(idx)
        {
            let thinking = ThinkingBlock {
                content,
                token_count,
            };
            // Find any existing thinking block and replace it
            if let Some(pos) = blocks
                .iter()
                .position(|b| matches!(b, ContentBlock::Thinking(_)))
            {
                blocks[pos] = ContentBlock::Thinking(thinking);
            } else {
                // No existing thinking — insert before trailing text run
                let insert_at = trailing_text_start(blocks);
                blocks.insert(insert_at, ContentBlock::Thinking(thinking));
            }
        }
    }

    /// Append thinking content to the current assistant response.
    /// Creates a new response if none exists.
    /// Position-aware: coalesces with existing thinking or inserts before trailing
    /// text blocks to maintain thinking-before-text ordering regardless of event
    /// arrival order.
    pub fn append_thinking(&mut self, delta: &str) {
        let response_idx = self.ensure_open_response();

        if let ChatContainer::AssistantResponse { blocks, .. } = &mut self.containers[response_idx]
        {
            // Try to coalesce with the last thinking block (handles consecutive deltas)
            if let Some(ContentBlock::Thinking(tb)) = blocks.last_mut() {
                tb.content.push_str(delta);
                tb.token_count += 1;
                return;
            }

            // Find insertion point: before trailing text run
            let insert_at = trailing_text_start(blocks);

            // If there's a thinking block just before the trailing text, coalesce with it
            if insert_at > 0 {
                if let ContentBlock::Thinking(tb) = &mut blocks[insert_at - 1] {
                    tb.content.push_str(delta);
                    tb.token_count += 1;
                    return;
                }
            }

            // No existing thinking to coalesce with — insert new block
            blocks.insert(
                insert_at,
                ContentBlock::Thinking(ThinkingBlock {
                    content: delta.to_string(),
                    token_count: 1,
                }),
            );
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
    fn test_text_block_splitting() {
        let mut list = ContainerList::new();

        list.start_assistant_response();
        list.append_text("Block 1\n\nBlock 2\n\nBlock 3");

        if let Some(ChatContainer::AssistantResponse { blocks, .. }) = list.containers.last() {
            assert_eq!(blocks.len(), 3);
            assert!(matches!(blocks[0], ContentBlock::Text(ref s) if s == "Block 1"));
            assert!(matches!(blocks[1], ContentBlock::Text(ref s) if s == "Block 2"));
            assert!(matches!(blocks[2], ContentBlock::Text(ref s) if s == "Block 3"));
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

        if let Some(ChatContainer::AssistantResponse { blocks, .. }) = list.containers.last() {
            assert_eq!(blocks.len(), 2);
            assert!(matches!(blocks[0], ContentBlock::Text(ref s) if s == "First part"));
            assert!(matches!(blocks[1], ContentBlock::Text(ref s) if s == "Second part"));
        } else {
            panic!("Expected AssistantResponse");
        }
    }

    #[test]
    fn content_block_thinking_coalesces() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_thinking("plan ");
        list.append_thinking("more");

        let blocks = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks,
            _ => panic!("Expected AssistantResponse"),
        };

        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Thinking(tb) => {
                assert_eq!(tb.content, "plan more");
                assert_eq!(tb.token_count, 2);
            }
            _ => panic!("Expected thinking block"),
        }
    }

    /// Thinking inserted before trailing text blocks preserves correct visual order.
    /// When text arrives before thinking (daemon misordering), append_thinking
    /// inserts before the trailing text run so thinking renders first.
    /// After the fix, "first" and "second" share the same trailing text run and
    /// merge (no \n\n separator), so we get 2 blocks, not 3.
    #[test]
    fn content_block_interleaving_order() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("first");
        list.append_thinking("thought");
        list.append_text("\n\nsecond");

        let blocks = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks,
            _ => panic!("Expected AssistantResponse"),
        };

        // Thinking is repositioned before trailing text, not appended after
        assert_eq!(blocks.len(), 3, "blocks: {blocks:?}");
        assert!(matches!(
            blocks[0],
            ContentBlock::Thinking(ThinkingBlock {
                content: ref s,
                token_count: 1
            }) if s == "thought"
        ));
        assert!(matches!(blocks[1], ContentBlock::Text(ref s) if s == "first"));
        assert!(matches!(blocks[2], ContentBlock::Text(ref s) if s == "second"));
    }

    /// Reproduce daemon bug: text_delta arrives before thinking for the same chunk.
    /// append_thinking must insert before trailing text so thinking renders first.
    #[test]
    fn append_thinking_before_trailing_text() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("Here is my answer");
        list.append_thinking("let me reason about this");

        let blocks = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks,
            _ => panic!("Expected AssistantResponse"),
        };

        assert_eq!(blocks.len(), 2);
        assert!(
            matches!(blocks[0], ContentBlock::Thinking(_)),
            "Thinking must come before text, got: {blocks:?}"
        );
        assert!(
            matches!(blocks[1], ContentBlock::Text(ref s) if s == "Here is my answer"),
            "Text should follow thinking, got: {blocks:?}"
        );
    }

    /// set_thinking should also find/replace thinking blocks that aren't last
    /// (e.g., when text blocks have been appended after thinking).
    #[test]
    fn set_thinking_replaces_existing_non_last() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_thinking("initial");
        list.append_text("some text");
        // Thinking is at blocks[0], text at blocks[1]. set_thinking should replace blocks[0].
        list.set_thinking("replaced".to_string(), 50);

        let blocks = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks,
            _ => panic!("Expected AssistantResponse"),
        };

        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Thinking(tb) => {
                assert_eq!(tb.content, "replaced");
                assert_eq!(tb.token_count, 50);
            }
            _ => panic!("Expected thinking at index 0, got: {blocks:?}"),
        }
        assert!(matches!(blocks[1], ContentBlock::Text(ref s) if s == "some text"));
    }

    /// Legitimate thinking→text→thinking coalesces thinking blocks.
    #[test]
    fn append_thinking_coalesces_at_boundary() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_thinking("phase one ");
        list.append_text("response");
        list.append_thinking("phase two");

        let blocks = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks,
            _ => panic!("Expected AssistantResponse"),
        };

        // Both thinking phases coalesce before the text
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Thinking(tb) => {
                assert_eq!(tb.content, "phase one phase two");
            }
            _ => panic!("Expected thinking at index 0, got: {blocks:?}"),
        }
        assert!(matches!(blocks[1], ContentBlock::Text(ref s) if s == "response"));
    }

    #[test]
    fn content_block_text_coalesces() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("Hello");
        list.append_text(" world");

        let blocks = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks,
            _ => panic!("Expected AssistantResponse"),
        };

        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], ContentBlock::Text(ref s) if s == "Hello world"));
    }

    /// Reproduces exact append_text sequence captured from cursor-acp duplication bug.
    /// Log showed 3 calls:
    ///   1. "\nHello — I'm " (17 bytes, streaming delta)
    ///   2. "here to help with the Crucible codebase or anything else you're working on." (77 bytes, streaming continuation)
    ///   3. "\nHello — I'm here to help with the Crucible codebase or anything else you're working on." (94 bytes, FULL TEXT re-sent)
    ///
    /// The third call duplicated the response text in the viewport.
    #[test]
    fn repro_cursor_acp_text_duplication() {
        let mut list = ContainerList::new();
        list.add_user_message("Say hello in one sentence".to_string());
        list.start_assistant_response();

        // Exact deltas from the reproduction log
        list.append_text("\nHello \u{2014} I'm ");
        list.append_text(
            "here to help with the Crucible codebase or anything else you're working on.",
        );
        list.append_text("\nHello \u{2014} I'm here to help with the Crucible codebase or anything else you're working on.");

        let blocks = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks,
            _ => panic!("Expected AssistantResponse"),
        };

        // The full concatenated text across all blocks
        let full_text: String = blocks
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text(content) => Some(content.as_str()),
                ContentBlock::Thinking(_) => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        // The greeting text must appear exactly ONCE
        let greeting = "Hello \u{2014} I'm here to help with the Crucible codebase or anything else you're working on.";
        let count = full_text.matches(greeting).count();
        assert_eq!(
            count,
            1,
            "Expected greeting to appear exactly once, but appeared {} times.\nBlocks ({}):\n{:#?}",
            count,
            blocks.len(),
            blocks
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
                    ChatContainer::AssistantResponse { id, blocks, .. } => {
                        eprintln!("{}: Asst({}): {:?}", i, id, blocks);
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
                ChatContainer::AssistantResponse { id, blocks, .. } => {
                    eprintln!("{}: Asst({}): {:?}", i, id, blocks);
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
    fn test_ordered_list_not_split_across_blocks() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("1. First item\n\n2. Second item\n\n3. Third item");

        if let Some(ChatContainer::AssistantResponse { blocks, .. }) = list.containers.last() {
            // All list items should be in one block
            assert_eq!(
                blocks.len(),
                1,
                "List items should not be split: {:?}",
                blocks
            );
            match &blocks[0] {
                ContentBlock::Text(content) => {
                    assert!(content.contains("1. First item"));
                    assert!(content.contains("2. Second item"));
                    assert!(content.contains("3. Third item"));
                }
                _ => panic!("Expected text block"),
            }
        } else {
            panic!("Expected AssistantResponse");
        }
    }

    #[test]
    fn test_ordered_list_incremental_streaming() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("1. First item\n\n");
        list.append_text("2. Second item\n\n");
        list.append_text("3. Third item");

        if let Some(ChatContainer::AssistantResponse { blocks, .. }) = list.containers.last() {
            // All list items should be merged into one block
            assert_eq!(
                blocks.len(),
                1,
                "Streamed list items should merge: {:?}",
                blocks
            );
            match &blocks[0] {
                ContentBlock::Text(content) => {
                    assert!(content.contains("1. First item"));
                    assert!(content.contains("2. Second item"));
                    assert!(content.contains("3. Third item"));
                }
                _ => panic!("Expected text block"),
            }
        } else {
            panic!("Expected AssistantResponse");
        }
    }

    #[test]
    fn test_ordered_list_followed_by_paragraph() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("1. First item\n\n2. Second item\n\nSome paragraph after the list");

        if let Some(ChatContainer::AssistantResponse { blocks, .. }) = list.containers.last() {
            // List items in one block, paragraph in another
            assert_eq!(
                blocks.len(),
                2,
                "Paragraph should be separate: {:?}",
                blocks
            );
            assert!(matches!(
                blocks[0],
                ContentBlock::Text(ref s) if s.contains("1. First item") && s.contains("2. Second item")
            ));
            assert!(matches!(
                blocks[1],
                ContentBlock::Text(ref s) if s == "Some paragraph after the list"
            ));
        } else {
            panic!("Expected AssistantResponse");
        }
    }

    #[test]
    fn test_is_ordered_list_continuation() {
        assert!(super::is_ordered_list_continuation("2. Second"));
        assert!(super::is_ordered_list_continuation("3. Third"));
        assert!(super::is_ordered_list_continuation("10. Tenth"));
        assert!(!super::is_ordered_list_continuation("1. First")); // starts new list
        assert!(!super::is_ordered_list_continuation("Not a list"));
        assert!(!super::is_ordered_list_continuation(""));
    }

    #[test]
    fn test_ends_with_ordered_list_item() {
        assert!(super::ends_with_ordered_list_item("1. First item"));
        assert!(super::ends_with_ordered_list_item(
            "Some text\n2. Second item"
        ));
        assert!(!super::ends_with_ordered_list_item("Just text"));
        assert!(!super::ends_with_ordered_list_item(""));
    }

    #[test]
    fn thinking_renders_at_arrival_position() {
        use crucible_oil::render::render_to_plain_text;

        let blocks = vec![
            ContentBlock::Text("First paragraph".to_string()),
            ContentBlock::Thinking(ThinkingBlock {
                content: "my deep thought".to_string(),
                token_count: 5,
            }),
            ContentBlock::Text("Second paragraph".to_string()),
        ];

        let params = super::RenderBlocksParams {
            container_id: "test-1",
            blocks: &blocks,
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

        let first_pos = output.find("First paragraph").expect("first text missing");
        let think_pos = output.find("my deep thought").expect("thinking missing");
        let second_pos = output
            .find("Second paragraph")
            .expect("second text missing");

        assert!(
            first_pos < think_pos,
            "thinking should appear after first text"
        );
        assert!(
            think_pos < second_pos,
            "thinking should appear before second text"
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
        let blocks = vec![
            ContentBlock::Thinking(ThinkingBlock {
                content: long_thinking,
                token_count: 50,
            }),
            ContentBlock::Text("Response text".to_string()),
        ];

        let params = super::RenderBlocksParams {
            container_id: "test-2",
            blocks: &blocks,
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

        // show_thinking=false shows a one-line summary, not a bounded tail
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

    /// Bug: when streaming text after a thinking block, the spinner stays on the
    /// "Thinking… (N words)" summary even after text output has started.
    /// The thinking summary spinner should stop once text content begins streaming.
    #[test]
    fn thinking_summary_spinner_stops_when_text_starts_streaming() {
        use crucible_oil::render::render_to_plain_text;

        let spinner_chars: Vec<char> = vec!['◐', '◓', '◑', '◒'];

        // Scenario: thinking complete, text streaming (not complete)
        let blocks = vec![
            ContentBlock::Thinking(ThinkingBlock {
                content: "let me reason about this carefully".to_string(),
                token_count: 10,
            }),
            ContentBlock::Text("Here is my response so far".to_string()),
        ];

        let params = super::RenderBlocksParams {
            container_id: "test-spinner",
            blocks: &blocks,
            complete: false, // still streaming
            is_continuation: false,
        };
        let render_state = super::RenderState {
            terminal_width: 80,
            spinner_frame: 0,
            show_thinking: false, // collapsed thinking
        };

        let node = super::render_assistant_blocks_with_graduation(&params, &render_state);
        let output = render_to_plain_text(&node, 80);

        // The thinking summary line should NOT have a spinner — text has started
        let thinking_line = output
            .lines()
            .find(|l| l.contains("Thinking") || l.contains("Thought"))
            .expect("should have thinking summary line");

        assert!(
            !spinner_chars.iter().any(|c| thinking_line.contains(*c)),
            "Thinking summary should not show spinner once text is streaming.\nLine: {thinking_line}\nFull output:\n{output}"
        );

        // There SHOULD still be a trailing spinner (for the streaming text)
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

    /// The completed thinking summary should use a thinking-specific icon,
    /// not ┌─ (box drawing corner) which implies content below it.
    #[test]
    fn completed_thinking_summary_uses_thinking_icon_not_box_corner() {
        use crucible_oil::render::render_to_plain_text;

        let blocks = vec![
            ContentBlock::Thinking(ThinkingBlock {
                content: "deep thoughts about architecture".to_string(),
                token_count: 8,
            }),
            ContentBlock::Text("Here is my answer.".to_string()),
        ];

        let params = super::RenderBlocksParams {
            container_id: "test-icon",
            blocks: &blocks,
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

        // Should NOT use box-drawing corner ┌ when thinking is collapsed
        assert!(
            !thought_line.contains('\u{250C}'),
            "Collapsed thinking summary should not use box-drawing corner ┌\nLine: {thought_line}"
        );
    }

    #[test]
    fn code_block_not_split_across_text_blocks() {
        // A fenced code block separated from surrounding text by \n\n should
        // remain as a single block (fences + content together), not get split
        // at the \n\n boundary which tears the fences off the content.
        let md = "## Quick Commands\n\n```bash\n# Chat\ncru chat\n```\n\nText between\n\n```bash\ncru chat -a claude\n```\n\nMore text\n\n```bash\ncru mcp\n```";
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text(md);

        let texts = extract_text_blocks(&list);
        assert_no_orphaned_fences(&texts, "Single-delta code blocks");
    }

    #[test]
    fn streamed_code_block_not_split_across_text_blocks() {
        // Simulates streaming where fences arrive as separate tokens from content
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("Here is code:\n\n");
        list.append_text("```bash\n");
        list.append_text("cru chat -a claude\n");
        list.append_text("```\n\n");
        list.append_text("And more code:\n\n");
        list.append_text("```bash\n");
        list.append_text("cru mcp\n");
        list.append_text("```");

        let blocks = match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks,
            _ => panic!("Expected AssistantResponse"),
        };

        let texts: Vec<&str> = blocks
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        for (i, text) in texts.iter().enumerate() {
            let trimmed = text.trim();
            assert!(
                trimmed != "```" && trimmed != "```bash",
                "Block {i} is a bare fence marker '{}', streaming tore code block apart.\nAll blocks: {texts:?}",
                trimmed
            );
        }
    }

    #[test]
    fn code_block_fences_survive_chunked_streaming() {
        // Simulates various realistic streaming chunk patterns to find
        // which pattern tears code blocks apart.
        //
        // Full content:
        // ## Quick Commands\n\n```bash\n# Chat\ncru chat\n```\n\n
        // Chat with Claude Code\n\n```bash\ncru chat -a claude\n```\n\n
        // Start MCP server\n\n```bash\ncru mcp\n```

        // Pattern A: closing fence and \n\n arrive together (```\n\n)
        {
            let mut list = ContainerList::new();
            list.start_assistant_response();
            list.append_text("## Quick Commands\n\n```bash\n# Chat\ncru chat\n");
            list.append_text("```\n\nChat with Claude Code\n\n```bash\ncru chat -a claude\n");
            list.append_text("```\n\nStart MCP server\n\n```bash\ncru mcp\n```");

            let blocks = extract_text_blocks(&list);
            eprintln!("Pattern A blocks: {blocks:?}");
            assert_no_orphaned_fences(&blocks, "Pattern A");
        }

        // Pattern B: closing fence in one chunk, \n\n starts next chunk
        {
            let mut list = ContainerList::new();
            list.start_assistant_response();
            list.append_text("## Quick Commands\n\n```bash\n# Chat\ncru chat\n```");
            list.append_text("\n\nChat with Claude Code\n\n```bash\ncru chat -a claude\n```");
            list.append_text("\n\nStart MCP server\n\n```bash\ncru mcp\n```");

            let blocks = extract_text_blocks(&list);
            eprintln!("Pattern B blocks: {blocks:?}");
            assert_no_orphaned_fences(&blocks, "Pattern B");
        }

        // Pattern C: large chunks with mid-fence splits
        {
            let mut list = ContainerList::new();
            list.start_assistant_response();
            list.append_text("## Quick Commands\n\n```bash\n# Chat\ncru chat\n```\n\nChat with Claude Code\n\n```");
            list.append_text("bash\ncru chat -a claude\n```\n\nStart MCP server\n\n```bash\ncru mcp\n```");

            let blocks = extract_text_blocks(&list);
            eprintln!("Pattern C blocks: {blocks:?}");
            assert_no_orphaned_fences(&blocks, "Pattern C");
        }

        // Pattern D: \n\n inside code fence content
        {
            let mut list = ContainerList::new();
            list.start_assistant_response();
            list.append_text("```bash\n# Comment\n\ncru chat\n```\n\nSome text\n\n```bash\ncru mcp\n```");

            let blocks = extract_text_blocks(&list);
            eprintln!("Pattern D blocks: {blocks:?}");
            assert_no_orphaned_fences(&blocks, "Pattern D");
        }

        // Pattern E: bare ``` (no language tag) code blocks
        {
            let mut list = ContainerList::new();
            list.start_assistant_response();
            list.append_text("Text\n\n```\ncru chat\n```\n\nMore text\n\n```\ncru mcp\n```");

            let blocks = extract_text_blocks(&list);
            eprintln!("Pattern E blocks: {blocks:?}");
            assert_no_orphaned_fences(&blocks, "Pattern E");
        }
    }

    fn extract_text_blocks(list: &ContainerList) -> Vec<String> {
        match list.containers.last() {
            Some(ChatContainer::AssistantResponse { blocks, .. }) => blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text(s) => Some(s.clone()),
                    _ => None,
                })
                .collect(),
            _ => panic!("Expected AssistantResponse"),
        }
    }

    #[test]
    fn streamed_code_block_with_blank_line_inside() {
        // Code block with a blank line inside, streamed in chunks where
        // the \n\n arrives at a chunk boundary
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("```bash\n# Comment\n\n");
        list.append_text("cru chat\n```\n\n");
        list.append_text("Some text");

        let blocks = extract_text_blocks(&list);
        eprintln!("Streamed blank-line blocks: {blocks:?}");
        assert_no_orphaned_fences(&blocks, "Streamed blank-line");

        // The code block should be intact
        let code_block = &blocks[0];
        assert!(
            code_block.contains("```bash") && code_block.contains("```\n") || code_block.ends_with("```"),
            "Code block should have both fences: {:?}",
            code_block
        );
    }

    fn assert_no_orphaned_fences(blocks: &[String], label: &str) {
        for (i, text) in blocks.iter().enumerate() {
            let trimmed = text.trim();
            // A block that is JUST a fence marker means the code block was torn apart
            assert!(
                trimmed != "```" && !trimmed.starts_with("```") || trimmed.matches("```").count() >= 2,
                "{label}: Block {i} has orphaned fence marker: {:?}\nAll blocks: {:?}",
                trimmed,
                blocks
            );
        }
    }

}
