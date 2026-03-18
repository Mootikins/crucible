//! ChatAppMsg: Unified message type for bidirectional TUI ↔ daemon communication.
//!
//! ## Design Pattern: Dual-Duty Messages
//!
//! ChatAppMsg serves a dual purpose in the TUI event loop:
//!
//! 1. **Commands (TUI → daemon)**: User actions that trigger side effects
//!    - Examples: `UserMessage`, `SwitchModel`, `SetThinkingBudget`
//!    - Flow: User input → `process_action()` → RPC call to daemon
//!
//! 2. **Events (daemon → TUI)**: Responses from the daemon that update display state
//!    - Examples: `TextDelta`, `StreamComplete`, `ModelsLoaded`
//!    - Flow: Daemon event → `msg_tx` channel → `process_action()` → `app.on_message()`
//!
//! 3. **Dual-Duty**: Variants that serve both roles
//!    - Examples: `Error`, `Status`, `ContextUsage`
//!    - Flow: Can originate from user action OR daemon response
//!
//! ## The Bidirectional Flow
//!
//! The key insight is in `chat_runner.rs` line ~1334:
//!
//! ```ignore
//! // After process_action handles side effects (RPC calls, etc.),
//! // the SAME message flows through app.on_message() for state updates.
//! let action = params.app.on_message(msg);  // msg is the same ChatAppMsg
//! Box::pin(self.process_action(ProcessActionParams {
//!     action,  // Returned action from on_message()
//!     ...
//! }))
//! ```
//!
//! This pass-through pattern means:
//! - Many variants are processed TWICE: once for side effects, once for state updates
//! - The enum is NOT split into separate Command/Event types (would require explicit
//!   conversion after processing, plus 18 test file updates)
//! - Variants are grouped by domain (stream, config, delegation, ui) in `on_message()`
//!
//! ## Variant Classification
//!
//! Each variant is marked with one of:
//! - `/// **Command** (TUI → daemon)`: Outbound user action
//! - `/// **Event** (daemon → TUI)`: Inbound daemon response
//! - `/// **Dual-duty**: Both command and event
//!

use std::path::PathBuf;

use crucible_core::interaction::{InteractionRequest, InteractionResponse};
use crucible_core::traits::chat::PrecognitionNoteInfo;

use super::{ChatItem, McpServerDisplay, PluginStatusEntry};

#[derive(Debug, Clone)]
pub enum ChatAppMsg {
    // --- Outbound Commands (TUI → daemon) ---
    /// **Command** (TUI → daemon): User typed a message and pressed Enter.
    UserMessage(String),
    /// **Event** (daemon → TUI): Streaming text delta from LLM response.
    TextDelta(String),
    /// **Event** (daemon → TUI): Streaming thinking/reasoning delta from LLM.
    ThinkingDelta(String),
    /// **Event** (daemon → TUI): LLM initiated a tool call.
    ToolCall {
        name: String,
        args: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
        /// Human-readable description of what the tool does (from registry).
        description: Option<String>,
        /// Source provenance string (e.g. "Core", "Crucible", "Mcp:github").
        source: Option<String>,
    },
    /// **Event** (daemon → TUI): Streaming delta of tool result output.
    ToolResultDelta {
        name: String,
        delta: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    /// **Event** (daemon → TUI): Tool result streaming completed.
    ToolResultComplete {
        name: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    /// **Event** (daemon → TUI): Tool execution failed with error.
    ToolResultError {
        name: String,
        error: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    /// **Event** (daemon → TUI): LLM response streaming completed.
    StreamComplete,
    /// **Event** (daemon → TUI): LLM response streaming was cancelled by user.
    StreamCancelled,
    /// **Dual-duty**: Error message (from daemon or user action failure).
    Error(String),
    /// **Dual-duty**: Status message (from daemon or user action).
    Status(String),
    /// **Event** (daemon → TUI): User switched TUI mode (Normal/Plan/Auto).
    ModeChanged(String),
    /// **Event** (daemon → TUI): Context window usage updated.
    ContextUsage { used: usize, total: usize },
    /// **Command** (TUI → daemon): Clear chat history.
    ClearHistory,
    /// **Command** (TUI → daemon): Queue a message to send to agent.
    QueueMessage(String),
    /// **Command** (TUI → daemon): Switch to a different LLM model.
    SwitchModel(String),
    /// **Command** (TUI → daemon): Fetch available models from providers.
    FetchModels,
    /// **Event** (daemon → TUI): Models list loaded successfully.
    ModelsLoaded(Vec<String>),
    /// **Event** (daemon → TUI): Model fetch failed with error.
    ModelsFetchFailed(String),
    /// **Event** (daemon → TUI): MCP server status loaded.
    McpStatusLoaded(Vec<McpServerDisplay>),
    /// **Event** (daemon → TUI): Plugin status loaded.
    PluginStatusLoaded(Vec<PluginStatusEntry>),
    /// **Command** (TUI → daemon): Set LLM thinking budget (extended thinking).
    SetThinkingBudget(i64),
    /// **Command** (TUI → daemon): Set LLM temperature (sampling randomness).
    SetTemperature(f64),
    /// **Command** (TUI → daemon): Set maximum tokens for LLM response.
    SetMaxTokens(Option<u32>),
    // --- Delegation & Subagent Events (daemon → TUI) ---
    /// **Event** (daemon → TUI): Subagent spawned (background task started).
    SubagentSpawned { id: String, prompt: String },
    /// **Event** (daemon → TUI): Subagent completed successfully.
    SubagentCompleted { id: String, summary: String },
    /// **Event** (daemon → TUI): Subagent failed with error.
    SubagentFailed { id: String, error: String },
    /// **Event** (daemon → TUI): Delegation spawned (cross-agent task started).
    DelegationSpawned {
        id: String,
        prompt: String,
        target_agent: Option<String>,
    },
    /// **Event** (daemon → TUI): Delegation completed successfully.
    DelegationCompleted { id: String, summary: String },
    /// **Event** (daemon → TUI): Delegation failed with error.
    DelegationFailed { id: String, error: String },
    // --- UI State & Interaction Events ---
    /// **Command** (TUI → daemon): Toggle message visibility in chat.
    ToggleMessages,
    /// **Event** (daemon → TUI): Open interaction popup (user input required).
    OpenInteraction {
        request_id: String,
        request: InteractionRequest,
    },
    /// **Event** (daemon → TUI): Close interaction popup with user response.
    CloseInteraction {
        request_id: String,
        response: InteractionResponse,
    },
    /// **Event** (daemon → TUI): Load chat history (session resume or replay).
    LoadHistory(Vec<ChatItem>),
    /// **Command** (TUI → daemon): Reload a Lua/Fennel plugin.
    ReloadPlugin(String),
    /// **Command** (TUI → daemon): Execute a slash command (/:command args).
    ExecuteSlashCommand(String),
    /// **Command** (TUI → daemon): Export session to markdown file.
    ExportSession(PathBuf),
    /// **Event** (daemon → TUI): Precognition result (auto-injected context notes).
    PrecognitionResult {
        notes_count: usize,
        notes: Vec<PrecognitionNoteInfo>,
    },
    /// **Dual-duty**: Internal enriched message ready to send (from background precognition).
    EnrichedMessage { original: String, enriched: String },
}

/// Category of a `ChatAppMsg` for top-level dispatch.
///
/// Used by `on_message` to route to the correct handler without
/// enumerating every variant in a single match block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MsgCategory {
    /// User-submitted chat input (`UserMessage`).
    User,
    /// Streaming events from the LLM response.
    Stream,
    /// Model/provider configuration changes.
    Config,
    /// Subagent and delegation lifecycle events.
    Delegation,
    /// UI state, interaction modals, and miscellaneous events.
    Ui,
}

impl ChatAppMsg {
    /// Classify this message for top-level routing.
    pub(crate) fn category(&self) -> MsgCategory {
        match self {
            Self::UserMessage(_) => MsgCategory::User,

            Self::TextDelta(_)
            | Self::ThinkingDelta(_)
            | Self::ToolCall { .. }
            | Self::ToolResultDelta { .. }
            | Self::ToolResultComplete { .. }
            | Self::ToolResultError { .. }
            | Self::StreamComplete
            | Self::StreamCancelled => MsgCategory::Stream,

            Self::SwitchModel(_)
            | Self::FetchModels
            | Self::ModelsLoaded(_)
            | Self::ModelsFetchFailed(_)
            | Self::SetThinkingBudget(_)
            | Self::SetTemperature(_)
            | Self::SetMaxTokens(_)
            | Self::McpStatusLoaded(_)
            | Self::PluginStatusLoaded(_) => MsgCategory::Config,

            Self::SubagentSpawned { .. }
            | Self::SubagentCompleted { .. }
            | Self::SubagentFailed { .. }
            | Self::DelegationSpawned { .. }
            | Self::DelegationCompleted { .. }
            | Self::DelegationFailed { .. } => MsgCategory::Delegation,

            Self::QueueMessage(_)
            | Self::Error(_)
            | Self::Status(_)
            | Self::ModeChanged(_)
            | Self::ContextUsage { .. }
            | Self::ClearHistory
            | Self::ToggleMessages
            | Self::OpenInteraction { .. }
            | Self::CloseInteraction { .. }
            | Self::LoadHistory(_)
            | Self::PrecognitionResult { .. }
            | Self::EnrichedMessage { .. }
            | Self::ExecuteSlashCommand(_)
            | Self::ExportSession(_)
            | Self::ReloadPlugin(_) => MsgCategory::Ui,
        }
    }
}
