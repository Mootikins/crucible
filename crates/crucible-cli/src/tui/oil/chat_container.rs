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
use crate::tui::oil::node::{col, row, scrollback, spinner, text, Node};
use crate::tui::oil::style::Padding;
use crate::tui::oil::viewport_cache::{CachedShellExecution, CachedSubagent, CachedToolCall};

/// Parameters for rendering a container view.
///
/// Bundles layout context and derived state that containers need for rendering.
/// This prepares for plugin extensibility where external code can define
/// `(data, view_fn)` pairs using the `ContainerView` trait.
#[derive(Debug, Clone, Copy)]
pub struct ViewParams {
    pub width: usize,
    pub spinner_frame: usize,
    pub show_thinking: bool,
    /// Whether this container is a continuation after a tool call (no bullet shown).
    pub is_continuation: bool,
    /// Whether this response is complete (derived from turn state + position).
    pub is_complete: bool,
}

/// Trait for rendering a container to a node tree.
///
/// Built-in containers implement this via `ChatContainer`. External plugins
/// can provide their own implementations for custom container types.
pub trait ContainerView {
    /// Unique ID for this container (used for graduation).
    fn id(&self) -> &str;
    /// Render this container to a Node tree.
    fn view(&self, params: &ViewParams) -> Node;
}

impl ContainerView for ChatContainer {
    fn id(&self) -> &str {
        // Delegate to the inherent method
        ChatContainer::id(self)
    }

    fn view(&self, params: &ViewParams) -> Node {
        // Delegate to the inherent method
        ChatContainer::view_with_params(self, params)
    }
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

    /// Assistant response, may contain multiple text blocks and optional thinking
    AssistantResponse {
        id: String,
        /// Text blocks separated by double newlines
        blocks: Vec<String>,
        /// Associated thinking block (shown if enabled)
        thinking: Option<ThinkingBlock>,
    },

    /// Group of consecutive tool calls (rendered compactly)
    ToolGroup {
        id: String,
        tools: Vec<CachedToolCall>,
    },

    /// Subagent execution
    Subagent {
        id: String,
        subagent: CachedSubagent,
    },

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
            Self::Subagent { id, .. } => id,
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
            Self::Subagent { subagent, .. } => {
                use crate::tui::oil::viewport_cache::SubagentStatus;
                matches!(
                    subagent.status,
                    SubagentStatus::Completed | SubagentStatus::Failed
                )
            }
            Self::ShellExecution { .. } => true,
            Self::SystemMessage { .. } => true,
        }
    }

    /// Render this container to a Node tree using individual parameters.
    ///
    /// Prefer using `view_with_params()` or the `ContainerView` trait for new code.
    pub fn view(
        &self,
        width: usize,
        spinner_frame: usize,
        show_thinking: bool,
        is_continuation: bool,
        is_complete: bool,
    ) -> Node {
        self.view_with_params(&ViewParams {
            width,
            spinner_frame,
            show_thinking,
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
                let content_node = render_user_prompt(content, params.width);
                scrollback(id.clone(), [content_node])
            }

            Self::AssistantResponse {
                id,
                blocks,
                thinking,
            } => render_assistant_blocks_with_graduation(
                id,
                blocks,
                thinking.as_ref(),
                params.is_complete,
                params.width,
                params.show_thinking,
                params.is_continuation,
                params.spinner_frame,
            ),

            Self::ToolGroup { id, tools } => {
                let content = render_tool_group(tools, params.spinner_frame);
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

            Self::Subagent { id, subagent } => {
                let content = render_subagent(subagent, params.spinner_frame);
                // Only wrap in scrollback when subagent is complete
                use crate::tui::oil::viewport_cache::SubagentStatus;
                let is_complete = matches!(
                    subagent.status,
                    SubagentStatus::Completed | SubagentStatus::Failed
                );
                if is_complete {
                    scrollback(id.clone(), [content])
                } else {
                    content
                }
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

/// Render assistant text blocks with graduation support.
///
/// Each completed block (all but the last if streaming) gets its own scrollback
/// to enable incremental graduation. The in-progress block stays in the viewport.
///
/// If `is_continuation` is true, no bullet is shown (it's a continuation after a tool call).
fn render_assistant_blocks_with_graduation(
    container_id: &str,
    blocks: &[String],
    thinking: Option<&ThinkingBlock>,
    complete: bool,
    width: usize,
    show_thinking: bool,
    is_continuation: bool,
    spinner_frame: usize,
) -> Node {
    let mut nodes = Vec::new();

    // Render thinking block if present and enabled
    if show_thinking {
        if let Some(tb) = thinking {
            let thinking_node = render_thinking_block(&tb.content, tb.token_count, width);
            let thinking_with_margin = thinking_node.with_margin(Padding {
                top: 1,
                ..Default::default()
            });
            // Thinking gets its own scrollback key
            nodes.push(scrollback(
                format!("{}-thinking", container_id),
                [thinking_with_margin],
            ));
        }
    }

    // Determine how many blocks are "complete" (can graduate)
    // If streaming (!complete), all but the last block are complete
    // If complete, all blocks are complete
    let complete_block_count = if complete || blocks.is_empty() {
        blocks.len()
    } else {
        blocks.len().saturating_sub(1)
    };

    // Show spinner when streaming and no text yet
    if !complete && blocks.is_empty() {
        let spinner_node = if thinking.is_some() && show_thinking {
            // Thinking is visible, show plain spinner below it
            row([text(" "), spinner(None, spinner_frame)])
        } else {
            // No content at all yet — show spinner as the only indicator
            row([text(" "), spinner(None, spinner_frame)])
        };
        nodes.push(spinner_node.with_margin(Padding {
            top: 1,
            ..Default::default()
        }));
    }

    // Render text blocks
    for (i, block) in blocks.iter().enumerate() {
        // Skip empty blocks
        if block.is_empty() {
            continue;
        }

        // Use continuation margins (no bullet) if this is a continuation response
        // or if this is a subsequent block within the response
        let margins = if is_continuation || i > 0 || thinking.is_some() {
            Margins::assistant_continuation()
        } else {
            Margins::assistant()
        };
        let style = RenderStyle::natural_with_margins(width, margins);
        let md_node = markdown_to_node_styled(block, style);

        // First block gets top margin, all get bottom margin
        let padding = if i == 0 {
            Padding::xy(0, 1)
        } else {
            Padding {
                bottom: 1,
                ..Default::default()
            }
        };
        let block_node = md_node.with_margin(padding);

        // Wrap completed blocks in scrollback for graduation
        if i < complete_block_count {
            nodes.push(scrollback(
                format!("{}-block-{}", container_id, i),
                [block_node],
            ));
        } else {
            // In-progress block - no scrollback, stays in viewport
            nodes.push(block_node);
        }
    }

    // Show spinner after text blocks while still streaming
    if !complete && !blocks.is_empty() {
        nodes.push(row([text(" "), spinner(None, spinner_frame)]));
    }

    col(nodes)
}

/// Render a group of tool calls compactly.
fn render_tool_group(tools: &[CachedToolCall], spinner_frame: usize) -> Node {
    let tool_nodes: Vec<Node> = tools
        .iter()
        .map(|t| render_tool_call_with_frame(t, spinner_frame))
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
    use crate::tui::oil::theme::styles;

    styled(format!(" * {} ", content), styles::system_message()).with_margin(Padding {
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

/// Manages the list of chat containers.
///
/// Containers are append-only during a conversation. When content graduates
/// to stdout, containers are marked but not immediately removed (for efficiency).
pub struct ContainerList {
    containers: Vec<ChatContainer>,
    /// Index of first non-graduated container
    viewport_start: usize,
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
            viewport_start: 0,
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

    /// Remove the last container if it's an empty AssistantResponse (no text, no thinking).
    /// This avoids "gap" containers that block graduation.
    fn remove_empty_trailing_response(&mut self) {
        let should_remove = matches!(
            self.containers.last(),
            Some(ChatContainer::AssistantResponse {
                blocks,
                thinking: None,
                ..
            }) if blocks.is_empty()
        );
        if should_remove {
            self.containers.pop();
        }
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
        let id = self.next_id("assistant");
        self.containers.push(ChatContainer::AssistantResponse {
            id,
            blocks: Vec::new(),
            thinking: None,
        });
        self.turn_active = true;
        self.containers.len() - 1
    }

    /// Start a new assistant response (called when streaming begins).
    pub fn start_assistant_response(&mut self) -> &str {
        let id = self.next_id("assistant");
        self.containers.push(ChatContainer::AssistantResponse {
            id,
            blocks: Vec::new(),
            thinking: None,
        });
        self.turn_active = true;
        self.containers.last().unwrap().id()
    }

    /// Append text to the current assistant response.
    /// Creates a new response if none exists (via backward scan).
    pub fn append_text(&mut self, text: &str) {
        let response_idx = self.ensure_open_response();

        if let ChatContainer::AssistantResponse { blocks, .. } = &mut self.containers[response_idx]
        {
            // Check if text contains block separators
            let parts: Vec<&str> = text.split("\n\n").collect();

            if blocks.is_empty() {
                // First content - add first part as new block
                if let Some((first, rest)) = parts.split_first() {
                    if !first.is_empty() {
                        blocks.push(first.to_string());
                    }
                    // Add remaining parts, merging ordered list continuations
                    for part in rest {
                        if !part.is_empty() {
                            if is_ordered_list_continuation(part)
                                && blocks
                                    .last()
                                    .map(|b| ends_with_ordered_list_item(b))
                                    .unwrap_or(false)
                            {
                                // Merge with previous block to preserve list numbering
                                if let Some(last) = blocks.last_mut() {
                                    last.push_str("\n\n");
                                    last.push_str(part);
                                }
                            } else {
                                blocks.push(part.to_string());
                            }
                        }
                    }
                }
            } else if parts.len() == 1 {
                // No separator in this text
                // Check if we should merge a list continuation into the previous block
                // (the last block is an empty placeholder from a prior \n\n split)
                let should_merge_list = blocks.len() >= 2
                    && blocks.last().map(|b| b.is_empty()).unwrap_or(false)
                    && is_ordered_list_continuation(text)
                    && ends_with_ordered_list_item(&blocks[blocks.len() - 2]);

                if should_merge_list {
                    // Remove empty placeholder, merge into previous block
                    blocks.pop();
                    if let Some(prev) = blocks.last_mut() {
                        prev.push_str("\n\n");
                        prev.push_str(text);
                    }
                } else if let Some(last) = blocks.last_mut() {
                    if last.is_empty() {
                        // Replace placeholder with content
                        *last = text.to_string();
                    } else {
                        // Append to existing content
                        last.push_str(text);
                    }
                }
            } else {
                // Has separator(s) - append first part to current, create new blocks for rest
                if let Some((first, rest)) = parts.split_first() {
                    if let Some(last) = blocks.last_mut() {
                        // If the first part is a list continuation and the current block
                        // ends with a list item, preserve the \n\n separator
                        if !first.is_empty()
                            && is_ordered_list_continuation(first)
                            && ends_with_ordered_list_item(last)
                        {
                            last.push_str("\n\n");
                        }
                        last.push_str(first);
                    }
                    // Push parts, merging ordered list continuations
                    for part in rest {
                        if !part.is_empty()
                            && is_ordered_list_continuation(part)
                            && blocks
                                .last()
                                .map(|b| ends_with_ordered_list_item(b))
                                .unwrap_or(false)
                        {
                            if let Some(last) = blocks.last_mut() {
                                last.push_str("\n\n");
                                last.push_str(part);
                            }
                        } else {
                            blocks.push(part.to_string());
                        }
                    }
                }
            }
        }
    }

    /// Set thinking content for the current assistant response.
    pub fn set_thinking(&mut self, content: String, token_count: usize) {
        let idx = self.ensure_open_response();
        if let Some(ChatContainer::AssistantResponse { thinking, .. }) =
            self.containers.get_mut(idx)
        {
            *thinking = Some(ThinkingBlock {
                content,
                token_count,
            });
        }
    }

    /// Append thinking content to the current assistant response.
    /// Creates a new response if none exists.
    pub fn append_thinking(&mut self, delta: &str) {
        let response_idx = self.ensure_open_response();

        if let ChatContainer::AssistantResponse { thinking, .. } =
            &mut self.containers[response_idx]
        {
            match thinking {
                Some(tb) => {
                    tb.content.push_str(delta);
                    tb.token_count += 1;
                }
                None => {
                    *thinking = Some(ThinkingBlock {
                        content: delta.to_string(),
                        token_count: 1,
                    });
                }
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

        // Check if we can add to existing tool group in the viewport
        // Only append if:
        // 1. The last container is a ToolGroup
        // 2. It's still in the viewport (hasn't graduated)
        // This keeps consecutive tools together during streaming while preventing
        // tools from being added to already-graduated groups.
        let can_append = if !self.containers.is_empty() {
            let last_idx = self.containers.len() - 1;
            let in_viewport = last_idx >= self.viewport_start;
            let is_tool_group = matches!(self.containers.last(), Some(ChatContainer::ToolGroup { .. }));
            in_viewport && is_tool_group
        } else {
            false
        };

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
                    if let Some(tool) = tools
                        .iter_mut()
                        .find(|t| t.call_id.as_deref() == Some(cid))
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

    /// Add a subagent.
    /// Like tools, subagents break the current response.
    pub fn add_subagent(&mut self, subagent: CachedSubagent) {
        // Remove empty trailing response to avoid graduation gaps.
        self.remove_empty_trailing_response();

        let id = self.next_id("subagent");
        self.containers
            .push(ChatContainer::Subagent { id, subagent });
    }

    /// Update a subagent by ID.
    pub fn update_subagent(&mut self, subagent_id: &str, f: impl FnOnce(&mut CachedSubagent)) {
        for container in self.containers.iter_mut().rev() {
            if let ChatContainer::Subagent { subagent, .. } = container {
                if subagent.id.as_ref() == subagent_id {
                    f(subagent);
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

    /// Mark containers as graduated.
    pub fn graduate(&mut self, graduated_ids: &[String]) {
        // Move viewport_start past graduated containers
        while self.viewport_start < self.containers.len() {
            let id = self.containers[self.viewport_start].id();
            if graduated_ids.contains(&id.to_string()) {
                self.viewport_start += 1;
            } else {
                break;
            }
        }

        // Compact if we've accumulated too many graduated containers
        if self.viewport_start > 100 {
            self.containers.drain(0..self.viewport_start);
            self.viewport_start = 0;
        }
    }

    /// Get non-graduated containers for viewport rendering.
    pub fn viewport_containers(&self) -> &[ChatContainer] {
        &self.containers[self.viewport_start..]
    }

    /// Index of the first non-graduated container.
    pub fn viewport_start_index(&self) -> usize {
        self.viewport_start
    }

    /// Get all containers (including graduated, for debugging).
    pub fn all_containers(&self) -> &[ChatContainer] {
        &self.containers
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

    /// Derive whether a container at the given index is a continuation response.
    ///
    /// An AssistantResponse is a continuation if the container before it is a
    /// ToolGroup, Subagent, or ShellExecution — meaning the assistant is
    /// continuing after a tool call rather than starting a fresh response.
    pub fn is_continuation(&self, index: usize) -> bool {
        if index == 0 {
            return false;
        }
        // Only relevant for AssistantResponse containers
        if !matches!(
            &self.containers[index],
            ChatContainer::AssistantResponse { .. }
        ) {
            return false;
        }
        matches!(
            &self.containers[index - 1],
            ChatContainer::ToolGroup { .. }
                | ChatContainer::Subagent { .. }
                | ChatContainer::ShellExecution { .. }
        )
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
            // Incomplete subagent already shows its own spinner
            Some(ChatContainer::Subagent { subagent, .. }) => {
                use crate::tui::oil::viewport_cache::SubagentStatus;
                matches!(
                    subagent.status,
                    SubagentStatus::Completed | SubagentStatus::Failed
                )
            }
            // Everything else (completed containers, user messages, etc.)
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

    /// Check whether a tool's output should be spilled to a file.
    pub fn tool_should_spill(&self, name: &str) -> bool {
        self.find_tool(name)
            .map(|t| t.should_spill_to_file())
            .unwrap_or(false)
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

    /// Format all containers for export as markdown.
    pub fn format_for_export(&self) -> String {
        use crate::tui::oil::viewport_cache::SubagentStatus;
        use std::fmt::Write;

        let mut output = String::from("# Chat Session Export\n\n");

        for container in &self.containers {
            match container {
                ChatContainer::UserMessage { content, .. } => {
                    let _ = writeln!(output, "## User\n\n{}\n", content);
                }
                ChatContainer::AssistantResponse { blocks, .. } => {
                    let combined = blocks.join("\n\n");
                    if !combined.is_empty() {
                        let _ = writeln!(output, "## Assistant\n\n{}\n", combined);
                    }
                }
                ChatContainer::ToolGroup { tools, .. } => {
                    for tool in tools {
                        let _ = writeln!(output, "### Tool: {}\n", tool.name);
                        if !tool.args.is_empty() {
                            let _ = writeln!(output, "```json\n{}\n```\n", tool.args);
                        }
                        let result_str = tool.result();
                        if !result_str.is_empty() {
                            let _ =
                                writeln!(output, "**Result:**\n```\n{}\n```\n", result_str);
                        }
                    }
                }
                ChatContainer::Subagent { subagent, .. } => {
                    let status = match subagent.status {
                        SubagentStatus::Running => "running",
                        SubagentStatus::Completed => "completed",
                        SubagentStatus::Failed => "failed",
                    };
                    let _ = writeln!(output, "### Subagent: {} ({})\n", subagent.id, status);
                    let prompt_preview = if subagent.prompt.len() > 100 {
                        format!("{}...", &subagent.prompt[..100])
                    } else {
                        subagent.prompt.to_string()
                    };
                    let _ = writeln!(output, "Prompt: {}\n", prompt_preview);
                    if let Some(ref summary) = subagent.summary {
                        let _ = writeln!(output, "Result: {}\n", summary);
                    }
                    if let Some(ref error) = subagent.error {
                        let _ = writeln!(output, "Error: {}\n", error);
                    }
                }
                ChatContainer::ShellExecution { shell, .. } => {
                    let _ = writeln!(
                        output,
                        "### Shell: `{}`\n\nExit code: {}\n",
                        shell.command, shell.exit_code
                    );
                    if !shell.output_tail.is_empty() {
                        output.push_str("```\n");
                        for line in &shell.output_tail {
                            output.push_str(line);
                            output.push('\n');
                        }
                        output.push_str("```\n\n");
                    }
                }
                ChatContainer::SystemMessage { content, .. } => {
                    let _ = writeln!(output, "> {}\n", content.replace('\n', "\n> "));
                }
            }
        }

        output
    }

    /// Clear all containers.
    pub fn clear(&mut self) {
        self.containers.clear();
        self.viewport_start = 0;
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
            assert_eq!(blocks[0], "Block 1");
            assert_eq!(blocks[1], "Block 2");
            assert_eq!(blocks[2], "Block 3");
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
            assert_eq!(blocks[0], "First part");
            assert_eq!(blocks[1], "Second part");
        } else {
            panic!("Expected AssistantResponse");
        }
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
        assert!(
            list.is_response_complete(1),
            "Turn ended => complete"
        );
    }

    #[test]
    fn test_graduation() {
        let mut list = ContainerList::new();

        list.add_user_message("Hello".to_string());
        list.start_assistant_response();
        list.append_text("Response");
        list.complete_response();

        let user_id = list.containers[0].id().to_string();

        // Initially all in viewport
        assert_eq!(list.viewport_containers().len(), 2);

        // Graduate user message
        list.graduate(&[user_id]);

        // Only assistant response in viewport now
        assert_eq!(list.viewport_containers().len(), 1);
        assert!(matches!(
            &list.viewport_containers()[0],
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
        for container in list.viewport_containers() {
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
        };

        list.add_tool_call(tool);

        let containers = list.viewport_containers();
        assert_eq!(containers.len(), 1);
        assert!(matches!(containers[0], ChatContainer::ToolGroup { .. }));

        // Verify rendering
        let node = containers[0].view(80, 0, false, false, true);
        assert!(!matches!(node, Node::Empty));
    }

    #[test]
    fn test_container_ids_used_in_scrollback() {
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

        // Check that scrollback nodes use the container IDs
        // We can check this by rendering and seeing the output contains content
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

        assert_eq!(list.viewport_containers().len(), 3);

        // Graduate first two
        let ids: Vec<String> = list.containers[..2]
            .iter()
            .map(|c| c.id().to_string())
            .collect();
        list.graduate(&ids);

        // Only third should remain in viewport
        assert_eq!(list.viewport_containers().len(), 1);
        assert!(list.viewport_containers()[0].id().contains("user-"));
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
        use crate::tui::oil::render::render_to_string;
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
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "___".to_string(),
            delta: "result".to_string(),
                call_id: None,
        });
        app.on_message(ChatAppMsg::ToolResultComplete { name: "___".to_string(),
                call_id: None });
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
            for (i, c) in app.container_list().all_containers().iter().enumerate() {
                match c {
                    ChatContainer::UserMessage { id, content } => {
                        eprintln!("{}: User({}): {:.30}", i, id, content);
                    }
                    ChatContainer::AssistantResponse {
                        id,
                        blocks,
                        ..
                    } => {
                        eprintln!(
                            "{}: Asst({}): {:?}",
                            i, id, blocks
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
        for (i, c) in app.container_list().all_containers().iter().enumerate() {
            match c {
                ChatContainer::UserMessage { id, content } => {
                    eprintln!("{}: User({}): {:.30}", i, id, content);
                }
                ChatContainer::AssistantResponse {
                    id,
                    blocks,
                    ..
                } => {
                    eprintln!(
                        "{}: Asst({}): {:?}",
                        i, id, blocks
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
        let shell =
            CachedShellExecution::new("s1", "ls -la", 0, vec!["file.rs".to_string()], None);
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
        };
        list.add_tool_call(tool);

        assert!(list.find_tool("read_file").is_some());
        assert!(list.find_tool("write_file").is_none());
        assert!(list.tool_should_spill("read_file") == false);
    }

    #[test]
    fn test_ordered_list_not_split_across_blocks() {
        let mut list = ContainerList::new();
        list.start_assistant_response();
        list.append_text("1. First item\n\n2. Second item\n\n3. Third item");

        if let Some(ChatContainer::AssistantResponse { blocks, .. }) = list.containers.last() {
            // All list items should be in one block
            assert_eq!(blocks.len(), 1, "List items should not be split: {:?}", blocks);
            assert!(blocks[0].contains("1. First item"));
            assert!(blocks[0].contains("2. Second item"));
            assert!(blocks[0].contains("3. Third item"));
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
            assert_eq!(blocks.len(), 1, "Streamed list items should merge: {:?}", blocks);
            assert!(blocks[0].contains("1. First item"));
            assert!(blocks[0].contains("2. Second item"));
            assert!(blocks[0].contains("3. Third item"));
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
            assert_eq!(blocks.len(), 2, "Paragraph should be separate: {:?}", blocks);
            assert!(blocks[0].contains("1. First item"));
            assert!(blocks[0].contains("2. Second item"));
            assert_eq!(blocks[1], "Some paragraph after the list");
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
        assert!(super::ends_with_ordered_list_item("Some text\n2. Second item"));
        assert!(!super::ends_with_ordered_list_item("Just text"));
        assert!(!super::ends_with_ordered_list_item(""));
    }

    #[test]
    fn test_format_for_export() {
        let mut list = ContainerList::new();
        list.add_user_message("Hello".to_string());
        list.start_assistant_response();
        list.append_text("World");
        list.complete_response();
        list.add_system_message("Info".to_string());

        let export = list.format_for_export();
        assert!(export.contains("## User"));
        assert!(export.contains("Hello"));
        assert!(export.contains("## Assistant"));
        assert!(export.contains("World"));
        assert!(export.contains("> Info"));
    }
}
