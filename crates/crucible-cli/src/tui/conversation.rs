//! Conversation view renderer
//!
//! Renders the chat conversation history with styled messages,
//! tool calls, and status indicators. Designed for ratatui rendering
//! with full viewport control.

use crate::tui::styles::{indicators, styles};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

// =============================================================================
// Conversation Types
// =============================================================================

/// A message in the conversation
#[derive(Debug, Clone)]
pub enum ConversationItem {
    /// User input message
    UserMessage { content: String },
    /// Assistant text response
    AssistantMessage { content: String },
    /// Status indicator (thinking, generating)
    Status(StatusKind),
    /// Tool call with status
    ToolCall(ToolCallDisplay),
}

/// Status indicator types
#[derive(Debug, Clone, PartialEq)]
pub enum StatusKind {
    /// Agent is thinking (no output yet)
    Thinking,
    /// Agent is generating tokens
    Generating { token_count: usize },
    /// Processing (generic)
    Processing { message: String },
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
        self.items.push(ConversationItem::AssistantMessage {
            content: content.into(),
        });
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
pub fn render_item_to_lines(item: &ConversationItem) -> Vec<Line<'static>> {
    match item {
        ConversationItem::UserMessage { content } => render_user_message(content),
        ConversationItem::AssistantMessage { content } => render_assistant_message(content),
        ConversationItem::Status(status) => render_status(status),
        ConversationItem::ToolCall(tool) => render_tool_call(tool),
    }
}

fn render_user_message(content: &str) -> Vec<Line<'static>> {
    // User messages: inverted style with > prefix
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
            Span::styled(prefix, styles::user_prefix()),
            Span::styled(format!("{} ", line), styles::user_message()),
        ]));
    }

    lines
}

fn render_assistant_message(content: &str) -> Vec<Line<'static>> {
    // Assistant messages: normal style, no prefix
    let mut lines = Vec::new();

    // Add blank line for spacing
    lines.push(Line::from(""));

    for line in content.lines() {
        lines.push(Line::from(Span::styled(
            line.to_string(),
            styles::assistant_message(),
        )));
    }

    lines
}

fn render_status(status: &StatusKind) -> Vec<Line<'static>> {
    let (indicator, text, style) = match status {
        StatusKind::Thinking => (
            indicators::THINKING,
            "Thinking...".to_string(),
            styles::thinking(),
        ),
        StatusKind::Generating { token_count } => {
            let text = if *token_count > 0 {
                format!("Generating... ({} tokens)", token_count)
            } else {
                "Generating...".to_string()
            };
            (indicators::STREAMING, text, styles::streaming())
        }
        StatusKind::Processing { message } => {
            (indicators::STREAMING, message.clone(), styles::streaming())
        }
    };

    vec![
        Line::from(""),
        Line::from(vec![Span::styled(format!("{} {}", indicator, text), style)]),
    ]
}

fn render_tool_call(tool: &ToolCallDisplay) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Add blank line for spacing
    lines.push(Line::from(""));

    // Tool status line
    let (indicator, style) = match &tool.status {
        ToolStatus::Running => (indicators::SPINNER_FRAMES[0], styles::tool_running()),
        ToolStatus::Complete { .. } => (indicators::COMPLETE, styles::tool_complete()),
        ToolStatus::Error { .. } => (indicators::ERROR, styles::tool_error()),
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
            styles::tool_output(),
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

    fn render_to_lines(&self) -> Vec<Line<'static>> {
        let mut all_lines = Vec::new();

        for item in self.state.items() {
            all_lines.extend(render_item_to_lines(item));
        }

        all_lines
    }
}

impl Widget for ConversationWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = self.render_to_lines();

        // Create paragraph with all content
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });

        paragraph.render(area, buf);
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
            styles::input_box()
        } else {
            styles::dim()
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
}

impl<'a> StatusBarWidget<'a> {
    pub fn new(mode_id: &'a str, status: &'a str) -> Self {
        Self {
            mode_id,
            token_count: None,
            status,
        }
    }

    pub fn token_count(mut self, count: usize) -> Self {
        self.token_count = Some(count);
        self
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode_style = styles::mode(self.mode_id);
        let mode_name = match self.mode_id {
            "plan" => "Plan",
            "act" => "Act",
            "auto" => "Auto",
            _ => self.mode_id,
        };

        let mut spans = vec![
            Span::styled(indicators::MODE_ARROW, styles::dim()),
            Span::raw(" "),
            Span::styled(mode_name, mode_style),
        ];

        if let Some(count) = self.token_count {
            spans.push(Span::styled(" │ ", styles::dim()));
            spans.push(Span::styled(format!("{} tokens", count), styles::metrics()));
        }

        spans.push(Span::styled(" │ ", styles::dim()));
        spans.push(Span::styled(self.status.to_string(), styles::dim()));

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(styles::status_line());
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_state_new() {
        let state = ConversationState::new();
        assert!(state.items().is_empty());
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
        state.set_status(StatusKind::Thinking);
        state.set_status(StatusKind::Generating { token_count: 50 });

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
        let lines = render_user_message("Hello world");
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
}
