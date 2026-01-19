# Cross-Platform UI Abstractions for Chat/Agent Interface

**Date:** 2025-12-23
**Author:** Claude (Sonnet 4.5)
**Status:** Research/Design

## Overview

This document sketches lightweight platform-agnostic UI abstractions for Crucible's chat/agent interface that can be shared across:

- **Terminal** (ratatui/crossterm in Rust)
- **Web** (Svelte frontend + SSE backend)
- **Desktop** (future: Tauri with web frontend, or native like GPUI)
- **Mobile** (future: React Native or Flutter)

The goal is to define **data structures and behaviors** that are platform-independent, not rendering implementations. Platform renderers consume these view models and translate them to native UI components.

## Current State Analysis

### Terminal (TUI) Implementation

Located in `crates/crucible-cli/src/tui/`:

- **conversation.rs**: Defines `ConversationItem` enum (UserMessage, AssistantMessage, Status, ToolCall)
- **conversation_view.rs**: Implements `ConversationView` trait and `RatatuiView`
- **State management**: `ConversationState` holds Vec of items
- **Rendering**: Platform-specific widgets (ConversationWidget, InputBoxWidget, StatusBarWidget)

**Key Types:**
```rust
pub enum ConversationItem {
    UserMessage { content: String },
    AssistantMessage { content: String },
    Status(StatusKind),
    ToolCall(ToolCallDisplay),
}

pub struct ConversationState {
    items: Vec<ConversationItem>,
    max_tool_output_lines: usize,
}
```

### Web Implementation

Located in `crates/crucible-web/`:

- **events.rs**: Defines `ChatEvent` enum (Token, ToolCall, ToolResult, Thinking, MessageComplete, Error)
- **Chat.svelte**: TypeScript types and rendering logic
- **Transport**: SSE (Server-Sent Events) for streaming

**Key Types:**
```rust
pub enum ChatEvent {
    Token { content: String },
    ToolCall { id: String, title: String, arguments: Option<Value> },
    ToolResult { id: String, result: Option<String> },
    Thinking { content: String },
    MessageComplete { id: String, content: String, tool_calls: Vec<ToolCallSummary> },
    Error { code: String, message: String },
}
```

### Divergence Points

1. **Event granularity**: Web uses fine-grained events (Token), TUI uses accumulated state (ConversationItem)
2. **Tool representation**: TUI has `ToolCallDisplay` with status; Web has separate ToolCall/ToolResult events
3. **Serialization**: Web types are Serde-serializable; TUI types are not
4. **Streaming model**: Web streams deltas; TUI updates state in-place

## Design Principles

1. **Separation of concerns**: View models (data) vs. renderers (platform code)
2. **Serialization-ready**: All core types should support Serde for network transport
3. **Streaming-first**: Support both delta updates (web) and full state (native)
4. **Mode-agnostic**: Works for Plan/Act/Auto modes without special casing
5. **Extensible**: Easy to add new message types (images, files, structured data)

## Proposed Architecture

### Layer 1: Core View Models (Platform-Agnostic)

These types live in `crucible-core/src/ui/` and are used by all platforms.

```rust
//! crucible-core/src/ui/view_models.rs
//!
//! Platform-agnostic UI view models for chat/agent interfaces.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// =============================================================================
// Message View Models
// =============================================================================

/// Unique identifier for a message
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Role of the message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// User input
    User,
    /// Agent/assistant response
    Assistant,
    /// System message (mode changes, status updates)
    System,
}

/// Content state for streaming support
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum ContentState {
    /// Content is still being generated
    Streaming {
        /// Partial content received so far
        partial: String,
        /// Number of tokens generated (if available)
        #[serde(skip_serializing_if = "Option::is_none")]
        token_count: Option<usize>,
    },
    /// Content generation is complete
    Complete {
        /// Final content
        content: String,
    },
    /// Content generation failed
    Error {
        /// Error message
        message: String,
        /// Error code (if available)
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
}

impl ContentState {
    /// Get the text content regardless of state
    pub fn text(&self) -> &str {
        match self {
            ContentState::Streaming { partial, .. } => partial,
            ContentState::Complete { content } => content,
            ContentState::Error { message, .. } => message,
        }
    }

    /// Check if content is still streaming
    pub fn is_streaming(&self) -> bool {
        matches!(self, ContentState::Streaming { .. })
    }

    /// Check if content completed successfully
    pub fn is_complete(&self) -> bool {
        matches!(self, ContentState::Complete { .. })
    }

    /// Check if content is in error state
    pub fn is_error(&self) -> bool {
        matches!(self, ContentState::Error { .. })
    }

    /// Append text to streaming content
    pub fn append(&mut self, text: &str) {
        if let ContentState::Streaming { partial, .. } = self {
            partial.push_str(text);
        }
    }

    /// Mark as complete with final content
    pub fn complete(self) -> Self {
        match self {
            ContentState::Streaming { partial, .. } => ContentState::Complete { content: partial },
            other => other,
        }
    }
}

/// A message in the conversation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageViewModel {
    /// Unique message identifier
    pub id: MessageId,
    /// Message sender role
    pub role: MessageRole,
    /// Message content and state
    pub content: ContentState,
    /// When the message was created
    pub timestamp: DateTime<Utc>,
    /// Attached tool calls (if any)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCallViewModel>,
    /// Optional metadata (model used, mode, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

impl MessageViewModel {
    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: MessageId::new(),
            role: MessageRole::User,
            content: ContentState::Complete {
                content: content.into(),
            },
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
            metadata: None,
        }
    }

    /// Create a new assistant message (initially streaming)
    pub fn assistant_streaming() -> Self {
        Self {
            id: MessageId::new(),
            role: MessageRole::Assistant,
            content: ContentState::Streaming {
                partial: String::new(),
                token_count: None,
            },
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
            metadata: None,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: MessageId::new(),
            role: MessageRole::System,
            content: ContentState::Complete {
                content: content.into(),
            },
            timestamp: Utc::now(),
            tool_calls: Vec::new(),
            metadata: None,
        }
    }
}

/// Optional message metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Model that generated this message (e.g., "claude-opus-4")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Mode when message was sent (e.g., "plan", "act")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Additional custom fields
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

// =============================================================================
// Tool Call View Models
// =============================================================================

/// Unique identifier for a tool call
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ToolCallId(pub String);

impl ToolCallId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

/// Execution status of a tool call
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ToolStatus {
    /// Tool is currently running
    Running {
        /// Optional progress indicator (0.0 to 1.0)
        #[serde(skip_serializing_if = "Option::is_none")]
        progress: Option<f32>,
    },
    /// Tool completed successfully
    Complete {
        /// Optional summary/result message
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    /// Tool execution failed
    Error {
        /// Error message
        message: String,
        /// Error code (if available)
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
}

impl ToolStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, ToolStatus::Running { .. })
    }

    pub fn is_complete(&self) -> bool {
        matches!(self, ToolStatus::Complete { .. })
    }

    pub fn is_error(&self) -> bool {
        matches!(self, ToolStatus::Error { .. })
    }
}

/// Output from a tool (streaming or complete)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolOutput {
    /// Output lines (most recent N lines for streaming tools)
    pub lines: Vec<String>,
    /// Whether this is a partial output (more coming)
    pub is_partial: bool,
}

impl ToolOutput {
    pub fn empty() -> Self {
        Self {
            lines: Vec::new(),
            is_partial: false,
        }
    }

    pub fn from_text(text: &str) -> Self {
        Self {
            lines: text.lines().map(|s| s.to_string()).collect(),
            is_partial: false,
        }
    }

    /// Append new output (for streaming)
    pub fn append(&mut self, text: &str) {
        self.lines.extend(text.lines().map(|s| s.to_string()));
    }

    /// Truncate to last N lines (for display limits)
    pub fn truncate_to(&mut self, max_lines: usize) {
        if self.lines.len() > max_lines {
            let start = self.lines.len() - max_lines;
            self.lines.drain(0..start);
        }
    }
}

/// View model for a tool call
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallViewModel {
    /// Unique tool call identifier
    pub id: ToolCallId,
    /// Tool name (e.g., "Read", "Bash", "Grep")
    pub name: String,
    /// Execution status
    pub status: ToolStatus,
    /// Tool output (if any)
    #[serde(skip_serializing_if = "ToolOutput::is_empty", default)]
    pub output: ToolOutput,
    /// Tool arguments (optional, for debugging/display)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    /// When the tool started
    pub started_at: DateTime<Utc>,
    /// When the tool completed (if finished)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

impl ToolCallViewModel {
    /// Create a new running tool call
    pub fn running(name: impl Into<String>) -> Self {
        Self {
            id: ToolCallId::new(),
            name: name.into(),
            status: ToolStatus::Running { progress: None },
            output: ToolOutput::empty(),
            arguments: None,
            started_at: Utc::now(),
            completed_at: None,
        }
    }

    /// Mark the tool as complete
    pub fn complete(&mut self, summary: Option<String>) {
        self.status = ToolStatus::Complete { summary };
        self.completed_at = Some(Utc::now());
    }

    /// Mark the tool as errored
    pub fn error(&mut self, message: impl Into<String>) {
        self.status = ToolStatus::Error {
            message: message.into(),
            code: None,
        };
        self.completed_at = Some(Utc::now());
    }
}

impl ToolOutput {
    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

// =============================================================================
// Status Indicators
// =============================================================================

/// Status indicator for agent state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum StatusIndicator {
    /// Agent is thinking/reasoning (no output yet)
    Thinking {
        /// Optional thinking content (if visible)
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
    },
    /// Agent is generating text
    Generating {
        /// Number of tokens generated
        #[serde(skip_serializing_if = "Option::is_none")]
        token_count: Option<usize>,
    },
    /// Generic processing status
    Processing {
        /// Status message
        message: String,
    },
    /// No current status
    Idle,
}

// =============================================================================
// Input State
// =============================================================================

/// State of the input area
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InputState {
    /// Current input buffer content
    pub content: String,
    /// Cursor position (byte offset)
    pub cursor_position: usize,
    /// Whether input is enabled
    pub enabled: bool,
    /// Placeholder text (when empty)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor_position: 0,
            enabled: true,
            placeholder: None,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }
}

// =============================================================================
// Autocomplete/Popup
// =============================================================================

/// Type of autocomplete item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletionItemKind {
    Command,
    Agent,
    File,
    Note,
    Skill,
}

/// An autocomplete/popup item
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionItem {
    /// Item kind
    pub kind: CompletionItemKind,
    /// Main label (displayed prominently)
    pub label: String,
    /// Secondary text (description, path, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Text to insert when selected
    pub insert_text: String,
    /// Whether item is currently available
    pub available: bool,
    /// Fuzzy match score (for sorting)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<i32>,
}

/// State of the autocomplete popup
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionState {
    /// Items to display
    pub items: Vec<CompletionItem>,
    /// Currently selected index
    pub selected_index: usize,
    /// Query string (for filtering)
    pub query: String,
    /// Whether popup is visible
    pub visible: bool,
}

impl CompletionState {
    pub fn hidden() -> Self {
        Self {
            items: Vec::new(),
            selected_index: 0,
            query: String::new(),
            visible: false,
        }
    }

    pub fn show(&mut self, items: Vec<CompletionItem>, query: String) {
        self.items = items;
        self.query = query;
        self.selected_index = 0;
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.query.clear();
    }

    pub fn selected_item(&self) -> Option<&CompletionItem> {
        self.items.get(self.selected_index)
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.items.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.items.is_empty() {
            self.selected_index = if self.selected_index == 0 {
                self.items.len() - 1
            } else {
                self.selected_index - 1
            };
        }
    }
}

// =============================================================================
// Session State
// =============================================================================

/// Overall session state (mode, connection status, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    /// Current mode ID (e.g., "plan", "act", "auto")
    pub mode_id: String,
    /// Connection status
    pub connected: bool,
    /// Optional status text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    /// Total tokens in conversation (if tracked)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_count: Option<usize>,
}

impl SessionState {
    pub fn new(mode_id: impl Into<String>) -> Self {
        Self {
            mode_id: mode_id.into(),
            connected: false,
            status_text: None,
            token_count: None,
        }
    }
}

// =============================================================================
// Diff/Code Viewer
// =============================================================================

/// Type of diff change
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffChangeKind {
    /// Line was added
    Added,
    /// Line was removed
    Removed,
    /// Line was modified
    Modified,
    /// Line unchanged (context)
    Unchanged,
}

/// A line in a diff view
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffLine {
    /// Line number (before change, if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_line_num: Option<usize>,
    /// Line number (after change, if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line_num: Option<usize>,
    /// Type of change
    pub change: DiffChangeKind,
    /// Line content
    pub content: String,
}

/// View model for a code/diff viewer
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeViewerViewModel {
    /// Language for syntax highlighting (e.g., "rust", "python")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Lines to display
    pub lines: Vec<DiffLine>,
    /// Whether to show line numbers
    pub show_line_numbers: bool,
}

impl CodeViewerViewModel {
    /// Create a viewer for plain code (no diff)
    pub fn from_code(language: Option<String>, content: &str) -> Self {
        let lines = content
            .lines()
            .enumerate()
            .map(|(idx, line)| DiffLine {
                old_line_num: Some(idx + 1),
                new_line_num: Some(idx + 1),
                change: DiffChangeKind::Unchanged,
                content: line.to_string(),
            })
            .collect();

        Self {
            language,
            lines,
            show_line_numbers: true,
        }
    }
}
```

### Layer 2: Streaming Events (Transport)

For web and network transport, define delta events that update view models.

```rust
//! crucible-core/src/ui/events.rs
//!
//! Streaming events for updating UI view models over the network.

use super::view_models::*;
use serde::{Deserialize, Serialize};

/// UI update event (streamed to clients)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    /// New message was added
    MessageAdded {
        message: MessageViewModel,
    },
    /// Message content was updated (for streaming)
    MessageUpdated {
        id: MessageId,
        content: ContentState,
    },
    /// Tool call was added to a message
    ToolCallAdded {
        message_id: MessageId,
        tool_call: ToolCallViewModel,
    },
    /// Tool call status/output updated
    ToolCallUpdated {
        message_id: MessageId,
        tool_id: ToolCallId,
        status: ToolStatus,
        output: Option<ToolOutput>,
    },
    /// Status indicator changed
    StatusChanged {
        status: StatusIndicator,
    },
    /// Session state changed (mode, connection, etc.)
    SessionUpdated {
        session: SessionState,
    },
    /// An error occurred
    Error {
        code: String,
        message: String,
    },
}

impl UiEvent {
    /// Format as Server-Sent Event (SSE) string
    pub fn to_sse(&self) -> String {
        let event_type = match self {
            UiEvent::MessageAdded { .. } => "message_added",
            UiEvent::MessageUpdated { .. } => "message_updated",
            UiEvent::ToolCallAdded { .. } => "tool_call_added",
            UiEvent::ToolCallUpdated { .. } => "tool_call_updated",
            UiEvent::StatusChanged { .. } => "status_changed",
            UiEvent::SessionUpdated { .. } => "session_updated",
            UiEvent::Error { .. } => "error",
        };

        let data = serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string());

        format!("event: {}\ndata: {}\n\n", event_type, data)
    }
}
```

### Layer 3: Platform Renderers (Trait)

Define traits that each platform implements to render view models.

```rust
//! crucible-core/src/ui/renderer.rs
//!
//! Platform renderer traits.

use super::view_models::*;
use anyhow::Result;

/// Platform-agnostic renderer trait
///
/// Each platform (TUI, Web, Desktop) implements this to render view models
/// in their native UI framework.
pub trait UiRenderer {
    /// Render a message
    fn render_message(&mut self, msg: &MessageViewModel) -> Result<()>;

    /// Render a tool call (within a message)
    fn render_tool_call(&mut self, tool: &ToolCallViewModel) -> Result<()>;

    /// Render status indicator
    fn render_status(&mut self, status: &StatusIndicator) -> Result<()>;

    /// Render input area
    fn render_input(&mut self, input: &InputState) -> Result<()>;

    /// Render completion popup
    fn render_completion(&mut self, completion: &CompletionState) -> Result<()>;

    /// Render session state (status bar)
    fn render_session(&mut self, session: &SessionState) -> Result<()>;

    /// Render code/diff viewer
    fn render_code_viewer(&mut self, viewer: &CodeViewerViewModel) -> Result<()>;

    /// Full frame render (called once per frame)
    fn render_frame(&mut self) -> Result<()>;

    /// Handle terminal/window resize
    fn handle_resize(&mut self, width: u16, height: u16) -> Result<()>;
}

/// Conversation renderer (subset for minimal implementations)
///
/// Simplified interface for platforms that only need basic message rendering.
pub trait ConversationRenderer {
    /// Push a user message
    fn push_user_message(&mut self, content: &str) -> Result<()>;

    /// Push an assistant message
    fn push_assistant_message(&mut self, content: &str) -> Result<()>;

    /// Update streaming content
    fn update_streaming_content(&mut self, message_id: &MessageId, content: &str) -> Result<()>;

    /// Add a tool call
    fn push_tool_call(&mut self, message_id: &MessageId, tool: ToolCallViewModel) -> Result<()>;

    /// Update tool status
    fn update_tool_status(
        &mut self,
        message_id: &MessageId,
        tool_id: &ToolCallId,
        status: ToolStatus,
    ) -> Result<()>;

    /// Render the conversation
    fn render(&mut self) -> Result<()>;
}
```

### Layer 4: State Management

Centralized state that platforms can use or adapt.

```rust
//! crucible-core/src/ui/state.rs
//!
//! Centralized UI state container.

use super::view_models::*;
use std::collections::HashMap;

/// Centralized conversation state
///
/// Platforms can use this directly or adapt it to their needs.
pub struct ConversationState {
    /// All messages in order
    messages: Vec<MessageViewModel>,
    /// Message lookup by ID
    message_index: HashMap<MessageId, usize>,
    /// Current status indicator
    status: StatusIndicator,
    /// Input state
    input: InputState,
    /// Completion state
    completion: CompletionState,
    /// Session state
    session: SessionState,
}

impl ConversationState {
    pub fn new(initial_mode: impl Into<String>) -> Self {
        Self {
            messages: Vec::new(),
            message_index: HashMap::new(),
            status: StatusIndicator::Idle,
            input: InputState::new(),
            completion: CompletionState::hidden(),
            session: SessionState::new(initial_mode),
        }
    }

    // === Message Operations ===

    pub fn push_message(&mut self, message: MessageViewModel) {
        let id = message.id.clone();
        self.message_index.insert(id, self.messages.len());
        self.messages.push(message);
    }

    pub fn get_message(&self, id: &MessageId) -> Option<&MessageViewModel> {
        self.message_index
            .get(id)
            .and_then(|&idx| self.messages.get(idx))
    }

    pub fn get_message_mut(&mut self, id: &MessageId) -> Option<&mut MessageViewModel> {
        self.message_index
            .get(id)
            .and_then(|&idx| self.messages.get_mut(idx))
    }

    pub fn update_message_content(&mut self, id: &MessageId, content: ContentState) {
        if let Some(msg) = self.get_message_mut(id) {
            msg.content = content;
        }
    }

    pub fn add_tool_call(&mut self, message_id: &MessageId, tool: ToolCallViewModel) {
        if let Some(msg) = self.get_message_mut(message_id) {
            msg.tool_calls.push(tool);
        }
    }

    pub fn update_tool_status(
        &mut self,
        message_id: &MessageId,
        tool_id: &ToolCallId,
        status: ToolStatus,
    ) {
        if let Some(msg) = self.get_message_mut(message_id) {
            if let Some(tool) = msg.tool_calls.iter_mut().find(|t| &t.id == tool_id) {
                tool.status = status;
            }
        }
    }

    pub fn messages(&self) -> &[MessageViewModel] {
        &self.messages
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.message_index.clear();
    }

    // === Status Operations ===

    pub fn set_status(&mut self, status: StatusIndicator) {
        self.status = status;
    }

    pub fn status(&self) -> &StatusIndicator {
        &self.status
    }

    // === Input Operations ===

    pub fn input(&self) -> &InputState {
        &self.input
    }

    pub fn input_mut(&mut self) -> &mut InputState {
        &mut self.input
    }

    // === Completion Operations ===

    pub fn completion(&self) -> &CompletionState {
        &self.completion
    }

    pub fn completion_mut(&mut self) -> &mut CompletionState {
        &mut self.completion
    }

    // === Session Operations ===

    pub fn session(&self) -> &SessionState {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut SessionState {
        &mut self.session
    }
}
```

## Platform Implementations

### Terminal (Ratatui)

```rust
//! crucible-cli/src/tui/renderer.rs
//!
//! Ratatui implementation of UiRenderer.

use crucible_core::ui::{ConversationState, UiRenderer};
use ratatui::{Frame, backend::Backend};

pub struct RatatuiRenderer {
    state: ConversationState,
    // ... ratatui-specific fields
}

impl UiRenderer for RatatuiRenderer {
    fn render_message(&mut self, msg: &MessageViewModel) -> Result<()> {
        // Convert MessageViewModel to ratatui widgets
        // Use existing markdown rendering for assistant messages
        // ...
        Ok(())
    }

    fn render_frame(&mut self) -> Result<()> {
        // Full frame render with layout
        // ...
        Ok(())
    }

    // ... other methods
}
```

### Web (TypeScript)

```typescript
// web/src/lib/viewModels.ts
//
// TypeScript definitions matching Rust types (generated via typeshare or similar)

export interface MessageViewModel {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: ContentState;
  timestamp: string;
  toolCalls: ToolCallViewModel[];
  metadata?: MessageMetadata;
}

export type ContentState =
  | { state: 'streaming'; partial: string; tokenCount?: number }
  | { state: 'complete'; content: string }
  | { state: 'error'; message: string; code?: string };

// ... other types

// React/Svelte components consume these view models
export function MessageComponent({ message }: { message: MessageViewModel }) {
  // Render based on view model
}
```

### Desktop (Tauri + Web or Native)

For Tauri, use the same web frontend with Rust backend.

For native (e.g., GPUI):

```rust
//! crucible-desktop/src/renderer.rs
//!
//! GPUI implementation of UiRenderer.

use crucible_core::ui::{ConversationState, UiRenderer};
use gpui::*;

pub struct GpuiRenderer {
    state: ConversationState,
    // ... GPUI-specific fields
}

impl UiRenderer for GpuiRenderer {
    fn render_message(&mut self, msg: &MessageViewModel) -> Result<()> {
        // Convert to GPUI elements
        // ...
        Ok(())
    }

    // ... other methods
}
```

## Migration Path

### Phase 1: Define Core Types (crucible-core)

1. Create `crucible-core/src/ui/` module
2. Implement `view_models.rs`, `events.rs`, `renderer.rs`, `state.rs`
3. Add Serde derives and tests
4. Add `ui` feature flag to `crucible-core`

### Phase 2: Adapt Web Backend

1. Replace `crucible-web/src/events.rs` with `crucible_core::ui::events::UiEvent`
2. Update SSE endpoints to emit `UiEvent::to_sse()`
3. Update Svelte frontend to consume new event types
4. Preserve existing functionality while migrating types

### Phase 3: Adapt TUI

1. Convert `ConversationState` to use `crucible_core::ui::ConversationState`
2. Implement `UiRenderer` or `ConversationRenderer` trait for `RatatuiView`
3. Map existing `ConversationItem` to `MessageViewModel` in rendering layer
4. Preserve markdown rendering and scrollback behavior

### Phase 4: Add Desktop Support

1. Create `crucible-desktop` crate (when ready)
2. Implement `UiRenderer` for chosen framework (Tauri/GPUI)
3. Reuse all view models and state management
4. Platform-specific rendering only

## Open Questions

1. **Code viewer positioning**: Should code/diff viewers be embedded in messages or separate panels?
   - **Recommendation**: Embedded in messages for web/mobile; separate panel for desktop/TUI.

2. **Attachment support**: How to handle images, files, PDFs in messages?
   - **Recommendation**: Add `MessageAttachment` type with URL/data and MIME type.

3. **Offline support**: Should view models support offline caching?
   - **Recommendation**: Yes, for web/mobile. Add optional persistence layer.

4. **Real-time collaboration**: Multiple users viewing same session?
   - **Recommendation**: Out of scope for now. Design allows for it (message IDs are UUIDs).

5. **Custom renderers**: Allow plugins to register custom message renderers?
   - **Recommendation**: Yes. Add `MessageViewModel.render_hint` field for extensibility.

## Benefits

1. **Code reuse**: Core logic shared across all platforms
2. **Type safety**: Serde ensures web/Rust type compatibility
3. **Streaming-first**: Native support for SSE and real-time updates
4. **Testable**: View models are plain data structures
5. **Extensible**: Easy to add new message types, status indicators
6. **Platform-optimized**: Renderers can adapt to platform constraints (e.g., terminal width)

## References

- **Current TUI**: `crates/crucible-cli/src/tui/conversation.rs`
- **Current Web**: `crates/crucible-web/src/events.rs`, `crates/crucible-web/web/src/lib/Chat.svelte`
- **Input abstraction**: `crates/crucible-core/src/traits/input.rs`
- **ACP types**: `crates/crucible-core/src/types/acp/`

---

**Next Steps:**

1. Review this design with maintainers
2. Create `crucible-core/src/ui/` module structure
3. Implement Phase 1 (core types)
4. Write integration tests for type conversions
5. Begin Phase 2 (web migration) as proof of concept
