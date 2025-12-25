//! Conversation view renderer
//!
//! Renders the chat conversation history with styled messages,
//! tool calls, and status indicators. Designed for ratatui rendering
//! with full viewport control.

use crate::tui::{
    content_block::ContentBlock,
    markdown::MarkdownRenderer,
    styles::{indicators, presets},
};
use ansi_to_tui::IntoText;
use once_cell::sync::Lazy;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

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
        blocks: Vec<ContentBlock>,
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
// Conversation State
// =============================================================================

/// Holds the conversation history for rendering
#[derive(Debug, Default)]
pub struct ConversationState {
    items: Vec<ConversationItem>,
    /// Maximum output lines to show per tool
    max_tool_output_lines: usize,
}

impl ConversationState {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            max_tool_output_lines: 3,
        }
    }

    pub fn with_max_tool_lines(mut self, max: usize) -> Self {
        self.max_tool_output_lines = max;
        self
    }

    pub fn push(&mut self, item: ConversationItem) {
        self.items.push(item);
    }

    pub fn push_user_message(&mut self, content: impl Into<String>) {
        self.items.push(ConversationItem::UserMessage {
            content: content.into(),
        });
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
        let blocks = vec![ContentBlock::prose(content.into())];
        self.items.push(ConversationItem::AssistantMessage {
            blocks,
            is_streaming: false,
        });
    }

    /// Start streaming an assistant message (creates empty blocks list)
    ///
    /// If already streaming, does nothing to prevent duplicate messages.
    pub fn start_assistant_streaming(&mut self) {
        // Guard: Don't start a new streaming message if one is already active
        if self.items.iter().any(|item| {
            matches!(
                item,
                ConversationItem::AssistantMessage {
                    is_streaming: true,
                    ..
                }
            )
        }) {
            return;
        }

        self.items.push(ConversationItem::AssistantMessage {
            blocks: Vec::new(),
            is_streaming: true,
        });
    }

    /// Append blocks to the most recent streaming assistant message
    pub fn append_streaming_blocks(&mut self, new_blocks: Vec<ContentBlock>) {
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::AssistantMessage {
                blocks,
                is_streaming,
            } = item
            {
                if *is_streaming {
                    blocks.extend(new_blocks);
                    return;
                }
            }
        }
    }

    /// Mark the most recent streaming assistant message as complete
    pub fn complete_streaming(&mut self) {
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::AssistantMessage { is_streaming, .. } = item {
                if *is_streaming {
                    *is_streaming = false;
                    return;
                }
            }
        }
    }

    /// Append content to the last block of the streaming assistant message
    pub fn append_to_last_block(&mut self, content: &str) {
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::AssistantMessage {
                blocks,
                is_streaming,
            } = item
            {
                if *is_streaming {
                    if let Some(last_block) = blocks.last_mut() {
                        last_block.append(content);
                    }
                    return;
                }
            }
        }
    }

    /// Mark the last block of the streaming assistant message as complete
    pub fn complete_last_block(&mut self) {
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::AssistantMessage {
                blocks,
                is_streaming,
            } = item
            {
                if *is_streaming {
                    if let Some(last_block) = blocks.last_mut() {
                        last_block.complete();
                    }
                    return;
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
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::AssistantMessage {
                blocks,
                is_streaming,
            } = item
            {
                if *is_streaming {
                    // Check if last block is an incomplete prose block
                    if let Some(last_block) = blocks.last_mut() {
                        if last_block.is_prose() && !last_block.is_complete() {
                            last_block.append(text);
                            return;
                        }
                    }
                    // Create new prose block
                    blocks.push(ContentBlock::prose_partial(text));
                    return;
                }
            }
        }

        // No streaming message found - create a new one (e.g., after tool call)
        self.start_assistant_streaming();
        self.append_or_create_prose(text);
    }

    pub fn set_status(&mut self, status: StatusKind) {
        // Remove any existing status
        self.items
            .retain(|item| !matches!(item, ConversationItem::Status(_)));
        self.items.push(ConversationItem::Status(status));
    }

    pub fn clear_status(&mut self) {
        self.items
            .retain(|item| !matches!(item, ConversationItem::Status(_)));
    }

    pub fn push_tool_running(&mut self, name: impl Into<String>) {
        // Complete any streaming assistant message first, so that subsequent
        // prose creates a new message (preserves chronological order)
        self.complete_streaming();

        self.items.push(ConversationItem::ToolCall(ToolCallDisplay {
            name: name.into(),
            status: ToolStatus::Running,
            output_lines: Vec::new(),
        }));
    }

    pub fn update_tool_output(&mut self, name: &str, output: &str) {
        // Find the most recent tool with this name and update it
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::ToolCall(tool) = item {
                if tool.name == name && matches!(tool.status, ToolStatus::Running) {
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
                    return;
                }
            }
        }
    }

    pub fn complete_tool(&mut self, name: &str, summary: Option<String>) {
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::ToolCall(tool) = item {
                if tool.name == name && matches!(tool.status, ToolStatus::Running) {
                    tool.status = ToolStatus::Complete { summary };
                    return;
                }
            }
        }
    }

    pub fn error_tool(&mut self, name: &str, message: impl Into<String>) {
        for item in self.items.iter_mut().rev() {
            if let ConversationItem::ToolCall(tool) = item {
                if tool.name == name && matches!(tool.status, ToolStatus::Running) {
                    tool.status = ToolStatus::Error {
                        message: message.into(),
                    };
                    return;
                }
            }
        }
    }

    pub fn items(&self) -> &[ConversationItem] {
        &self.items
    }

    pub fn clear(&mut self) {
        self.items.clear();
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

fn render_user_message(content: &str, _width: usize) -> Vec<Line<'static>> {
    // User messages: inverted style with > prefix
    // Note: User input is typically short, no wrapping applied
    let mut lines = Vec::new();

    // Add blank line before user message for spacing
    lines.push(Line::from(""));

    for (i, line) in content.lines().enumerate() {
        let prefix = if i == 0 {
            format!(" {} ", indicators::USER_PREFIX)
        } else {
            "   ".to_string() // Continuation indent
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, presets::user_prefix()),
            Span::styled(format!("{} ", line), presets::user_message()),
        ]));
    }

    lines
}

/// Render assistant message blocks with streaming indicators
fn render_assistant_blocks(
    blocks: &[ContentBlock],
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
            ContentBlock::Prose { text, is_complete } => {
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
            ContentBlock::Code {
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
    // Use width for word-aware wrapping (0 = no wrap)
    let wrap_width = if width > 0 { Some(width) } else { None };
    let ansi_output = MARKDOWN_RENDERER.render_with_width(content, wrap_width);
    match ansi_output.into_text() {
        Ok(text) => text.lines,
        Err(_) => {
            // Fallback to plain text on ANSI parse error
            content.lines().map(|l| Line::from(l.to_string())).collect()
        }
    }
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
    let blocks = vec![ContentBlock::prose(content)];
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

fn render_tool_call(tool: &ToolCallDisplay) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Add blank line for spacing
    lines.push(Line::from(""));

    // Tool status line
    let (indicator, style) = match &tool.status {
        ToolStatus::Running => (indicators::SPINNER_FRAMES[0], presets::tool_running()),
        ToolStatus::Complete { .. } => (indicators::COMPLETE, presets::tool_complete()),
        ToolStatus::Error { .. } => (indicators::ERROR, presets::tool_error()),
    };

    let status_suffix = match &tool.status {
        ToolStatus::Running => String::new(),
        ToolStatus::Complete { summary } => summary
            .as_ref()
            .map(|s| format!(" → {}", s))
            .unwrap_or_default(),
        ToolStatus::Error { message } => format!(" → {}", message),
    };

    lines.push(Line::from(vec![Span::styled(
        format!("{} {}{}", indicator, tool.name, status_suffix),
        style,
    )]));

    // Tool output lines (indented)
    for line in &tool.output_lines {
        lines.push(Line::from(vec![Span::styled(
            format!("  {}", line),
            presets::tool_output(),
        )]));
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

        for item in self.state.items() {
            all_lines.extend(render_item_to_lines(item, width));
        }

        all_lines
    }
}

impl Widget for ConversationWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Content width minus prefix (" ● " = 3 chars) and right margin (1 char)
        let content_width = (area.width as usize).saturating_sub(4);
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
            // No Wrap needed - termimad pre-wraps at word boundaries
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

            // No Wrap needed - termimad pre-wraps at word boundaries
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
        let middle_row = area.y + area.height / 2;
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

        state.push_tool_running("grep");
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
        let blocks = vec![crate::tui::ContentBlock::prose("World")];
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
        let blocks = vec![crate::tui::ContentBlock::prose(
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
}
