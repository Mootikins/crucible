//! Multi-session management API for Lua scripts
//!
//! Provides `cru.sessions.*` functions for managing daemon sessions from Lua plugins.
//! This module defines a [`DaemonSessionApi`] trait that the daemon crate implements,
//! avoiding a circular dependency (crucible-lua cannot depend on crucible-daemon).
//!
//! ## Architecture
//!
//! ```text
//! crucible-lua (this crate)         crucible-daemon
//! ┌──────────────────────┐          ┌──────────────────────┐
//! │ DaemonSessionApi     │◄─────────│ impl DaemonSessionApi│
//! │   (trait)            │          │  using SessionManager│
//! │                      │          │  AgentManager        │
//! │ register_sessions_*  │          │  broadcast::Sender   │
//! │   (module setup)     │          └──────────────────────┘
//! └──────────────────────┘
//! ```
//!
//! ## Usage in Lua
//!
//! ```lua
//! -- Create a new session
//! local session, err = cru.sessions.create("chat", "/path/to/kiln")
//! if session then
//!     print(session.id, session.state)
//! end
//!
//! -- List all sessions
//! local sessions, err = cru.sessions.list()
//! for _, s in ipairs(sessions) do
//!     print(s.id, s.session_type, s.state)
//! end
//!
//! -- Send a message to a session
//! local response_id, err = cru.sessions.send_message("chat-2025-...", "Hello!")
//!
//! -- Subscribe to events
//! local next_event, err = cru.sessions.subscribe("chat-2025-...")
//! if next_event then
//!     local event = next_event()  -- blocks until next event
//!     print(event.type, event.data)
//! end
//!
//! -- End a session
//! cru.sessions.end_session("chat-2025-...")
//! ```

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

mod register;

pub use register::{register_sessions_module, register_sessions_module_with_api};

#[cfg(test)]
mod tests;

/// A structured part of an agent response.
///
/// `send_and_collect` returns a `Vec<ResponsePart>` so callers (e.g. the Discord
/// plugin) can render each segment independently — sending tool calls as separate
/// messages, folding thinking blocks, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponsePart {
    /// Prose / markdown text from the LLM.
    Text { content: String },
    /// The LLM requested a tool invocation.
    ToolCall {
        tool: String,
        /// Truncated JSON preview of the arguments.
        args_brief: String,
    },
    /// A tool finished executing.
    ToolResult {
        tool: String,
        /// Truncated preview of the result.
        result_brief: String,
        is_error: bool,
    },
    /// Chain-of-thought / thinking block.
    Thinking { content: String },
    /// The agent needs permission to proceed (e.g. run a command).
    PermissionRequest {
        request_id: String,
        tool: String,
        description: String,
    },
}

/// Trait abstracting daemon session operations for Lua plugins.
///
/// The daemon crate implements this using its `SessionManager`, `AgentManager`,
/// and `broadcast::Sender<SessionEventMessage>`. All methods use `serde_json::Value`
/// as the interchange format to avoid coupling to concrete daemon types.
///
/// # Error Convention
///
/// Methods return `Result<T, String>` where the error string is surfaced to Lua
/// as the second return value: `local result, err = cru.sessions.create(...)`.
pub trait DaemonSessionApi: Send + Sync + 'static {
    /// Create a new session.
    ///
    /// Returns a JSON object with at least `{ id, session_type, state, kiln, workspace }`.
    /// `kiln` defaults to `crucible_home()` when None.
    /// `connected_kilns` are additional kilns the session can query for knowledge.
    fn create_session(
        &self,
        session_type: String,
        kiln: Option<String>,
        workspace: Option<String>,
        connected_kilns: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

    /// Get a session by ID.
    ///
    /// Returns `Ok(None)` if the session doesn't exist.
    fn get_session(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<serde_json::Value>, String>> + Send>>;

    /// List all sessions.
    ///
    /// Returns an array of session summary objects.
    fn list_sessions(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>;

    /// Configure the agent for a session.
    ///
    /// `agent_config` is a JSON object matching `SessionAgent` fields.
    fn configure_agent(
        &self,
        session_id: String,
        agent_config: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Send a user message to a session, triggering agent processing.
    ///
    /// Returns a request/response ID for tracking.
    fn send_message(
        &self,
        session_id: String,
        content: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

    /// Cancel the current operation in a session.
    ///
    /// Returns `true` if something was cancelled.
    fn cancel(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>>;

    /// Pause a session.
    fn pause(&self, session_id: String)
        -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Resume a paused session.
    fn resume(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// End a session permanently.
    fn end_session(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Respond to a permission/interaction request.
    fn respond_to_permission(
        &self,
        session_id: String,
        request_id: String,
        response: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Subscribe to session events.
    ///
    /// Returns a receiver that yields JSON event objects. Each call to `recv()`
    /// returns the next event or `None` if the subscription ended.
    fn subscribe(
        &self,
        session_id: String,
    ) -> Pin<
        Box<
            dyn Future<
                    Output = Result<
                        tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
                        String,
                    >,
                > + Send,
        >,
    >;

    /// Unsubscribe from session events.
    fn unsubscribe(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Load conversation messages for a session.
    ///
    /// Returns an array of `{ role, content, timestamp }` objects filtered from
    /// the session event log. Only User, Assistant, and System events are included.
    /// `role_filter` restricts to a single role (e.g. `"user"`).
    /// `limit` returns only the last N messages.
    fn load_messages(
        &self,
        session_id: String,
        role_filter: Option<String>,
        limit: Option<usize>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>;

    /// Inject a message into the session context without triggering LLM completion.
    ///
    /// Persists a `LogEvent` to the session's JSONL log and emits a broadcast event.
    /// `role` must be `"system"`, `"user"`, or `"assistant"`.
    fn inject_context(
        &self,
        session_id: String,
        role: String,
        content: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Wait for multiple background subagent jobs to complete.
    ///
    /// Returns one result object per job ID with `id`, `status`, and
    /// `output`/`error`/`exit_code` fields. `timeout_secs` defaults to 120.
    fn collect_subagents(
        &self,
        job_ids: Vec<String>,
        timeout_secs: Option<f64>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>;

    /// Fork a session, creating a new session with copied message history.
    ///
    /// Returns a JSON object with `{ id, parent_id, messages_copied }`.
    /// `up_to` limits copying to the first N user/assistant/system messages.
    fn fork_session(
        &self,
        session_id: String,
        up_to: Option<u64>,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

    /// Fetch the prompt-cache aggregate for a session.
    ///
    /// Returns a JSON object with hits/misses/{read,creation,prompt,completion}_tokens
    /// and `hit_rate` (null until the first cache event has fired).
    fn cache_stats(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

    /// Send a message and stream structured response parts.
    ///
    /// Subscribes, sends the message, then returns a receiver that yields
    /// [`ResponsePart`]s as they become available. Text deltas are accumulated
    /// and flushed as a single `Text` part at each boundary (tool call, tool
    /// result, thinking, or completion). `timeout_secs` defaults to 120.
    /// `max_tool_result_len` caps tool-result previews (default 500).
    fn send_and_collect(
        &self,
        session_id: String,
        content: String,
        timeout_secs: Option<f64>,
        max_tool_result_len: Option<usize>,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>>
                + Send,
        >,
    >;
}
