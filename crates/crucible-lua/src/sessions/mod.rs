//! Multi-session management API for Lua scripts
//!
//! Provides `cru.session.*` functions for managing daemon sessions from Lua
//! plugins, and the shared operation bodies (`_op` functions in `register`)
//! that the `Session` handle's methods also call. This module defines a
//! [`DaemonSessionApi`] trait that the daemon crate implements, avoiding a
//! circular dependency (crucible-lua cannot depend on crucible-daemon).
//!
//! `cru.sessions` (plural) is a deprecated alias forwarding to `cru.session`.
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
//! -- Create a new session (returns a handle)
//! local session, err = cru.session.create({ type = "chat" })
//! if session then
//!     print(session.id, session.state)
//! end
//!
//! -- List all sessions (handles again; `list` is the only plural verb)
//! local sessions, err = cru.session.list()
//! for _, s in ipairs(sessions) do
//!     print(s.id, s.session_type, s.state)
//! end
//!
//! -- Send a message: free function or handle method, same body
//! local response_id, err = cru.session.send_message(session.id, "Hello!")
//! local response_id, err = session:send_message("Hello!")
//!
//! -- The session this VM is executing for
//! local cur = cru.session.current()
//!
//! -- End a session
//! cru.session.end_session(session.id)
//! ```

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

// `pub(crate)` so `session_api` can reach the shared `_op` bodies the handle
// methods and the free functions both call.
pub(crate) mod register;

pub use register::{
    register_sessions_module, register_sessions_module_with_api,
    register_sessions_module_with_api_and_current,
};

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
/// as the second return value: `local result, err = cru.session.create(...)`.
pub trait DaemonSessionApi: Send + Sync + 'static {
    /// Create a new session.
    ///
    /// `params` is the caller's whole options table as JSON — the same shape
    /// the daemon's `session.create` RPC takes (`type`, `kilns`, `workspace`,
    /// `agent_card`, `isolation`, …). It is a `Value` rather than a typed
    /// struct because that request type lives in `crucible-daemon`, which
    /// depends on this crate and not the reverse; passing the object through
    /// means a plugin reaches the same fields an RPC caller does without this
    /// crate re-declaring any of them.
    ///
    /// Returns a JSON object with at least `{ id, session_type, state, kilns }`.
    /// An omitted `kilns` is resolved daemon-side, not here.
    fn create_session(
        &self,
        params: serde_json::Value,
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

    /// Set the session's mode by id (`normal`, `plan`, `auto`, or a
    /// Lua-declared one).
    ///
    /// The mode a session runs its turns in, not a property of one turn: it
    /// persists on the session's agent, so it applies to a turn a plugin
    /// sends later and survives a handle eviction. A plugin needs it because
    /// a session it creates starts in the default mode, which asks for
    /// permission, while a plugin turn has nobody to answer — an unattended
    /// pass therefore has every write denied until it says which stance it
    /// wants.
    ///
    /// It is a verb rather than a `session.mode = …` setter because a handle
    /// from `create` binds no [`crate::session_api::SessionConfigRpc`], so the
    /// assignment would answer "Session not connected" on exactly the handle
    /// a plugin has.
    ///
    /// An id the session does not offer is an error naming the ids it does.
    fn set_mode(
        &self,
        session_id: String,
        mode_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Set the session's title, the name a human reads in the sessions list.
    ///
    /// A plugin session is one nobody typed into, so the daemon's own
    /// titling — which reads the first user message — leaves it "Untitled".
    /// The plugin is the only caller that knows what its pass was about.
    fn set_title(
        &self,
        session_id: String,
        title: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// End a session permanently.
    fn end_session(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Ask the attached client something, and wait for the answer.
    ///
    /// `request` is a serialized `InteractionRequest` — internally tagged on
    /// `kind`, the same wire shape `interaction_respond` accepts back. It is a
    /// `Value` rather than a typed struct for the reason `create_session`'s
    /// params are: the enum lives in `crucible-core`, and re-declaring seven
    /// variants here to pass them straight through would be a second
    /// definition to keep in step.
    ///
    /// Resolves to a serialized `InteractionResponse`. `{"kind":"cancelled"}`
    /// means nobody answered — no client attached, the user dismissed it, or
    /// `timeout_secs` elapsed.
    fn request_interaction(
        &self,
        session_id: String,
        request: serde_json::Value,
        timeout_secs: u64,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

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
    /// the session event log. `role_filter` restricts to a single text role
    /// (e.g. `"user"`). `limit` returns only the last N messages.
    /// `include_tools` adds `tool_call` and `tool_result` rows
    /// (`{ role, id, name, args }` and `{ role, id, content, truncated, error? }`).
    /// A `tool_result` row carries `error` only when the tool failed.
    fn load_messages(
        &self,
        session_id: String,
        role_filter: Option<String>,
        limit: Option<usize>,
        include_tools: bool,
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

    /// Return current context usage for a session.
    ///
    /// JSON shape:
    /// `{ messages: u32, prompt_tokens: u32, budget: u32, percent: f64 }`
    fn context_usage(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

    /// Trigger compaction on a session.
    ///
    /// Returns `()`; compaction runs asynchronously on the next agent turn.
    /// Wraps `SessionManager::request_compaction`.
    fn compact(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Remove messages from a session's conversation tree by range.
    ///
    /// `range` is `{ "type": "all" }` | `{ "type": "last" | "first", "n": N }` |
    /// `{ "type": "indices", "start": S, "end": E }` (half-open `[S, E)`).
    /// Returns the count of messages actually removed.
    fn remove_messages(
        &self,
        session_id: String,
        range: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>>;

    /// Run one completion against the session's own client and answer with
    /// its text.
    ///
    /// `params` is the caller's options table as JSON: `prompt` (required),
    /// `system` and `timeout` (seconds). One exchange, no tools, no history,
    /// nothing written back to the session — the primitive a plugin needs to
    /// ask the model a small question ABOUT a session rather than take a turn
    /// in it. `runtime/plugins/auto-title` is the worked example.
    fn complete(
        &self,
        session_id: String,
        params: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

    /// Undo the last `count` agent turns by rewinding the session's
    /// conversation tree cursor. Returns the number of turns actually
    /// undone (capped at available turns).
    fn undo(
        &self,
        session_id: String,
        count: usize,
    ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>>;

    /// Whether the session has at least one turn that can be undone.
    fn can_undo(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<bool, String>> + Send>>;

    /// Number of turns currently available for undo.
    fn undo_depth(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<usize, String>> + Send>>;

    /// Per-turn summaries of every turn currently undoable, oldest-to-
    /// newest. Each entry serialises to (at minimum) `{ messages_removed }`.
    fn undo_history(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>;

    // ── Attributed-diff review ──────────────────────────────────────────
    //
    // These take a `session_id` like everything else here, and that is the
    // whole point: a delegating agent reviews the session it delegated to,
    // not itself. An RPC-only review surface could not express that from
    // inside a plugin tool.

    /// The session's composed diff: one JSON object per hunk, shaped like
    /// `ComposedHunk`. A session that never ran a turn has an empty queue,
    /// not an error.
    fn review_list_hunks(
        &self,
        session_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<serde_json::Value>, String>> + Send>>;

    /// Record a decision about one hunk. `state` is `"unreviewed"`,
    /// `"accepted"` or `"rejected"`; rejecting reverts the hunk on disk and
    /// tells the session's agent it was rejected.
    fn review_set_state(
        &self,
        session_id: String,
        hunk_id: String,
        state: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Anchor a comment to a line range. `spec` carries
    /// `{ path, line_start, line_end?, body, root?, author? }`; the stored
    /// comment (including its minted id) comes back.
    fn review_comment(
        &self,
        session_id: String,
        spec: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send>>;

    /// Mark a comment answered.
    fn review_resolve_comment(
        &self,
        session_id: String,
        comment_id: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

    /// Send a message and stream structured response parts.
    ///
    /// Subscribes, sends the message, then returns a receiver that yields
    /// [`ResponsePart`]s as they become available. Text deltas are accumulated
    /// and flushed as a single `Text` part at each boundary (tool call, tool
    /// result, thinking, or completion). `timeout_secs` defaults to 120.
    /// `max_tool_result_len` caps tool-result previews (default 500).
    /// `interactive` lets a plugin assert that this turn has exactly one
    /// identified principal who may answer a permission prompt — a DM from an
    /// account the operator named, and nothing looser. It defaults to `false`
    /// and the daemon cannot infer it: only the plugin knows whether the
    /// channel it is serving has one person in it.
    fn send_and_collect(
        &self,
        session_id: String,
        content: String,
        timeout_secs: Option<f64>,
        max_tool_result_len: Option<usize>,
        interactive: bool,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>, String>>
                + Send,
        >,
    >;
}
