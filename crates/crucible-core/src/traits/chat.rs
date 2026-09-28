//! Chat framework abstraction traits
//!
//! Following SOLID principles, this module defines backend-agnostic chat abstractions.
//!
//! ## Architecture
//!
//! - **CommandHandler**: Trait for implementing slash commands
//! - **ChatContext**: Execution context for command handlers
//!
//! ## Mode Handling
//!
//! Modes are now handled via string IDs (e.g., "plan", "act", "auto") with
//! `SessionModeState` providing the list of available modes from the agent.
//!
//! ## Naming Convention
//!
//! - **AgentCard**: Static definition (prompt + metadata) - see `agent::types`
//! - **AgentHandle**: Runtime handle to an active agent, in the daemon
//!   (`crucible_daemon::agent_manager::AgentHandle`)
//!
//! ## Design Principles
//!
//! **Dependency Inversion**: Core defines interfaces, implementations live in CLI/agent crates
//! **Interface Segregation**: Separate traits for distinct capabilities
//! **Protocol Independence**: Abstracts over ACP, internal agents, direct LLM APIs

use serde::{Deserialize, Serialize};

/// Result type for chat operations
pub type ChatResult<T> = Result<T, ChatError>;

/// Chat operation errors
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum ChatError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Communication error: {0}")]
    Communication(String),

    #[error("Mode change error: {0}")]
    ModeChange(String),

    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Agent not available: {0}")]
    AgentUnavailable(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid mode: {0}")]
    InvalidMode(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),
}

/// Metadata about a note found during Precognition enrichment.
/// Carried through RPC so TUI/web can display which notes informed the response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrecognitionNoteInfo {
    pub title: String,
    /// Which kiln the note came from, by registry name.
    ///
    /// Was `kiln_label: Option<String>`, filled from the kiln directory's
    /// basename. This payload is persisted into `session.jsonl` and broadcast
    /// to the web and TUI, so that basename outlived the turn and reached two
    /// UIs. The key is renamed as well as retyped: a transcript recorded before
    /// this change holds a basename under the old key, and it must be dropped
    /// on read rather than parsed as if it were a name.
    #[serde(default)]
    pub kiln: Option<crate::config::KilnName>,
    /// Search relevance score from the vector index. Defaults for payloads
    /// recorded before the field existed.
    #[serde(default)]
    pub score: f64,
}

/// Result from a completed tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolResult {
    /// Tool name that completed
    pub name: String,
    /// Result content (may be truncated for display)
    pub result: String,
    /// Error message if tool failed
    pub error: Option<String>,
    /// LLM-assigned call ID for matching results to the correct tool call
    #[serde(default)]
    pub call_id: Option<String>,
    /// Tool signaled the agent loop should end after this batch.
    /// The loop only honors termination when *every* result in the batch
    /// sets this — one tool can't unilaterally cut another's work short.
    ///
    /// **Producer scope (v1):** today this is only set by Lua
    /// `pre_tool_call` handlers returning `{ handled = true,
    /// terminate = true }`. The native `ToolExecutor::execute_tool` trait
    /// returns `serde_json::Value` and has no way to signal terminate —
    /// non-Lua tools always send `terminate: false`.
    ///
    /// **Consumer scope (v1):** the conjunctive check fires at
    /// `TurnEvent::ToolBatchEnd`, which both agent paths now emit — the
    /// genai loop after every tool batch, the ACP delegation path
    /// (`crucible-daemon/src/acp_handle.rs`) once per turn after the last
    /// tool call is announced. On the ACP side that is only for a turn that
    /// actually announced a call: a text-only turn emits no batch-end (it
    /// would claim a batch that never existed), and a turn whose only tool
    /// evidence is a completion update for a call the agent never announced
    /// has nothing to close — the handle drops that result rather than
    /// naming a tool that was never introduced, so "a batch existed" and "a
    /// result was reported" cannot disagree.
    ///
    /// The flag still has no effect on `cru chat -a claude / opencode /
    /// gemini`, for a different reason: it is produced by the scheduler's
    /// tool-dispatch path, and an `owns_history` agent never takes it. Such
    /// an agent executes the tool in its own process and the scheduler only
    /// passes the call through (`agent_manager/messaging/stream.rs`), so no
    /// `ChatToolResult` — and therefore no `terminate` — is ever produced
    /// for it. Reaching a delegated agent's tools needs a signal on the ACP
    /// wire, not another event here.
    #[serde(default)]
    pub terminate: bool,
}

impl ChatToolResult {
    /// A failed call: no result text, the error message, and the call id
    /// the model assigned, so the model can match the failure to its request.
    pub fn error(
        name: impl Into<String>,
        call_id: impl Into<String>,
        msg: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            result: String::new(),
            error: Some(msg.into()),
            call_id: Some(call_id.into()),
            terminate: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub name: String,
    pub arguments: Option<serde_json::Value>,
    pub id: Option<String>,
}
