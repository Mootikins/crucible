//! Semantic containers for chat content.
//!
//! This module provides a layer of abstraction between raw cache items and the node tree.
//! Each container represents a logical unit of content that graduates together:
//! - UserMessage: A single user prompt
//! - AssistantResponse: Text blocks + optional thinking (may span multiple deltas)
//! - ToolGroup: Consecutive tool calls grouped together
//! - SystemMessage: System-level messages

use crate::tui::oil::components::{
    render_subagent, render_thinking_block, render_tool_call_with_frame, render_user_prompt,
};
use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::node::{col, scrollback, Node};
use crate::tui::oil::style::Padding;
use crate::tui::oil::viewport_cache::{CachedSubagent, CachedToolCall};

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
        /// Whether streaming is complete for this response
        complete: bool,
        /// Whether this is a continuation (text after tool call) - no bullet shown
        is_continuation: bool,
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
            Self::SystemMessage { id, .. } => id,
        }
    }

    /// Whether this container is complete and can graduate
    pub fn is_complete(&self) -> bool {
        match self {
            Self::UserMessage { .. } => true,
            Self::AssistantResponse { complete, .. } => *complete,
            Self::ToolGroup { tools, .. } => tools.iter().all(|t| t.complete),
            Self::Subagent { subagent, .. } => {
                use crate::tui::oil::viewport_cache::SubagentStatus;
                matches!(
                    subagent.status,
                    SubagentStatus::Completed | SubagentStatus::Failed
                )
            }
            Self::SystemMessage { .. } => true,
        }
    }

    /// Render this container to a Node tree.
    ///
    /// For most containers, content is wrapped in scrollback with the container's ID.
    /// For AssistantResponse, each completed block gets its own scrollback to enable
    /// incremental graduation during streaming.
    pub fn view(&self, width: usize, spinner_frame: usize, show_thinking: bool) -> Node {
        match self {
            Self::UserMessage { id, content } => {
                let content_node = render_user_prompt(content, width);
                scrollback(id.clone(), [content_node])
            }

            Self::AssistantResponse {
                id,
                blocks,
                thinking,
                complete,
                is_continuation,
            } => {
                render_assistant_blocks_with_graduation(
                    id,
                    blocks,
                    thinking.as_ref(),
                    *complete,
                    width,
                    show_thinking,
                    *is_continuation,
                )
            }

            Self::ToolGroup { id, tools } => {
                let content = render_tool_group(tools, spinner_frame);
                // Only wrap in scrollback (allow graduation) when all tools are complete
                // This prevents tools from graduating with spinners and then not updating
                let all_complete = tools.iter().all(|t| t.complete);
                if all_complete {
                    scrollback(id.clone(), [content])
                } else {
                    content
                }
            }

            Self::Subagent { id, subagent } => {
                let content = render_subagent(subagent, spinner_frame);
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
    /// Index of current open (incomplete) assistant response, if any.
    /// This allows text to continue after tool calls interrupt the flow.
    current_response_idx: Option<usize>,
    /// If true, next response created is a continuation (no bullet)
    next_response_is_continuation: bool,
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
            current_response_idx: None,
            next_response_is_continuation: false,
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
        self.containers.push(ChatContainer::UserMessage { id, content });
    }

    /// Start a new assistant response (called when streaming begins).
    /// Uses and clears the `next_response_is_continuation` flag.
    pub fn start_assistant_response(&mut self) -> &str {
        let is_continuation = self.next_response_is_continuation;
        self.next_response_is_continuation = false;

        let id = self.next_id("assistant");
        let idx = self.containers.len();
        self.containers.push(ChatContainer::AssistantResponse {
            id,
            blocks: Vec::new(),
            thinking: None,
            complete: false,
            is_continuation,
        });
        self.current_response_idx = Some(idx);
        self.containers.last().unwrap().id()
    }

    /// Append text to the current assistant response.
    /// Creates a new response if none exists.
    /// Uses tracked current_response_idx to continue after tool calls.
    pub fn append_text(&mut self, text: &str) {
        // Find existing open response by tracked index, or create new
        let response_idx = match self.current_response_idx {
            Some(idx) if idx < self.containers.len() => {
                // Verify it's still an open response
                if matches!(
                    &self.containers[idx],
                    ChatContainer::AssistantResponse { complete: false, .. }
                ) {
                    idx
                } else {
                    self.start_assistant_response();
                    self.containers.len() - 1
                }
            }
            _ => {
                self.start_assistant_response();
                self.containers.len() - 1
            }
        };

        if let ChatContainer::AssistantResponse { blocks, .. } = &mut self.containers[response_idx] {
            // Check if text contains block separators
            let parts: Vec<&str> = text.split("\n\n").collect();

            if blocks.is_empty() {
                // First content - add first part as new block
                if let Some((first, rest)) = parts.split_first() {
                    if !first.is_empty() {
                        blocks.push(first.to_string());
                    }
                    // Add remaining parts as new blocks
                    for part in rest {
                        if !part.is_empty() {
                            blocks.push(part.to_string());
                        }
                    }
                }
            } else if parts.len() == 1 {
                // No separator in this text
                if let Some(last) = blocks.last_mut() {
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
                        last.push_str(first);
                    }
                    // Push even empty parts as placeholders for next append
                    for part in rest {
                        blocks.push(part.to_string());
                    }
                }
            }
        }
    }

    /// Set thinking content for the current assistant response.
    pub fn set_thinking(&mut self, content: String, token_count: usize) {
        if let Some(idx) = self.current_response_idx {
            if let Some(ChatContainer::AssistantResponse { thinking, .. }) =
                self.containers.get_mut(idx)
            {
                *thinking = Some(ThinkingBlock {
                    content,
                    token_count,
                });
            }
        }
    }

    /// Append thinking content to the current assistant response.
    /// Creates a new response if none exists.
    pub fn append_thinking(&mut self, delta: &str) {
        // Ensure we have a current response
        let response_idx = match self.current_response_idx {
            Some(idx) if idx < self.containers.len() => {
                if matches!(
                    &self.containers[idx],
                    ChatContainer::AssistantResponse { complete: false, .. }
                ) {
                    idx
                } else {
                    self.start_assistant_response();
                    self.containers.len() - 1
                }
            }
            _ => {
                self.start_assistant_response();
                self.containers.len() - 1
            }
        };

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

    /// Mark the current assistant response as complete.
    pub fn complete_response(&mut self) {
        if let Some(idx) = self.current_response_idx.take() {
            if let Some(ChatContainer::AssistantResponse { complete, .. }) =
                self.containers.get_mut(idx)
            {
                *complete = true;
            }
        }
    }

    /// Add a tool call.
    /// Groups with previous tool group if one exists and is incomplete.
    /// Marks any current response as complete, then clears current_response_idx.
    /// Sets next_response_is_continuation so the next text won't have a bullet.
    pub fn add_tool_call(&mut self, tool: CachedToolCall) {
        // Mark current response as complete - no more text will be added to it
        if let Some(idx) = self.current_response_idx.take() {
            if let Some(ChatContainer::AssistantResponse { complete, .. }) =
                self.containers.get_mut(idx)
            {
                *complete = true;
            }
        }
        // Next text response is a continuation (no bullet)
        self.next_response_is_continuation = true;

        // Check if we can add to existing tool group
        let can_append = self
            .containers
            .last()
            .map(|c| matches!(c, ChatContainer::ToolGroup { tools, .. } if tools.iter().all(|t| !t.complete)))
            .unwrap_or(false);

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

    /// Update a tool call by name (finds most recent matching tool).
    pub fn update_tool(&mut self, tool_name: &str, f: impl FnOnce(&mut CachedToolCall)) {
        for container in self.containers.iter_mut().rev() {
            if let ChatContainer::ToolGroup { tools, .. } = container {
                if let Some(tool) = tools.iter_mut().rev().find(|t| t.name.as_ref() == tool_name) {
                    f(tool);
                    return;
                }
            }
        }
    }

    /// Add a subagent.
    /// Like tools, subagents break the current response and mark next as continuation.
    pub fn add_subagent(&mut self, subagent: CachedSubagent) {
        // Mark current response as complete - no more text will be added to it
        if let Some(idx) = self.current_response_idx.take() {
            if let Some(ChatContainer::AssistantResponse { complete, .. }) =
                self.containers.get_mut(idx)
            {
                *complete = true;
            }
        }
        // Next text response is a continuation (no bullet)
        self.next_response_is_continuation = true;

        let id = self.next_id("subagent");
        self.containers.push(ChatContainer::Subagent { id, subagent });
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

    /// Clear all containers.
    pub fn clear(&mut self) {
        self.containers.clear();
        self.viewport_start = 0;
        self.current_response_idx = None;
        self.next_response_is_continuation = false;
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

        // Assistant responses start incomplete
        list.start_assistant_response();
        assert!(!list.containers[1].is_complete());

        list.complete_response();
        assert!(list.containers[1].is_complete());
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
            let node = container.view(80, 0, false);
            assert!(!matches!(node, Node::Empty), "Container should render non-empty");
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
        let node = containers[0].view(80, 0, false);
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
        let user_node = list.containers[0].view(80, 0, false);
        let asst_node = list.containers[1].view(80, 0, false);

        // Check that scrollback nodes use the container IDs
        // We can check this by rendering and seeing the output contains content
        let user_output = render_to_string(&user_node, 80);
        let asst_output = render_to_string(&asst_node, 80);

        assert!(user_output.contains("Hello"), "User content should be rendered");
        assert!(asst_output.contains("World"), "Assistant content should be rendered");

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
        list.update_tool("read_file", |t| {
            t.mark_complete();
        });

        // Verify tool is now complete
        if let Some(ChatContainer::ToolGroup { tools, .. }) = list.containers.last() {
            assert!(tools[0].complete, "Tool should be complete after update_tool by name");
        } else {
            panic!("Expected ToolGroup");
        }
    }

    /// Test reproducing the property test minimal failing input
    #[test]
    fn test_tool_complete_renders_checkmark() {
        use crate::tui::oil::app::{App, ViewContext};
        use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
        use crate::tui::oil::focus::FocusContext;
        use crate::tui::oil::render::render_to_string;
        use crate::tui::oil::TestRuntime;
        use crate::tui::oil::ansi::strip_ansi;

        let mut runtime = TestRuntime::new(80, 24);
        let mut app = OilChatApp::default();

        app.on_message(ChatAppMsg::UserMessage("Query".to_string()));
        app.on_message(ChatAppMsg::TextDelta("A".to_string()));
        app.on_message(ChatAppMsg::TextDelta("a".to_string()));
        app.on_message(ChatAppMsg::TextDelta("text".to_string()));
        app.on_message(ChatAppMsg::ToolCall {
            name: "___".to_string(),
            args: r#"{"query": "test"}"#.to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultDelta {
            name: "___".to_string(),
            delta: "result".to_string(),
        });
        app.on_message(ChatAppMsg::ToolResultComplete {
            name: "___".to_string(),
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
            checkmark_count, combined
        );
    }

    /// Same test but rendering after each event like the property test
    #[test]
    fn test_tool_complete_with_incremental_rendering() {
        use crate::tui::oil::app::{App, ViewContext};
        use crate::tui::oil::chat_app::{ChatAppMsg, OilChatApp};
        use crate::tui::oil::focus::FocusContext;
        use crate::tui::oil::TestRuntime;
        use crate::tui::oil::ansi::strip_ansi;

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
            }),
            ("ToolResultDelta ___", ChatAppMsg::ToolResultDelta {
                name: "___".to_string(),
                delta: "Q_   __ge95.AYs 5sD_9.hQ._HD-1.K_I-N3L-0E  wL".to_string(),
            }),
            ("ToolResultComplete ___", ChatAppMsg::ToolResultComplete {
                name: "___".to_string(),
            }),
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
                    ChatContainer::AssistantResponse { id, blocks, is_continuation, complete, .. } => {
                        eprintln!("{}: Asst({}, cont={}, complete={}): {:?}", i, id, is_continuation, complete, blocks);
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
                ChatContainer::AssistantResponse { id, blocks, is_continuation, complete, .. } => {
                    eprintln!("{}: Asst({}, cont={}, complete={}): {:?}", i, id, is_continuation, complete, blocks);
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
            checkmark_count, combined
        );
    }
}
