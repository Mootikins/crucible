use std::path::PathBuf;

use crucible_core::interaction::{InteractionRequest, InteractionResponse};
use crucible_core::traits::chat::PrecognitionNoteInfo;

use super::{ChatItem, McpServerDisplay, PluginStatusEntry};

#[derive(Debug, Clone)]
pub enum ChatAppMsg {
    UserMessage(String),
    TextDelta(String),
    ThinkingDelta(String),
    ToolCall {
        name: String,
        args: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    ToolResultDelta {
        name: String,
        delta: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    ToolResultComplete {
        name: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    ToolResultError {
        name: String,
        error: String,
        /// LLM-assigned call ID for correlating results with the correct tool
        call_id: Option<String>,
    },
    StreamComplete,
    StreamCancelled,
    Error(String),
    Status(String),
    ModeChanged(String),
    ContextUsage {
        used: usize,
        total: usize,
    },
    ClearHistory,
    QueueMessage(String),
    SwitchModel(String),
    FetchModels,
    ModelsLoaded(Vec<String>),
    ModelsFetchFailed(String),
    McpStatusLoaded(Vec<McpServerDisplay>),
    PluginStatusLoaded(Vec<PluginStatusEntry>),
    SetThinkingBudget(i64),
    SetTemperature(f64),
    SetMaxTokens(Option<u32>),
    SubagentSpawned {
        id: String,
        prompt: String,
    },
    SubagentCompleted {
        id: String,
        summary: String,
    },
    SubagentFailed {
        id: String,
        error: String,
    },
    DelegationSpawned {
        id: String,
        prompt: String,
        target_agent: Option<String>,
    },
    DelegationCompleted {
        id: String,
        summary: String,
    },
    DelegationFailed {
        id: String,
        error: String,
    },
    ToggleMessages,
    OpenInteraction {
        request_id: String,
        request: InteractionRequest,
    },
    CloseInteraction {
        request_id: String,
        response: InteractionResponse,
    },
    LoadHistory(Vec<ChatItem>),
    ReloadPlugin(String),
    /// Forward an unrecognized slash command to the runner for registry-based execution
    ExecuteSlashCommand(String),
    /// Export session to markdown file via observe renderer
    ExportSession(PathBuf),
    PrecognitionResult {
        notes_count: usize,
        notes: Vec<PrecognitionNoteInfo>,
    },
    /// Internal: enriched message ready to send to agent (from background precognition)
    EnrichedMessage {
        original: String,
        enriched: String,
    },
}
