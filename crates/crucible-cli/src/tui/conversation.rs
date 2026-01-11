//! Conversation view renderer
//!
//! Renders the chat conversation history with styled messages,
//! tool calls, and status indicators. Designed for ratatui rendering
//! with full viewport control.

use crate::tui::{
    content_block::StreamBlock,
    markdown::MarkdownRenderer,
    styles::{colors, indicators, presets},
};
use once_cell::sync::Lazy;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::cell::RefCell;
use std::collections::VecDeque;

// =============================================================================
// Static Instances
// =============================================================================

/// Global markdown renderer (initialized once to avoid loading syntect themes repeatedly)
static MARKDOWN_RENDERER: Lazy<MarkdownRenderer> = Lazy::new(MarkdownRenderer::new);

// =============================================================================
// Conversation Types
// =============================================================================

/// A message in the conversation
#[derive(Debug, Clone)]
pub enum ConversationItem {
    /// User input message
    UserMessage { content: String },
    /// Assistant text response
    AssistantMessage {
        blocks: Vec<StreamBlock>,
        /// True if still streaming
        is_streaming: bool,
    },
    /// Status indicator (thinking, generating)
    Status(StatusKind),
    /// Tool call with status
    ToolCall(ToolCallDisplay),
}

/// Status indicator types
#[derive(Debug, Clone, PartialEq)]
pub enum StatusKind {
    /// Agent is thinking (no output yet)
    Thinking {
        /// Spinner animation frame (0-3)
        spinner_frame: usize,
    },
    /// Agent is generating tokens
    Generating {
        token_count: usize,
        /// Previous token count for direction indicator
        prev_token_count: usize,
        /// Spinner animation frame (0-3)
        spinner_frame: usize,
    },
    /// Processing (generic)
    Processing {
        message: String,
        /// Spinner animation frame (0-3)
        spinner_frame: usize,
    },
}

/// Tool call display state
#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    pub name: String,
    /// Tool arguments as JSON value
    pub args: serde_json::Value,
    pub status: ToolStatus,
    /// Last N lines of output (truncated)
    pub output_lines: Vec<String>,
}

/// Tool execution status
#[derive(Debug, Clone, PartialEq)]
pub enum ToolStatus {
    Running,
    Complete { summary: Option<String> },
    Error { message: String },
}

// =============================================================================
// Render Cache
// =============================================================================

/// Cached rendered lines for a single conversation item
#[derive(Debug, Clone)]
struct CachedLines {
    /// Width the lines were rendered at
    width: usize,
    /// The rendered lines
    lines: Vec<Line<'static>>,
}

/// Per-item render cache with dirty tracking
///
/// Caches rendered lines for each conversation item to avoid
/// re-parsing markdown on every frame. Automatically invalidates
/// when content changes or terminal width changes.
#[derive(Debug, Default)]
pub struct RenderCache {
    /// Cached lines per item index
    items: Vec<Option<CachedLines>>,
    /// Cached height (line count) per item - always valid if lines are cached
    heights: Vec<usize>,
    /// Cached total height (sum of all item heights)
    total_height: Option<usize>,
    /// Last known terminal width
    last_width: usize,
    /// Global dirty flag - if true, at least one item needs re-render
    dirty: bool,
}

impl RenderCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            heights: Vec::new(),
            total_height: None,
            last_width: 0,
            dirty: true, // Start dirty to ensure first render
        }
    }

    /// Check if any items need re-rendering
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the cache as clean (call after render)
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Mark a specific item as dirty (invalidate its cache)
    pub fn invalidate_item(&mut self, index: usize) {
        if index < self.items.len() {
            self.items[index] = None;
        }
        if index < self.heights.len() {
            self.heights[index] = 0;
        }
        self.total_height = None; // Invalidate total
        self.dirty = true;
    }

    /// Invalidate all items (e.g., on width change)
    pub fn invalidate_all(&mut self) {
        for item in &mut self.items {
            *item = None;
        }
        self.heights.clear();
        self.total_height = None;
        self.dirty = true;
    }

    /// Check width and invalidate all if changed
    pub fn check_width(&mut self, width: usize) {
        if self.last_width != width {
            self.last_width = width;
            self.invalidate_all();
        }
    }

    /// Ensure cache has capacity for the given number of items
    fn ensure_capacity(&mut self, count: usize) {
        if self.items.len() < count {
            self.items.resize_with(count, || None);
        }
        if self.heights.len() < count {
            self.heights.resize(count, 0);
        }
    }

    /// Get cached lines for an item, or None if not cached
    pub fn get(&self, index: usize, width: usize) -> Option<&Vec<Line<'static>>> {
        self.items.get(index).and_then(|cached| {
            cached
                .as_ref()
                .filter(|c| c.width == width)
                .map(|c| &c.lines)
        })
    }

    /// Get cached height for an item, or None if not cached
    pub fn get_height(&self, index: usize) -> Option<usize> {
        self.heights.get(index).copied().filter(|&h| h > 0)
    }

    /// Store rendered lines for an item (also stores height)
    pub fn store(&mut self, index: usize, width: usize, lines: Vec<Line<'static>>) {
        self.ensure_capacity(index + 1);
        let height = lines.len();
        self.items[index] = Some(CachedLines { width, lines });
        self.heights[index] = height;
        self.total_height = None; // Invalidate cached total
    }

    /// Get cached total height, or None if not cached
    pub fn get_total_height(&self) -> Option<usize> {
        self.total_height
    }

    /// Set cached total height
    pub fn set_total_height(&mut self, height: usize) {
        self.total_height = Some(height);
    }

    /// Called when an item is added
    pub fn on_item_added(&mut self) {
        self.items.push(None);
        self.heights.push(0);
        self.total_height = None;
        self.dirty = true;
    }

    /// Called when items are cleared
    pub fn on_clear(&mut self) {
        self.items.clear();
        self.heights.clear();
        self.total_height = None;
        self.dirty = true;
    }
}

// =============================================================================
// Conversation State
// =============================================================================

/// Holds the conversation history for rendering
#[derive(Debug)]
pub struct ConversationState {
    items: VecDeque<ConversationItem>,
    /// Maximum output lines to show per tool
    max_tool_output_lines: usize,
    /// Per-item render cache (RefCell for interior mutability during render)
    cache: RefCell<RenderCache>,
}

impl Default for ConversationState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationState {
    pub fn new() -> Self {
        Self {
            items: VecDeque::new(),
            max_tool_output_lines: 3,
            cache: RefCell::new(RenderCache::new()),
        }
    }

    pub fn with_max_tool_lines(mut self, max: usize) -> Self {
        self.max_tool_output_lines = max;
        self
    }

    /// Check if cache is dirty (any items need re-render)
    pub fn is_dirty(&self) -> bool {
        self.cache.borrow().is_dirty()
    }

    /// Mark cache as clean after render
    pub fn mark_clean(&self) {
        self.cache.borrow_mut().mark_clean();
    }

    /// Invalidate all cached lines (e.g., on resize)
    pub fn invalidate_all(&self) {
        self.cache.borrow_mut().invalidate_all();
    }

    /// Check width and invalidate if changed
    pub fn check_width(&self, width: usize) {
        self.cache.borrow_mut().check_width(width);
    }

    /// Get cached lines for an item, or None if not cached
    pub fn get_cached(&self, index: usize, width: usize) -> Option<Vec<Line<'static>>> {
        self.cache.borrow().get(index, width).cloned()
    }

    /// Get cached height for an item, or None if not cached
    pub fn get_cached_height(&self, index: usize) -> Option<usize> {
        self.cache.borrow().get_height(index)
    }

    /// Get cached total height, or None if not cached
    pub fn get_total_height(&self) -> Option<usize> {
        self.cache.borrow().get_total_height()
    }

    /// Set cached total height
    pub fn set_total_height(&self, height: usize) {
        self.cache.borrow_mut().set_total_height(height);
    }

    /// Store rendered lines for an item
    pub fn store_cached(&self, index: usize, width: usize, lines: Vec<Line<'static>>) {
        self.cache.borrow_mut().store(index, width, lines);
    }

    /// Find the index of the currently streaming assistant message
    fn streaming_item_index(&self) -> Option<usize> {
        self.items.iter().position(|item| {
            matches!(
                item,
                ConversationItem::AssistantMessage {
                    is_streaming: true,
                    ..
                }
            )
        })
    }

    /// Find the index of the most recent tool call with the given name
    fn tool_index(&self, name: &str) -> Option<usize> {
        self.items.iter().rposition(|item| {
            matches!(
                item,
                ConversationItem::ToolCall(tool) if tool.name == name && matches!(tool.status, ToolStatus::Running)
            )
        })
    }

    pub fn push(&mut self, item: ConversationItem) {
        self.items.push_back(item);
        self.cache.borrow_mut().on_item_added();
    }

    pub fn push_user_message(&mut self, content: impl Into<String>) {
        self.items.push_back(ConversationItem::UserMessage {
            content: content.into(),
        });
        self.cache.borrow_mut().on_item_added();
    }

    pub fn push_assistant_message(&mut self, content: impl Into<String>) {
        // Guard: Don't add a non-streaming message if streaming is active
        // This prevents double messages from race conditions
        if self.items.iter().any(|item| {
            matches!(
                item,
                ConversationItem::AssistantMessage {
                    is_streaming: true,
                    ..
                }
            )
        }) {
            // Just append to the streaming message instead
            let content = content.into();
            self.append_or_create_prose(&content);
            self.complete_streaming();
            return;
        }

        // For non-streaming messages, create a single prose block
        let blocks = vec![StreamBlock::prose(content.into())];
        self.items.push_back(ConversationItem::AssistantMessage {
            blocks,
            is_streaming: false,
        });
        self.cache.borrow_mut().on_item_added();
    }

    /// Start streaming an assistant message (creates empty blocks list)
    ///
    /// If already streaming, does nothing to prevent duplicate messages.
    pub fn start_assistant_streaming(&mut self) {
        // Guard: Don't start a new streaming message if one is already active
        if self.streaming_item_index().is_some() {
            return;
        }

        self.items.push_back(ConversationItem::AssistantMessage {
            blocks: Vec::new(),
            is_streaming: true,
        });
        self.cache.borrow_mut().on_item_added();
    }

    /// Append blocks to the most recent streaming assistant message
    pub fn append_streaming_blocks(&mut self, new_blocks: Vec<StreamBlock>) {
        if let Some(idx) = self.streaming_item_index() {
            if let ConversationItem::AssistantMessage { blocks, .. } = &mut self.items[idx] {
                blocks.extend(new_blocks);
                self.cache.borrow_mut().invalidate_item(idx);
            }
        }
    }

    /// Mark the most recent streaming assistant message as complete
    pub fn complete_streaming(&mut self) {
        if let Some(idx) = self.streaming_item_index() {
            if let ConversationItem::AssistantMessage { is_streaming, .. } = &mut self.items[idx] {
                *is_streaming = false;
                self.cache.borrow_mut().invalidate_item(idx);
            }
        }
    }

    /// Append content to the last block of the streaming assistant message
    pub fn append_to_last_block(&mut self, content: &str) {
        if let Some(idx) = self.streaming_item_index() {
            if let ConversationItem::AssistantMessage { blocks, .. } = &mut self.items[idx] {
                if let Some(last_block) = blocks.last_mut() {
                    last_block.append(content);
                    self.cache.borrow_mut().invalidate_item(idx);
                }
            }
        }
    }

    /// Mark the last block of the streaming assistant message as complete
    pub fn complete_last_block(&mut self) {
        if let Some(idx) = self.streaming_item_index() {
            if let ConversationItem::AssistantMessage { blocks, .. } = &mut self.items[idx] {
                if let Some(last_block) = blocks.last_mut() {
                    last_block.complete();
                    self.cache.borrow_mut().invalidate_item(idx);
                }
            }
        }
    }

    /// Append text to the last prose block if it exists and is incomplete,
    /// otherwise create a new prose block. Used for streaming to consolidate text.
    ///
    /// If no streaming assistant message exists, starts a new one. This handles
    /// the case where a tool call interrupted streaming - subsequent prose should
    /// go into a new message to maintain chronological order.
    pub fn append_or_create_prose(&mut self, text: &str) {
        if let Some(idx) = self.streaming_item_index() {
            if let ConversationItem::AssistantMessage { blocks, .. } = &mut self.items[idx] {
                // Check if last block is an incomplete prose block
                if let Some(last_block) = blocks.last_mut() {
                    if last_block.is_prose() && !last_block.is_complete() {
                        last_block.append(text);
                        self.cache.borrow_mut().invalidate_item(idx);
                        return;
                    }
                }
                // Create new prose block
                blocks.push(StreamBlock::prose_partial(text));
                self.cache.borrow_mut().invalidate_item(idx);
                return;
            }
        }

        // No streaming message found - create a new one (e.g., after tool call)
        self.start_assistant_streaming();
        self.append_or_create_prose(text);
    }

    pub fn set_status(&mut self, status: StatusKind) {
        // Remove any existing status - indices shift, so invalidate all
        let had_status = self
            .items
            .iter()
            .any(|item| matches!(item, ConversationItem::Status(_)));
        self.items
            .retain(|item| !matches!(item, ConversationItem::Status(_)));
        if had_status {
            self.cache.borrow_mut().invalidate_all();
        }
        self.items.push_back(ConversationItem::Status(status));
        self.cache.borrow_mut().on_item_added();
    }

    pub fn clear_status(&mut self) {
        let had_status = self
            .items
            .iter()
            .any(|item| matches!(item, ConversationItem::Status(_)));
        self.items
            .retain(|item| !matches!(item, ConversationItem::Status(_)));
        if had_status {
            self.cache.borrow_mut().invalidate_all();
        }
    }

    pub fn push_tool_running(&mut self, name: impl Into<String>, args: serde_json::Value) {
        use crate::tui::content_block::ToolBlockStatus;

        let name = name.into();

        // If we're in the middle of streaming, add tool as a block within the message
        if let Some(idx) = self.streaming_item_index() {
            if let ConversationItem::AssistantMessage { blocks, .. } = &mut self.items[idx] {
                // Complete any partial prose block first
                if let Some(last_block) = blocks.last_mut() {
                    if last_block.is_prose() && !last_block.is_complete() {
                        last_block.complete();
                    }
                }

                // Add tool block to the streaming message
                blocks.push(StreamBlock::Tool {
                    name,
                    args,
                    status: ToolBlockStatus::Running,
                });
                self.cache.borrow_mut().invalidate_item(idx);
                return;
            }
        }

        // Fallback: no streaming message, create standalone tool call
        // (This handles the case where tool call comes before any assistant response)
        let status = self.take_status();

        self.items
            .push_back(ConversationItem::ToolCall(ToolCallDisplay {
                name,
                args,
                status: ToolStatus::Running,
                output_lines: Vec::new(),
            }));
        self.cache.borrow_mut().on_item_added();

        if let Some(s) = status {
            self.items.push_back(s);
            self.cache.borrow_mut().on_item_added();
        }
    }

    /// Remove and return the status item (if any)
    fn take_status(&mut self) -> Option<ConversationItem> {
        let pos = self
            .items
            .iter()
            .position(|item| matches!(item, ConversationItem::Status(_)));
        pos.and_then(|idx| {
            // Removing shifts indices, invalidate all
            self.cache.borrow_mut().invalidate_all();
            self.items.remove(idx)
        })
    }

    pub fn update_tool_output(&mut self, name: &str, output: &str) {
        if let Some(idx) = self.tool_index(name) {
            if let ConversationItem::ToolCall(tool) = &mut self.items[idx] {
                // Truncate to last N lines
                let lines: Vec<String> = output
                    .lines()
                    .rev()
                    .take(self.max_tool_output_lines)
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                tool.output_lines = lines;
                self.cache.borrow_mut().invalidate_item(idx);
            }
        }
    }

    pub fn complete_tool(&mut self, name: &str, summary: Option<String>) {
        use crate::tui::content_block::ToolBlockStatus;

        // First check if tool is a block in the streaming message
        if let Some(idx) = self.streaming_item_index() {
            if let ConversationItem::AssistantMessage { blocks, .. } = &mut self.items[idx] {
                for block in blocks.iter_mut().rev() {
                    if let StreamBlock::Tool {
                        name: block_name,
                        status,
                        ..
                    } = block
                    {
                        if block_name == name {
                            *status = ToolBlockStatus::Complete { summary };
                            self.cache.borrow_mut().invalidate_item(idx);
                            return;
                        }
                    }
                }
            }
        }

        // Fallback: check standalone tool items
        if let Some(idx) = self.tool_index(name) {
            if let ConversationItem::ToolCall(tool) = &mut self.items[idx] {
                tool.status = ToolStatus::Complete { summary };
                self.cache.borrow_mut().invalidate_item(idx);
            }
        }
    }

    pub fn error_tool(&mut self, name: &str, message: impl Into<String>) {
        use crate::tui::content_block::ToolBlockStatus;

        let message = message.into();

        // First check if tool is a block in the streaming message
        if let Some(idx) = self.streaming_item_index() {
            if let ConversationItem::AssistantMessage { blocks, .. } = &mut self.items[idx] {
                for block in blocks.iter_mut().rev() {
                    if let StreamBlock::Tool {
                        name: block_name,
                        status,
                        ..
                    } = block
                    {
                        if block_name == name {
                            *status = ToolBlockStatus::Error {
                                message: message.clone(),
                            };
                            self.cache.borrow_mut().invalidate_item(idx);
                            return;
                        }
                    }
                }
            }
        }

        // Fallback: check standalone tool items
        if let Some(idx) = self.tool_index(name) {
            if let ConversationItem::ToolCall(tool) = &mut self.items[idx] {
                tool.status = ToolStatus::Error { message };
                self.cache.borrow_mut().invalidate_item(idx);
            }
        }
    }

    pub fn items(&self) -> &VecDeque<ConversationItem> {
        &self.items
    }

    /// Get the last assistant message as markdown.
    ///
    /// Reconstructs the original markdown from StreamBlocks.
    /// Returns None if no assistant message exists.
    pub fn last_assistant_markdown(&self) -> Option<String> {
        for item in self.items.iter().rev() {
            if let ConversationItem::AssistantMessage { blocks, .. } = item {
                let mut markdown = String::new();
                for block in blocks {
                    match block {
                        StreamBlock::Prose { text, .. } => {
                            markdown.push_str(text);
                        }
                        StreamBlock::Code { lang, content, .. } => {
                            markdown.push_str("```");
                            if let Some(lang) = lang {
                                markdown.push_str(lang);
                            }
                            markdown.push('\n');
                            markdown.push_str(content);
                            if !content.ends_with('\n') {
                                markdown.push('\n');
                            }
                            markdown.push_str("```\n");
                        }
                        StreamBlock::Tool { name, .. } => {
                            // Tool calls aren't included in markdown output
                            markdown.push_str(&format!("[Tool: {}]\n", name));
                        }
                    }
                }
                return Some(markdown);
            }
        }
        None
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.cache.borrow_mut().on_clear();
    }

    /// Serialize the conversation to markdown format.
    ///
    /// Produces human-readable markdown with role prefixes:
    /// - `> **You:** ` for user messages
    /// - `**Assistant:** ` for assistant messages
    /// - `**Tool:** ` for tool calls
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# Chat Session\n\n");

        for item in &self.items {
            match item {
                ConversationItem::UserMessage { content } => {
                    md.push_str("> **You:** ");
                    for (i, line) in content.lines().enumerate() {
                        if i > 0 {
                            md.push_str("> ");
                        }
                        md.push_str(line);
                        md.push('\n');
                    }
                    md.push('\n');
                }
                ConversationItem::AssistantMessage { blocks, .. } => {
                    md.push_str("**Assistant:** ");
                    for block in blocks {
                        match block {
                            StreamBlock::Prose { text, .. } => {
                                md.push_str(text);
                            }
                            StreamBlock::Code { lang, content, .. } => {
                                md.push_str("```");
                                if let Some(lang) = lang {
                                    md.push_str(lang);
                                }
                                md.push('\n');
                                md.push_str(content);
                                if !content.ends_with('\n') {
                                    md.push('\n');
                                }
                                md.push_str("```\n");
                            }
                            StreamBlock::Tool { name, .. } => {
                                md.push_str(&format!("[Tool: {}]\n", name));
                            }
                        }
                    }
                    if !md.ends_with("\n\n") {
                        md.push('\n');
                    }
                }
                ConversationItem::ToolCall(tool) => {
                    let status = match &tool.status {
                        ToolStatus::Running => "running",
                        ToolStatus::Complete { summary } => {
                            if let Some(s) = summary {
                                s.as_str()
                            } else {
                                "completed"
                            }
                        }
                        ToolStatus::Error { message } => message.as_str(),
                    };
                    md.push_str(&format!("**Tool:** `{}` - {}\n\n", tool.name, status));
                }
                ConversationItem::Status(_) => {
                    // Skip status indicators in export
                }
            }
        }

        md
    }
}

// =============================================================================
// Rendering
// =============================================================================

/// Render a conversation item to lines
pub fn render_item_to_lines(item: &ConversationItem, width: usize) -> Vec<Line<'static>> {
    match item {
        ConversationItem::UserMessage { content } => render_user_message(content, width),
        ConversationItem::AssistantMessage {
            blocks,
            is_streaming,
        } => render_assistant_blocks(blocks, *is_streaming, width),
        ConversationItem::Status(status) => render_status(status),
        ConversationItem::ToolCall(tool) => render_tool_call(tool),
    }
}

fn render_user_message(content: &str, width: usize) -> Vec<Line<'static>> {
    // User messages: inverted style with > prefix
    // Note: User input can be long, apply word wrapping using markdown renderer
    let mut lines = Vec::new();

    // Add blank line before user message for spacing
    lines.push(Line::from(""));

    // Use render_lines() for word wrapping (same as assistant messages)
    let effective_width = if width > 0 { width } else { 80 };
    let rendered_lines = MARKDOWN_RENDERER.render_lines(content, effective_width);

    // Add prefix to each line, with consistent background
    let user_style = presets::user_message();
    let mut first_line = true;
    for line in rendered_lines.iter() {
        let prefix = if first_line {
            first_line = false;
            " > ".to_string() // space + > + space (3 chars)
        } else {
            "   ".to_string() // 3-space indent for continuation
        };

        let mut spans = vec![Span::styled(prefix, presets::user_prefix())];
        // Apply user bg to content spans while preserving their fg color
        for span in line.spans.iter() {
            let style = span.style.bg(user_style.bg.unwrap_or(colors::USER_BG));
            spans.push(Span::styled(span.content.clone(), style));
        }
        lines.push(Line::from(spans));
    }

    lines
}

/// Render assistant message blocks with streaming indicators
fn render_assistant_blocks(
    blocks: &[StreamBlock],
    is_streaming: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Only add blank line for spacing if there's content to render
    // (empty streaming messages shouldn't add extra space)
    if !blocks.is_empty() {
        lines.push(Line::from(""));
    }

    // Track if we've added the first-line prefix yet
    let mut first_content_line = true;

    for (idx, block) in blocks.iter().enumerate() {
        match block {
            StreamBlock::Prose { text, is_complete } => {
                // Render prose as markdown with word-aware wrapping
                let markdown_lines = render_markdown_text(text, width);

                // Add prefix/indent to each line, skipping leading empty lines
                // to prevent orphaned prefix symbols
                for line in markdown_lines {
                    // Skip leading empty lines (before any content has been shown)
                    if first_content_line && line.spans.iter().all(|s| s.content.trim().is_empty())
                    {
                        continue;
                    }
                    lines.push(add_assistant_prefix(line, &mut first_content_line));
                }

                // Show streaming cursor on incomplete blocks
                if !is_complete && is_streaming && idx == blocks.len() - 1 {
                    lines.push(Line::from(vec![
                        Span::raw("   "), // Indent to match
                        Span::styled("▌", presets::streaming()),
                    ]));
                }
            }
            StreamBlock::Code {
                lang,
                content,
                is_complete,
            } => {
                // Render code block - no wrapping for code
                let code_lines = render_code_block(lang.as_deref(), content);

                // Add prefix/indent to each line, skipping leading empty lines
                for line in code_lines {
                    if first_content_line && line.spans.iter().all(|s| s.content.trim().is_empty())
                    {
                        continue;
                    }
                    lines.push(add_assistant_prefix(line, &mut first_content_line));
                }

                // Show streaming cursor on incomplete blocks
                if !is_complete && is_streaming && idx == blocks.len() - 1 {
                    lines.push(Line::from(vec![
                        Span::raw("   "), // Indent to match
                        Span::styled("▌", presets::streaming()),
                    ]));
                }
            }
            StreamBlock::Tool { name, args, status } => {
                use crate::tui::content_block::ToolBlockStatus;

                // Format tool call with status indicator
                // Indicator: white dot running, green dot complete, red X error
                // Text: white normally, red on error
                let (indicator, indicator_style, text_style) = match status {
                    ToolBlockStatus::Running => (
                        indicators::TOOL_RUNNING,
                        presets::tool_running(),
                        presets::tool_running(),
                    ),
                    ToolBlockStatus::Complete { .. } => (
                        indicators::TOOL_COMPLETE,
                        presets::tool_complete(),
                        presets::tool_running(), // White text for completed tools
                    ),
                    ToolBlockStatus::Error { .. } => (
                        indicators::TOOL_ERROR,
                        presets::tool_error(),
                        presets::tool_error(), // Red text for errors
                    ),
                };

                // Format args as key=value pairs (compact)
                let args_str = format_tool_args(args);

                // Build the tool line suffix
                let suffix = match status {
                    ToolBlockStatus::Complete { summary } => {
                        summary.as_ref().map(|s| format!(" → {}", s)).unwrap_or_default()
                    }
                    ToolBlockStatus::Error { message } => format!(" → {}", message),
                    ToolBlockStatus::Running => String::new(),
                };

                // Build tool line content (indicator is part of content, not prefix)
                // Note: args_str already includes parens from format_tool_args()
                let tool_line = Line::from(vec![
                    Span::styled(format!("{} ", indicator), indicator_style),
                    Span::styled(format!("{}{}", name, args_str), text_style),
                    Span::styled(suffix, text_style),
                ]);

                // Use add_assistant_prefix for consistent prefixing
                lines.push(add_assistant_prefix(tool_line, &mut first_content_line));
            }
        }
    }

    lines
}

/// Add assistant prefix to a line (first line gets " · ", others get "   ")
fn add_assistant_prefix(line: Line<'static>, first_content_line: &mut bool) -> Line<'static> {
    let prefix = if *first_content_line {
        *first_content_line = false;
        Span::styled(
            format!(" {} ", indicators::ASSISTANT_PREFIX),
            presets::assistant_prefix(),
        )
    } else {
        Span::raw("   ") // 3-space indent for continuation
    };

    let mut spans = vec![prefix];
    spans.extend(line.spans);
    Line::from(spans)
}

/// Helper to render markdown text with word-aware wrapping
fn render_markdown_text(content: &str, width: usize) -> Vec<Line<'static>> {
    // Use width for word-aware wrapping (0 = no wrap, use default)
    let effective_width = if width > 0 { width } else { 80 };
    MARKDOWN_RENDERER.render_lines(content, effective_width)
}

/// Helper to render a code block with optional language (no wrapping)
fn render_code_block(lang: Option<&str>, content: &str) -> Vec<Line<'static>> {
    // Format as markdown code block and render without wrapping
    let markdown = if let Some(lang) = lang {
        format!("```{}\n{}\n```", lang, content)
    } else {
        format!("```\n{}\n```", content)
    };

    // Code blocks don't wrap
    render_markdown_text(&markdown, 0)
}

/// Legacy function for backward compatibility (now wraps block rendering)
fn render_assistant_message(content: &str) -> Vec<Line<'static>> {
    // Convert string to single prose block and render with default width
    let blocks = vec![StreamBlock::prose(content)];
    render_assistant_blocks(&blocks, false, 80) // Default 80 column width for tests
}

fn render_status(status: &StatusKind) -> Vec<Line<'static>> {
    let (spinner_frame, text, style) = match status {
        StatusKind::Thinking { spinner_frame } => (
            *spinner_frame,
            "Thinking...".to_string(),
            presets::thinking(),
        ),
        StatusKind::Generating {
            token_count,
            prev_token_count,
            spinner_frame,
        } => {
            let text = if *token_count > 0 {
                // Direction indicator based on token change
                let direction = if *token_count > *prev_token_count {
                    "↑"
                } else if *token_count < *prev_token_count {
                    "↓"
                } else {
                    " "
                };
                format!("Generating... {}{} tokens", direction, token_count)
            } else {
                "Generating...".to_string()
            };
            (*spinner_frame, text, presets::streaming())
        }
        StatusKind::Processing {
            message,
            spinner_frame,
        } => (*spinner_frame, message.clone(), presets::streaming()),
    };

    // Get spinner character (cycle through frames)
    let spinner = indicators::SPINNER_FRAMES[spinner_frame % indicators::SPINNER_FRAMES.len()];

    // Format with alignment prefix: " ◐ " aligns with " > " and " · "
    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(format!(" {} ", spinner), style),
            Span::styled(text, style),
        ]),
    ]
}

/// Format tool arguments for display (compact single-line format).
/// Hides default-looking values (null, false, small numbers) to reduce noise.
fn format_tool_args(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) if map.is_empty() => String::new(),
        serde_json::Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .filter_map(|(k, v)| {
                    // Skip default-looking values to reduce noise
                    match v {
                        serde_json::Value::Null => return None,
                        serde_json::Value::Bool(false) => return None,
                        serde_json::Value::Number(n) => {
                            // Skip common default numbers (0, 1, small limits)
                            if let Some(i) = n.as_i64() {
                                if i <= 1 || i == 100 || i == 50 || i == 10 {
                                    return None;
                                }
                            }
                        }
                        serde_json::Value::String(s) if s.is_empty() => return None,
                        _ => {}
                    }

                    let v_str = match v {
                        serde_json::Value::String(s) => {
                            // Truncate long strings
                            if s.len() > 40 {
                                format!("\"{}...\"", &s[..37])
                            } else {
                                format!("\"{}\"", s)
                            }
                        }
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
                        serde_json::Value::Object(_) => "{...}".to_string(),
                        serde_json::Value::Null => return None, // Already filtered above
                    };
                    Some(format!("{}={}", k, v_str))
                })
                .collect();
            if parts.is_empty() {
                String::new()
            } else {
                format!("({})", parts.join(", "))
            }
        }
        serde_json::Value::Null => String::new(),
        _ => format!("({})", args),
    }
}

/// Render a tool call without leading blank line.
/// Spacing between items is handled by ConversationWidget::render_to_lines.
fn render_tool_call(tool: &ToolCallDisplay) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Skip tool calls with empty names (prevents orphan spinner bug)
    if tool.name.is_empty() {
        return lines;
    }

    // Note: No blank line added here - spacing is handled at the widget level
    // to allow consecutive tool calls to be grouped together.

    // Tool status line - use " X " prefix to align with " > " and " ● " message prefixes
    // Indicator: white dot running, green dot complete, red X error
    // Text: white normally, red on error
    let (indicator, indicator_style, text_style) = match &tool.status {
        ToolStatus::Running => (
            indicators::TOOL_RUNNING,
            presets::tool_running(),
            presets::tool_running(),
        ),
        ToolStatus::Complete { .. } => (
            indicators::TOOL_COMPLETE,
            presets::tool_complete(),
            presets::tool_running(), // White text for completed tools
        ),
        ToolStatus::Error { .. } => (
            indicators::TOOL_ERROR,
            presets::tool_error(),
            presets::tool_error(), // Red text for errors
        ),
    };

    // Format tool name with arguments
    let args_str = format_tool_args(&tool.args);

    let status_suffix = match &tool.status {
        ToolStatus::Running => String::new(),
        ToolStatus::Complete { summary } => summary
            .as_ref()
            .map(|s| format!(" → {}", s))
            .unwrap_or_default(),
        ToolStatus::Error { message } => format!(" → {}", message),
    };

    // Use " X " prefix format to align with user/assistant message prefixes
    lines.push(Line::from(vec![
        Span::styled(format!(" {} ", indicator), indicator_style),
        Span::styled(format!("{}{}", tool.name, args_str), text_style),
        Span::styled(status_suffix, text_style),
    ]));

    // Tool output lines - only show while running (max 3 lines)
    // Output disappears when tool completes (shrinks to single line)
    if matches!(tool.status, ToolStatus::Running) {
        let output_lines: Vec<_> = tool.output_lines.iter().rev().take(3).collect();
        for line in output_lines.into_iter().rev() {
            lines.push(Line::from(vec![Span::styled(
                format!("    {}", line),
                presets::tool_output(),
            )]));
        }
    }

    lines
}

// =============================================================================
// Conversation Widget
// =============================================================================

/// Widget that renders the full conversation
pub struct ConversationWidget<'a> {
    state: &'a ConversationState,
    /// Scroll offset from bottom (0 = at bottom)
    scroll_offset: usize,
}

impl<'a> ConversationWidget<'a> {
    pub fn new(state: &'a ConversationState) -> Self {
        Self {
            state,
            scroll_offset: 0,
        }
    }

    pub fn scroll_offset(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    fn render_to_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut all_lines = Vec::new();
        let items = self.state.items();

        for (i, item) in items.iter().enumerate() {
            // Check if we need spacing before this item
            // Tool calls don't include their own spacing anymore, so we add it here
            // BUT skip blank line when previous item was also a tool call (group them)
            if matches!(item, ConversationItem::ToolCall(_)) {
                let prev_was_tool =
                    i > 0 && matches!(items.get(i - 1), Some(ConversationItem::ToolCall(_)));

                if !prev_was_tool {
                    // Add blank line before tool call (unless consecutive)
                    all_lines.push(Line::from(""));
                }
            }

            all_lines.extend(render_item_to_lines(item, width));
        }

        all_lines
    }
}

impl Widget for ConversationWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use crate::tui::constants::UiConstants;
        // Content width minus prefix (" ● " = 3 chars) and right margin (1 char)
        let content_width = UiConstants::content_width(area.width);
        let lines = self.render_to_lines(content_width);
        let content_height = lines.len();
        let viewport_height = area.height as usize;

        if content_height == 0 {
            return;
        }

        // Calculate the scroll position
        // scroll_offset = 0 means at bottom (newest content visible)
        // scroll_offset = N means N lines scrolled up from bottom

        if content_height <= viewport_height {
            // Content fits in viewport - render at bottom
            let empty_space = viewport_height - content_height;
            let offset_area = Rect {
                x: area.x,
                y: area.y + empty_space as u16,
                width: area.width,
                height: content_height as u16,
            };
            // No Wrap needed - ratatui markdown renderer pre-wraps at word boundaries
            let paragraph = Paragraph::new(lines);
            paragraph.render(offset_area, buf);
        } else {
            // Content exceeds viewport - apply scroll
            // scroll_offset = 0: show last viewport_height lines
            // scroll_offset = N: show lines from (content - viewport - N) to (content - N)
            let max_scroll = content_height - viewport_height;
            let effective_scroll = self.scroll_offset.min(max_scroll);

            // Convert bottom-relative to top-relative scroll
            let top_scroll = max_scroll - effective_scroll;

            // No Wrap needed - ratatui markdown renderer pre-wraps at word boundaries
            let paragraph = Paragraph::new(lines).scroll((top_scroll as u16, 0));
            paragraph.render(area, buf);
        }
    }
}

// =============================================================================
// Input Box Widget
// =============================================================================

/// The input box at the bottom of the screen
pub struct InputBoxWidget<'a> {
    content: &'a str,
    cursor_position: usize,
    focused: bool,
}

impl<'a> InputBoxWidget<'a> {
    pub fn new(content: &'a str, cursor_position: usize) -> Self {
        Self {
            content,
            cursor_position,
            focused: true,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for InputBoxWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Input box with accent background
        let style = if self.focused {
            presets::input_box()
        } else {
            presets::dim()
        };

        // Fill background
        buf.set_style(area, style);

        // Render content with cursor, centered vertically
        let content_with_cursor = if self.cursor_position >= self.content.len() {
            format!("{} ", self.content) // Space for cursor at end
        } else {
            self.content.to_string()
        };

        let line = Line::from(vec![Span::raw(" > "), Span::raw(content_with_cursor)]);

        // Center vertically in the area
        use crate::tui::geometry::PopupGeometry;
        let middle_row = PopupGeometry::center_vertically(area, 1);
        let centered_area = Rect {
            x: area.x,
            y: middle_row,
            width: area.width,
            height: 1,
        };

        let paragraph = Paragraph::new(line).style(style);
        paragraph.render(centered_area, buf);
    }
}

// =============================================================================
// Status Bar Widget
// =============================================================================

/// Status bar shown below the input
pub struct StatusBarWidget<'a> {
    mode_id: &'a str,
    token_count: Option<usize>,
    status: &'a str,
    notification: Option<(&'a str, crate::tui::notification::NotificationLevel)>,
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(mode_id: &'a str, status: &'a str) -> Self {
        Self {
            mode_id,
            token_count: None,
            status,
            notification: None,
        }
    }

    pub fn token_count(mut self, count: usize) -> Self {
        self.token_count = Some(count);
        self
    }

    pub fn notification(
        mut self,
        notification: Option<(&'a str, crate::tui::notification::NotificationLevel)>,
    ) -> Self {
        self.notification = notification;
        self
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode_style = presets::mode(self.mode_id);
        let mode_name = match self.mode_id {
            "plan" => "Plan",
            "act" => "Act",
            "auto" => "Auto",
            _ => self.mode_id,
        };

        let mut left_spans = vec![
            Span::styled(indicators::MODE_ARROW, presets::dim()),
            Span::raw(" "),
            Span::styled(mode_name, mode_style),
        ];

        if let Some(count) = self.token_count {
            left_spans.push(Span::styled(" │ ", presets::dim()));
            left_spans.push(Span::styled(
                format!("{} tokens", count),
                presets::metrics(),
            ));
        }

        left_spans.push(Span::styled(" │ ", presets::dim()));
        left_spans.push(Span::styled(self.status.to_string(), presets::dim()));

        // Add notification on the right if present
        if let Some((msg, level)) = self.notification {
            use crate::tui::notification::NotificationLevel;
            let style = match level {
                NotificationLevel::Info => presets::dim(),
                NotificationLevel::Error => Style::default().fg(Color::Red),
            };

            // Calculate padding to right-align notification
            let left_text: String = left_spans.iter().map(|s| s.content.as_ref()).collect();
            let left_width = left_text.chars().count();
            let notif_text = format!(" {}", msg);
            let notif_width = notif_text.chars().count();
            let available_width = area.width as usize;

            if left_width + notif_width < available_width {
                let padding = available_width - left_width - notif_width;
                left_spans.push(Span::raw(" ".repeat(padding)));
                left_spans.push(Span::styled(notif_text, style));
            }
        }

        let line = Line::from(left_spans);
        let paragraph = Paragraph::new(line).style(presets::status_line());
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Modifier, Style};

    #[test]
    fn test_conversation_state_new() {
        let state = ConversationState::new();
        assert!(state.items().is_empty());
    }

    #[test]
    fn test_append_or_create_prose_consolidates_text() {
        let mut state = ConversationState::new();
        state.start_assistant_streaming();

        // First text creates a prose block
        state.append_or_create_prose("Line 1\n");
        // Second text appends to the same block
        state.append_or_create_prose("Line 2\n");
        // Third text also appends
        state.append_or_create_prose("Line 3\n");

        // Should have exactly ONE assistant message with ONE block
        assert_eq!(state.items().len(), 1);
        if let ConversationItem::AssistantMessage { blocks, .. } = &state.items()[0] {
            assert_eq!(blocks.len(), 1, "Should have exactly one prose block");
            assert_eq!(blocks[0].text(), "Line 1\nLine 2\nLine 3\n");
        } else {
            panic!("Expected assistant message");
        }
    }

    #[test]
    fn test_push_user_message() {
        let mut state = ConversationState::new();
        state.push_user_message("Hello");
        assert_eq!(state.items().len(), 1);
        assert!(matches!(
            &state.items()[0],
            ConversationItem::UserMessage { content } if content == "Hello"
        ));
    }

    #[test]
    fn test_push_assistant_message() {
        let mut state = ConversationState::new();
        state.push_assistant_message("Hi there!");
        assert_eq!(state.items().len(), 1);
    }

    #[test]
    fn test_set_status_replaces_existing() {
        let mut state = ConversationState::new();
        state.set_status(StatusKind::Thinking { spinner_frame: 0 });
        state.set_status(StatusKind::Generating {
            token_count: 50,
            prev_token_count: 0,
            spinner_frame: 0,
        });

        let status_count = state
            .items()
            .iter()
            .filter(|i| matches!(i, ConversationItem::Status(_)))
            .count();
        assert_eq!(status_count, 1);
    }

    #[test]
    fn test_tool_lifecycle() {
        let mut state = ConversationState::new();

        state.push_tool_running("grep", serde_json::json!({"pattern": "test"}));
        state.update_tool_output("grep", "line1\nline2\nline3");
        state.complete_tool("grep", Some("3 matches".to_string()));

        let tool = state.items().iter().find_map(|i| {
            if let ConversationItem::ToolCall(t) = i {
                Some(t)
            } else {
                None
            }
        });

        assert!(tool.is_some());
        let tool = tool.unwrap();
        assert_eq!(tool.name, "grep");
        assert!(matches!(tool.status, ToolStatus::Complete { .. }));
    }

    #[test]
    fn test_render_user_message_lines() {
        let lines = render_user_message("Hello world", 80);
        assert!(!lines.is_empty());
        // First line is blank for spacing
        // Second line should contain the message
    }

    #[test]
    fn test_render_tool_running() {
        let tool = ToolCallDisplay {
            name: "grep".to_string(),
            args: serde_json::json!({"pattern": "test"}),
            status: ToolStatus::Running,
            output_lines: vec!["output line".to_string()],
        };
        let lines = render_tool_call(&tool);
        assert!(!lines.is_empty());
    }

    // =============================================================================
    // Markdown Rendering Tests
    // =============================================================================

    #[test]
    fn test_assistant_message_renders_code_blocks() {
        let content = "Here's some code:\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let lines = render_assistant_message(content);

        // Should have content (blank line + text + code block)
        assert!(
            lines.len() > 3,
            "Expected multiple lines, got {}",
            lines.len()
        );

        // Look for any styling changes that indicate code formatting
        // Code blocks should have different styling than plain text
        let has_styled_content = lines.iter().any(|line| {
            line.spans.iter().any(|span| {
                // Check if any span has non-default styling
                span.style != Style::default() && span.style != presets::assistant_message()
            })
        });

        assert!(
            has_styled_content,
            "Expected code blocks to have distinct styling"
        );
    }

    #[test]
    fn test_assistant_message_renders_bold() {
        let content = "This is **bold** text.";
        let lines = render_assistant_message(content);

        // Should have at least blank line + content
        assert!(lines.len() >= 2);

        // Look for bold modifier in any span
        let has_bold = lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.style.add_modifier.contains(Modifier::BOLD))
        });

        assert!(has_bold, "Expected bold text to have BOLD modifier");
    }

    #[test]
    fn test_assistant_message_renders_italic() {
        let content = "This is *italic* text.";
        let lines = render_assistant_message(content);

        assert!(lines.len() >= 2);

        // Look for italic modifier in any span
        let has_italic = lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.style.add_modifier.contains(Modifier::ITALIC))
        });

        assert!(has_italic, "Expected italic text to have ITALIC modifier");
    }

    #[test]
    fn test_assistant_message_renders_inline_code() {
        let content = "Use `cargo build` to compile.";
        let lines = render_assistant_message(content);

        assert!(lines.len() >= 2);

        // Inline code should have different styling (background or color change)
        let has_code_styling = lines.iter().any(|line| {
            line.spans.iter().any(|span| {
                // Check for background color or distinct foreground
                span.style.bg.is_some()
                    || (span.style.fg.is_some() && span.style != presets::assistant_message())
            })
        });

        assert!(
            has_code_styling,
            "Expected inline code to have distinct styling (background or color)"
        );
    }

    #[test]
    fn test_inline_code_preserves_spacing() {
        // Test that inline code doesn't lose leading/trailing spaces
        let content = "Run `cargo test` and check output.";
        let lines = render_assistant_message(content);

        // Get the full text content (skip the prefix added by add_assistant_prefix)
        let text: String = lines
            .iter()
            .skip(1) // Skip blank line
            .flat_map(|line| line.spans.iter().skip(1)) // Skip the prefix span
            .map(|span| span.content.as_ref())
            .collect();

        // Should preserve spacing: "Run " + "cargo test" + " and check output."
        // The inline code should have space before and after
        assert!(
            text.contains("Run "),
            "Expected 'Run ' before inline code, got: '{}'",
            text
        );
        assert!(
            text.contains(" and check"),
            "Expected ' and check' after inline code, got: '{}'",
            text
        );
    }

    #[test]
    fn test_assistant_message_plain_text_unchanged() {
        let content = "Just plain text here.";
        let lines = render_assistant_message(content);

        // Should still work for plain text
        assert!(lines.len() >= 2);

        // Should contain the text content
        let text_content: String = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();

        assert!(text_content.contains("plain text"));
    }

    #[test]
    fn test_assistant_message_multiline_markdown() {
        let content =
            "# Heading\n\nSome **bold** and *italic* text.\n\n- List item 1\n- List item 2";
        let lines = render_assistant_message(content);

        // Should have multiple lines
        assert!(lines.len() > 5);

        // Should have some styled content
        let has_styling = lines
            .iter()
            .any(|line| line.spans.iter().any(|span| span.style != Style::default()));

        assert!(has_styling, "Expected markdown formatting to apply styles");
    }

    // =============================================================================
    // Message Alignment Tests
    // =============================================================================

    #[test]
    fn test_user_and_assistant_prefix_alignment() {
        // User messages should have " > " prefix (3 chars: space + > + space)
        let user_lines = render_user_message("Hello", 80);
        // Skip the blank line
        let user_content_line = &user_lines[1];

        // Check user prefix starts with " > "
        let user_text: String = user_content_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            user_text.starts_with(" > "),
            "User message should start with ' > ', got: '{}'",
            user_text
        );

        // Assistant messages should have " ● " prefix (3 chars: space + ● + space)
        let blocks = vec![crate::tui::StreamBlock::prose("World")];
        let assistant_lines = render_assistant_blocks(&blocks, false, 80);
        // Skip the blank line
        let assistant_content_line = &assistant_lines[1];

        // Check assistant prefix starts with " ● "
        let assistant_text: String = assistant_content_line
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(
            assistant_text.starts_with(" ● "),
            "Assistant message should start with ' ● ', got: '{}'",
            assistant_text
        );
    }

    #[test]
    fn test_assistant_multiline_alignment() {
        // Multi-line assistant messages should have:
        // - First line: " ● " prefix
        // - Continuation lines: "   " (3 spaces) indent
        let blocks = vec![crate::tui::StreamBlock::prose(
            "Line one\nLine two\nLine three",
        )];
        let lines = render_assistant_blocks(&blocks, false, 80);

        // Skip blank line, get content lines
        let content_lines: Vec<_> = lines.iter().skip(1).collect();

        // All content lines should start with 3-char prefix
        for (i, line) in content_lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            if i == 0 {
                assert!(
                    text.starts_with(" ● "),
                    "First line should have ' ● ' prefix, got: '{}'",
                    text
                );
            } else if !text.trim().is_empty() {
                // Continuation lines should have indent
                assert!(
                    text.starts_with("   "),
                    "Continuation line {} should have 3-space indent, got: '{}'",
                    i,
                    text
                );
            }
        }
    }

    // =============================================================================
    // Bottom-Anchored Rendering Tests
    // =============================================================================

    #[test]
    fn test_conversation_widget_bottom_anchored_short() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        // Create widget with just one message
        let mut state = ConversationState::new();
        state.push_user_message("Hello");

        let widget = ConversationWidget::new(&state);

        // Render to a buffer
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                f.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        // Content should be at bottom, not top
        // Check that top rows are empty and bottom rows have content
        let top_line: String = (0..80)
            .map(|x| buffer.cell((x, 0)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();

        let _bottom_line: String = (0..80)
            .map(|x| buffer.cell((x, 19)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();

        // Top line should be mostly empty (whitespace)
        assert!(
            top_line.trim().is_empty(),
            "Expected top line to be empty, got: '{}'",
            top_line
        );

        // Bottom area should have content (the user message)
        // Check a few lines from the bottom for content
        let has_content = (15..20).any(|y| {
            let line: String = (0..80)
                .map(|x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect();
            line.contains("Hello")
        });

        assert!(
            has_content,
            "Expected 'Hello' to appear near bottom of viewport"
        );
    }

    #[test]
    fn test_conversation_widget_scroll_offset() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let mut state = ConversationState::new();
        for i in 0..30 {
            state.push_user_message(format!("Message {}", i));
        }

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        // Test with scroll_offset = 0 (should show newest at bottom)
        terminal
            .draw(|f| {
                let area = f.area();
                let widget = ConversationWidget::new(&state).scroll_offset(0);
                f.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // Should contain recent messages (29, 28, etc.)
        assert!(
            content.contains("Message 29"),
            "Expected newest message 29 to be visible with scroll_offset=0"
        );

        // Test with scroll_offset = 10 (should show older messages)
        terminal
            .draw(|f| {
                let area = f.area();
                let widget = ConversationWidget::new(&state).scroll_offset(10);
                f.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // Should NOT contain the newest message when scrolled up
        assert!(
            !content.contains("Message 29"),
            "Expected message 29 to be scrolled out of view with scroll_offset=10"
        );
    }

    /// Regression test: Status should always appear after tool calls, not before them.
    /// Bug: When status is set and then tool calls are pushed, status ends up
    /// in the middle of the conversation instead of at the end.
    #[test]
    fn test_status_always_last_after_tool_calls() {
        let mut state = ConversationState::new();

        // Set status first
        state.set_status(StatusKind::Generating {
            token_count: 50,
            prev_token_count: 0,
            spinner_frame: 0,
        });

        // Push tool calls after - status should still be last
        state.push_tool_running("glob", serde_json::json!({"pattern": "*.rs"}));
        state.push_tool_running("read", serde_json::json!({"path": "main.rs"}));

        // Verify status is the last item
        let items = state.items();
        let last_item = items.back().expect("Should have items");
        assert!(
            matches!(last_item, ConversationItem::Status(_)),
            "Status should be the last item, but got: {:?}",
            last_item
        );

        // Verify we still have exactly 3 items (2 tools + 1 status)
        assert_eq!(items.len(), 3, "Should have 2 tools + 1 status");
    }

    // =============================================================================
    // to_markdown tests
    // =============================================================================

    #[test]
    fn test_to_markdown_empty_conversation() {
        let state = ConversationState::new();
        let md = state.to_markdown();
        assert_eq!(md, "# Chat Session\n\n");
    }

    #[test]
    fn test_to_markdown_user_message() {
        let mut state = ConversationState::new();
        state.push_user_message("Hello there!");
        let md = state.to_markdown();
        assert!(md.contains("> **You:** Hello there!"));
    }

    #[test]
    fn test_to_markdown_assistant_message() {
        let mut state = ConversationState::new();
        state.push_assistant_message("Hi! How can I help?");
        let md = state.to_markdown();
        assert!(md.contains("**Assistant:** Hi! How can I help?"));
    }

    #[test]
    fn test_to_markdown_full_conversation() {
        let mut state = ConversationState::new();
        state.push_user_message("What is 2+2?");
        state.push_assistant_message("2+2 equals 4.");
        let md = state.to_markdown();

        assert!(md.contains("# Chat Session"));
        assert!(md.contains("> **You:** What is 2+2?"));
        assert!(md.contains("**Assistant:** 2+2 equals 4."));
    }

    #[test]
    fn test_to_markdown_with_tool_call() {
        let mut state = ConversationState::new();
        state.push_tool_running("calculator", serde_json::json!({"expr": "2+2"}));
        state.complete_tool("calculator", Some("4".to_string()));
        let md = state.to_markdown();

        assert!(md.contains("**Tool:** `calculator` - 4"));
    }

    /// Test that tables rendered in assistant messages don't have blank lines between rows.
    /// This tests the full path through render_item_to_lines.
    #[test]
    fn test_table_no_blank_lines_in_conversation() {
        let table_content = "Here's a table:\n\n| Tool | Description |\n|------|-------------|\n| Glob | Fast file pattern matching tool that finds files by pattern. |\n| Grep | Search content with regex. |";

        // Render through the full conversation path
        let lines = render_item_to_lines(
            &ConversationItem::AssistantMessage {
                blocks: vec![StreamBlock::prose(table_content)],
                is_streaming: false,
            },
            60, // Width that requires some wrapping
        );

        // Convert to text for analysis
        let line_texts: Vec<String> = lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();

        // Check for blank lines between table rows
        let mut in_table = false;
        for (i, text) in line_texts.iter().enumerate() {
            let is_table_row =
                text.contains('│') || text.contains('├') || text.contains('┌') || text.contains('└');
            let is_blank_or_only_prefix = text.trim().is_empty()
                || text == "   " // Just prefix spaces
                || text.chars().all(|c| c.is_whitespace() || c == '·' || c == '●');

            if is_table_row {
                in_table = true;
            }

            // If we're in a table and see a blank line, that's unexpected
            if in_table && is_blank_or_only_prefix && !text.contains('└') {
                // Check if this is really inside a table (not the blank line before)
                let prev_was_table = i > 0
                    && line_texts[i - 1]
                        .chars()
                        .any(|c| c == '│' || c == '├' || c == '┌');
                let next_is_table = i + 1 < line_texts.len()
                    && line_texts[i + 1]
                        .chars()
                        .any(|c| c == '│' || c == '├' || c == '└');

                if prev_was_table && next_is_table {
                    panic!(
                        "Found blank line at index {} between table rows. Prev: '{}', Current: '{}', Next: '{}'",
                        i,
                        line_texts.get(i.saturating_sub(1)).unwrap_or(&String::new()),
                        text,
                        line_texts.get(i + 1).unwrap_or(&String::new())
                    );
                }
            }
        }
    }
}
