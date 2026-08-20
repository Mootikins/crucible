//! Session log events for JSONL persistence
//!
//! These events are for session persistence/resume, separate from
//! `crucible_core::events::SessionEvent`, the canonical event type.

use chrono::{DateTime, Utc};
use crucible_core::protocol::SessionEventMessage;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Token usage on a persisted assistant turn.
///
/// Canonically owned by `crucible-core`; re-exported here so
/// `crucible_daemon::observe::events::TokenUsage` and
/// `crucible_daemon::TokenUsage` keep resolving. This module used to define
/// a second, two-field `{in,out}` copy — which meant every resumed turn
/// silently lost the cache-read and cache-creation accounting the wire form
/// already carried (`SessionEventMessage::message_complete`). CLAUDE.md: never duplicate types
/// between crates.
pub use crucible_core::traits::llm::TokenUsage;

/// What the gate did about one permission request, as recorded in the session
/// log.
///
/// The **outcome**, not the verdict.
/// [`crucible_core::config::components::permissions::PermissionDecision`] is
/// what the rule engine returns *before* anything happens, and it carries an
/// `Ask` that this type deliberately does not: by the time a line reaches the
/// log the prompt has resolved, so `Ask` is not a thing that can have happened.
///
/// `AutoAllow` runs the other way — it is a distinction only the log needs. The
/// gate allowed the call without prompting, and an auditor reading the
/// transcript wants to see which allows a human actually saw.
/// Permission decision for a tool call
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOutcome {
    /// User allowed the operation
    Allow,
    /// User denied the operation
    Deny,
    /// Operation was auto-approved (e.g., allowlisted)
    AutoAllow,
}

/// A single event in the session log
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LogEvent {
    /// Session initialization
    Init {
        ts: DateTime<Utc>,
        /// Session ID
        session_id: String,
        /// Working directory
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        /// Model being used
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },

    /// System message (prompt, context injection)
    System { ts: DateTime<Utc>, content: String },

    /// User message
    User { ts: DateTime<Utc>, content: String },

    /// Assistant response (final, not streaming chunks)
    Assistant {
        ts: DateTime<Utc>,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tokens: Option<TokenUsage>,
    },

    /// Assistant thinking/reasoning (if model supports it)
    Thinking { ts: DateTime<Utc>, content: String },

    /// Tool invocation
    ToolCall {
        ts: DateTime<Utc>,
        /// Correlation ID for matching with result
        id: String,
        name: String,
        /// Tool arguments as JSON
        args: Value,
    },

    /// Permission decision for a tool call
    Permission {
        ts: DateTime<Utc>,
        /// Correlation ID matching the ToolCall
        id: String,
        /// Tool name
        tool: String,
        /// The decision made
        decision: PermissionOutcome,
        /// Optional reason for the decision
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },

    /// Tool execution result
    ToolResult {
        ts: DateTime<Utc>,
        /// Correlation ID matching the ToolCall
        id: String,
        /// Result content (may be truncated for large outputs)
        result: String,
        /// Whether the result was truncated
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
        /// Original size in bytes (only set if truncated)
        #[serde(skip_serializing_if = "Option::is_none")]
        full_size: Option<usize>,
        /// Error message if tool failed
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    /// Context summary (for compaction/resume)
    Summary {
        ts: DateTime<Utc>,
        /// Summarized content replacing earlier messages
        content: String,
        /// Number of messages summarized
        #[serde(skip_serializing_if = "Option::is_none")]
        messages_summarized: Option<u32>,
    },

    /// Error during session
    Error {
        ts: DateTime<Utc>,
        message: String,
        /// Whether the error is recoverable
        #[serde(default)]
        recoverable: bool,
    },

    /// Background bash task spawned
    BashSpawned {
        ts: DateTime<Utc>,
        /// Task identifier
        id: String,
        /// Shell command being executed
        command: String,
    },

    /// Background bash task completed
    BashCompleted {
        ts: DateTime<Utc>,
        /// Task identifier
        id: String,
        /// Command output (stdout)
        output: String,
        /// Process exit code
        exit_code: i32,
    },

    /// Background bash task failed
    BashFailed {
        ts: DateTime<Utc>,
        /// Task identifier
        id: String,
        /// Error message
        error: String,
        /// Process exit code if available
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },

    /// Subagent spawned - links to subagent's own session file
    SubagentSpawned {
        ts: DateTime<Utc>,
        /// Task identifier (also the subagent session ID)
        id: String,
        /// Wikilink to subagent session (e.g., "[[.subagents/sub-20260124-1432-beef/session]]")
        session_link: String,
        /// Brief description/prompt summary for display
        description: String,
    },

    /// Subagent completed - summary only, full output in linked session
    SubagentCompleted {
        ts: DateTime<Utc>,
        /// Task identifier
        id: String,
        /// Wikilink to subagent session
        session_link: String,
        /// Brief summary of result (full output in subagent session)
        summary: String,
    },

    /// Subagent failed
    SubagentFailed {
        ts: DateTime<Utc>,
        /// Task identifier
        id: String,
        /// Wikilink to subagent session
        session_link: String,
        /// Error message
        error: String,
    },
}

impl LogEvent {
    /// Create a session init event
    pub fn init(session_id: impl Into<String>) -> Self {
        LogEvent::Init {
            ts: Utc::now(),
            session_id: session_id.into(),
            cwd: None,
            model: None,
        }
    }

    /// Create a session init event with details
    pub fn init_with_details(
        session_id: impl Into<String>,
        cwd: Option<String>,
        model: Option<String>,
    ) -> Self {
        LogEvent::Init {
            ts: Utc::now(),
            session_id: session_id.into(),
            cwd,
            model,
        }
    }

    /// Create a system event
    pub fn system(content: impl Into<String>) -> Self {
        LogEvent::System {
            ts: Utc::now(),
            content: content.into(),
        }
    }

    /// Create a user message event
    pub fn user(content: impl Into<String>) -> Self {
        LogEvent::User {
            ts: Utc::now(),
            content: content.into(),
        }
    }

    /// Create an assistant message event
    pub fn assistant(content: impl Into<String>) -> Self {
        LogEvent::Assistant {
            ts: Utc::now(),
            content: content.into(),
            model: None,
            tokens: None,
        }
    }

    /// Create an assistant message with model info
    pub fn assistant_with_model(
        content: impl Into<String>,
        model: impl Into<String>,
        tokens: Option<TokenUsage>,
    ) -> Self {
        LogEvent::Assistant {
            ts: Utc::now(),
            content: content.into(),
            model: Some(model.into()),
            tokens,
        }
    }

    /// Create a thinking/reasoning event
    pub fn thinking(content: impl Into<String>) -> Self {
        LogEvent::Thinking {
            ts: Utc::now(),
            content: content.into(),
        }
    }

    /// Create a tool call event
    pub fn tool_call(id: impl Into<String>, name: impl Into<String>, args: Value) -> Self {
        LogEvent::ToolCall {
            ts: Utc::now(),
            id: id.into(),
            name: name.into(),
            args,
        }
    }

    /// Create a tool result event (not truncated)
    pub fn tool_result(id: impl Into<String>, result: impl Into<String>) -> Self {
        LogEvent::ToolResult {
            ts: Utc::now(),
            id: id.into(),
            result: result.into(),
            truncated: false,
            full_size: None,
            error: None,
        }
    }

    /// Create a tool result event with truncation info
    pub fn tool_result_truncated(
        id: impl Into<String>,
        result: impl Into<String>,
        full_size: usize,
    ) -> Self {
        LogEvent::ToolResult {
            ts: Utc::now(),
            id: id.into(),
            result: result.into(),
            truncated: true,
            full_size: Some(full_size),
            error: None,
        }
    }

    /// Create a tool error event
    pub fn tool_error(id: impl Into<String>, error: impl Into<String>) -> Self {
        LogEvent::ToolResult {
            ts: Utc::now(),
            id: id.into(),
            result: String::new(),
            truncated: false,
            full_size: None,
            error: Some(error.into()),
        }
    }

    /// Create a permission event
    pub fn permission(
        id: impl Into<String>,
        tool: impl Into<String>,
        decision: PermissionOutcome,
    ) -> Self {
        LogEvent::Permission {
            ts: Utc::now(),
            id: id.into(),
            tool: tool.into(),
            decision,
            reason: None,
        }
    }

    /// Create a permission event with reason
    pub fn permission_with_reason(
        id: impl Into<String>,
        tool: impl Into<String>,
        decision: PermissionOutcome,
        reason: impl Into<String>,
    ) -> Self {
        LogEvent::Permission {
            ts: Utc::now(),
            id: id.into(),
            tool: tool.into(),
            decision,
            reason: Some(reason.into()),
        }
    }

    /// Create a context summary event
    pub fn summary(content: impl Into<String>) -> Self {
        LogEvent::Summary {
            ts: Utc::now(),
            content: content.into(),
            messages_summarized: None,
        }
    }

    /// Create a context summary event with message count
    pub fn summary_with_count(content: impl Into<String>, messages_summarized: u32) -> Self {
        LogEvent::Summary {
            ts: Utc::now(),
            content: content.into(),
            messages_summarized: Some(messages_summarized),
        }
    }

    /// Create an error event
    pub fn error(message: impl Into<String>, recoverable: bool) -> Self {
        LogEvent::Error {
            ts: Utc::now(),
            message: message.into(),
            recoverable,
        }
    }

    pub fn bash_spawned(id: impl Into<String>, command: impl Into<String>) -> Self {
        LogEvent::BashSpawned {
            ts: Utc::now(),
            id: id.into(),
            command: command.into(),
        }
    }

    pub fn bash_completed(
        id: impl Into<String>,
        output: impl Into<String>,
        exit_code: i32,
    ) -> Self {
        LogEvent::BashCompleted {
            ts: Utc::now(),
            id: id.into(),
            output: output.into(),
            exit_code,
        }
    }

    pub fn bash_failed(
        id: impl Into<String>,
        error: impl Into<String>,
        exit_code: Option<i32>,
    ) -> Self {
        LogEvent::BashFailed {
            ts: Utc::now(),
            id: id.into(),
            error: error.into(),
            exit_code,
        }
    }

    pub fn subagent_spawned(
        id: impl Into<String>,
        session_link: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        LogEvent::SubagentSpawned {
            ts: Utc::now(),
            id: id.into(),
            session_link: session_link.into(),
            description: description.into(),
        }
    }

    pub fn subagent_completed(
        id: impl Into<String>,
        session_link: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        LogEvent::SubagentCompleted {
            ts: Utc::now(),
            id: id.into(),
            session_link: session_link.into(),
            summary: summary.into(),
        }
    }

    pub fn subagent_failed(
        id: impl Into<String>,
        session_link: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        LogEvent::SubagentFailed {
            ts: Utc::now(),
            id: id.into(),
            session_link: session_link.into(),
            error: error.into(),
        }
    }

    /// Get the timestamp of this event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            LogEvent::Init { ts, .. }
            | LogEvent::System { ts, .. }
            | LogEvent::User { ts, .. }
            | LogEvent::Assistant { ts, .. }
            | LogEvent::Thinking { ts, .. }
            | LogEvent::ToolCall { ts, .. }
            | LogEvent::Permission { ts, .. }
            | LogEvent::ToolResult { ts, .. }
            | LogEvent::Summary { ts, .. }
            | LogEvent::Error { ts, .. }
            | LogEvent::BashSpawned { ts, .. }
            | LogEvent::BashCompleted { ts, .. }
            | LogEvent::BashFailed { ts, .. }
            | LogEvent::SubagentSpawned { ts, .. }
            | LogEvent::SubagentCompleted { ts, .. }
            | LogEvent::SubagentFailed { ts, .. } => *ts,
        }
    }

    /// Serialize to JSONL format (single line)
    pub fn to_jsonl(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parse from JSONL line
    pub fn from_jsonl(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line)
    }
}

/// One line of `session.jsonl`, in either shape the file can hold.
///
/// The log is mixed by construction and always has been. `persist_event`
/// (`server/core.rs`) appends a serialized [`SessionEventMessage`] —
/// `{"type":"event","event":"user_message","data":{…}}` — and that is the
/// overwhelming majority of every real file. `inject_context_impl`
/// (`server/session/messaging.rs`) and both fork handlers append
/// [`LogEvent`] instead — `{"type":"user","ts":…,"content":…}`. Neither
/// writer knows about the other, so a reader that understands only one
/// shape silently drops the rest of the file. That was the bug this type
/// exists to make unrepresentable.
#[derive(Debug, Clone)]
pub enum SessionLogLine {
    /// The daemon's broadcast event, as persisted.
    Wire(SessionEventMessage),
    /// The presentation-shaped event written by inject-context and fork.
    View(LogEvent),
}

/// Reads only the discriminator, so neither full parse is attempted twice.
#[derive(Deserialize)]
struct LineTypeProbe {
    #[serde(rename = "type")]
    kind: Option<String>,
}

impl SessionLogLine {
    /// Parse one JSONL line.
    ///
    /// Discriminated on `type` rather than `#[serde(untagged)]`: untagged
    /// reports "data did not match any variant" without saying which shape
    /// it tried, and it would run `LogEvent`'s 16-way tag against every
    /// wire line before failing. `"event"` is `SessionEventMessage`'s
    /// hardcoded `msg_type` (`SessionEventMessage::new`) and is not a `LogEvent` tag, so
    /// the split is unambiguous. `"replay_event"` (`replay.rs`) never
    /// reaches `session.jsonl`, but accept it so a hand-copied recording
    /// line does not read as a broken `LogEvent` tag.
    pub fn from_jsonl(line: &str) -> Result<Self, serde_json::Error> {
        let probe: LineTypeProbe = serde_json::from_str(line)?;
        match probe.kind.as_deref() {
            Some("event" | "replay_event") => serde_json::from_str(line).map(SessionLogLine::Wire),
            _ => serde_json::from_str(line).map(SessionLogLine::View),
        }
    }
}

/// Project a persisted [`SessionEventMessage`] onto the presentation type
/// every reader already matches on.
///
/// Only the events `should_persist` (`server/core.rs`) admits can reach
/// a session log, so only those are mapped. `None` means "carries no
/// conversation content" — an ordinary outcome, not a parse failure, so
/// callers must not warn on it.
pub fn wire_to_log_event(msg: &SessionEventMessage) -> Option<LogEvent> {
    // Events reaching the persist task via `emit_event` are stamped
    // (`event_emitter.rs`'s `stamp_event`); the ~15 direct `event_tx.send` sites are
    // not, so `timestamp` is genuinely absent on some real lines.
    // `Utc::now()` is the fail-safe fallback: `handle_session_cleanup`
    // (`server/observe.rs`) deletes sessions whose newest event predates
    // a cutoff, so a fabricated-recent stamp keeps a session, where the
    // epoch would delete one. Deriving the stamp from the session id — which
    // encodes date and time to the minute — was considered and rejected: it
    // is per-session, not per-event, and buys nothing `.max()` does not
    // already get from the stamped majority.
    let ts = msg.timestamp.unwrap_or_else(Utc::now);
    let data = &msg.data;
    let text = |key: &str| data.get(key).and_then(Value::as_str).map(str::to_string);

    match msg.event.as_str() {
        "user_message" => Some(LogEvent::User {
            ts,
            content: text("content")?,
        }),
        "thinking" => Some(LogEvent::Thinking {
            ts,
            content: text("content")?,
        }),
        "message_complete" => Some(LogEvent::Assistant {
            ts,
            content: text("full_response")?,
            // Not carried by the payload (`SessionEventMessage::message_complete` writes only
            // message_id, full_response and usage). `parse_session_log`
            // fills it from the session's last `model_switched`.
            model: None,
            tokens: wire_token_usage(data),
        }),
        "tool_call" => Some(LogEvent::ToolCall {
            ts,
            id: text("call_id")?,
            name: text("tool").unwrap_or_default(),
            args: data.get("args").cloned().unwrap_or(Value::Null),
        }),
        "tool_result" => Some(LogEvent::ToolResult {
            ts,
            id: text("call_id")?,
            // `data.result` is an arbitrary `Value` (`SessionEventMessage::tool_result`);
            // `LogEvent::ToolResult.result` is a String. Unwrap the common
            // string case rather than re-quoting it.
            result: match data.get("result") {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            },
            // Truncation is applied before the event is emitted, so the
            // wire form carries no marker to recover. Reporting `false`
            // is honest about what the log knows.
            truncated: false,
            full_size: None,
            error: text("error"),
        }),
        // Injected context is part of the turn's record: without it a
        // resumed transcript cannot say which notes grounded the answer.
        // `server/core.rs` persists it for exactly that reason.
        "precognition_complete" => Some(LogEvent::System {
            ts,
            content: format!(
                "Context injected: {} note(s) for \"{}\"",
                data.get("notes_count").and_then(Value::as_u64).unwrap_or(0),
                text("query_summary").unwrap_or_default(),
            ),
        }),
        // `segment_complete` is a prefix of the same turn's
        // `message_complete.full_response` — `segment_complete`'s own doc comment says so outright
        // ("`message_complete` still carries the WHOLE turn's accumulated
        // text"). Mapping both would print every turn twice.
        // `model_switched` is consumed by `parse_session_log` for the model
        // attribution, not rendered on its own. `ended` is lifecycle
        // bookkeeping with no conversation content.
        _ => None,
    }
}

/// The canonical [`TokenUsage`] out of a `message_complete` payload.
///
/// `SessionEventMessage::message_complete` flattens all
/// five canonical fields into `data`, so the wire form carries strictly more
/// accounting than the two-field `{in,out}` struct this replaces. The three
/// required fields are written together or not at all, so any one being
/// absent means the turn recorded no usage.
fn wire_token_usage(data: &Value) -> Option<TokenUsage> {
    let at = |key: &str| data.get(key).and_then(Value::as_u64).map(|n| n as u32);
    Some(TokenUsage {
        prompt_tokens: at("prompt_tokens")?,
        completion_tokens: at("completion_tokens")?,
        total_tokens: at("total_tokens")?,
        cache_read_tokens: at("cache_read_tokens"),
        cache_creation_tokens: at("cache_creation_tokens"),
    })
}

/// Parse a whole session log into presentation events.
///
/// Pure `&str -> Vec<LogEvent>` so the async file-backed reader
/// (`observe::load_events`) and the sync in-memory one
/// (`observe::rebuild::rebuild_tree_from_str`) share one parser instead of
/// the two divergent line loops they had — which is how only one of them
/// would have been fixed.
///
/// A line that parses as neither shape warns and is skipped, keeping a
/// partially-corrupt log recoverable. A line that parses but maps to no
/// presentation event is skipped **silently**: a new persisted event kind is
/// not corruption, and warning on it would put a line in the log for every
/// `segment_complete` of every turn.
pub fn parse_session_log(jsonl: &str) -> Vec<LogEvent> {
    let mut events = Vec::new();
    // The model a turn ran under is announced once, by `model_switched`,
    // and not repeated on each `message_complete`. Carry it forward so
    // `## Assistant (model)` in the markdown renderer and `[assistant
    // (model)]` in `cru session show` say something true. Turns before the
    // first switch keep `None`: a session's *starting* model is never
    // emitted as an event, and inventing one would be worse than omitting it.
    let mut current_model: Option<String> = None;

    for (line_no, line) in jsonl.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match SessionLogLine::from_jsonl(trimmed) {
            Ok(SessionLogLine::Wire(msg)) => {
                if msg.event == "model_switched" {
                    current_model = msg
                        .data
                        .get("model_id")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                if let Some(mut event) = wire_to_log_event(&msg) {
                    if let LogEvent::Assistant { model, .. } = &mut event {
                        *model = current_model.clone();
                    }
                    events.push(event);
                }
            }
            Ok(SessionLogLine::View(event)) => events.push(event),
            Err(e) => {
                tracing::warn!(
                    line = line_no + 1,
                    error = %e,
                    "skipping unparseable session log line"
                );
            }
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_system_event_json() {
        let event = LogEvent::system("You are a helpful assistant");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"system\""));
        assert!(json.contains("\"content\":\"You are a helpful assistant\""));
        assert!(json.contains("\"ts\":"));

        // Round-trip
        let parsed = LogEvent::from_jsonl(&json).unwrap();
        if let LogEvent::System { content, .. } = parsed {
            assert_eq!(content, "You are a helpful assistant");
        } else {
            panic!("wrong event type");
        }
    }

    #[test]
    fn test_user_event_json() {
        let event = LogEvent::user("Hello");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"user\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn assistant_event_json_carries_canonical_token_fields() {
        let event = LogEvent::assistant_with_model(
            "Hi there!",
            "claude-3-haiku",
            Some(TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
                cache_read_tokens: None,
                cache_creation_tokens: None,
            }),
        );
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"assistant\""));
        assert!(json.contains("\"model\":\"claude-3-haiku\""));
        assert!(json.contains("\"prompt_tokens\":10"));
        assert!(json.contains("\"completion_tokens\":5"));
    }

    #[test]
    fn test_assistant_minimal_json() {
        let event = LogEvent::assistant("Hi!");
        let json = event.to_jsonl().unwrap();

        // Should NOT contain model or tokens when None
        assert!(!json.contains("\"model\""));
        assert!(!json.contains("\"tokens\""));
    }

    #[test]
    fn test_tool_call_json() {
        let event =
            LogEvent::tool_call("tc_001", "read_file", serde_json::json!({"path": "foo.rs"}));
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"id\":\"tc_001\""));
        assert!(json.contains("\"name\":\"read_file\""));
        assert!(json.contains("\"path\":\"foo.rs\""));
    }

    #[test]
    fn test_tool_result_json() {
        let event = LogEvent::tool_result("tc_001", "fn main() {}");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"tool_result\""));
        assert!(json.contains("\"id\":\"tc_001\""));
        assert!(json.contains("\"result\":\"fn main() {}\""));
        // truncated: false should be omitted
        assert!(!json.contains("\"truncated\""));
    }

    #[test]
    fn test_tool_result_truncated_json() {
        let event = LogEvent::tool_result_truncated("tc_001", "...", 50000);
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"truncated\":true"));
        assert!(json.contains("\"full_size\":50000"));
    }

    #[test]
    fn test_tool_error_json() {
        let event = LogEvent::tool_error("tc_001", "File not found");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"error\":\"File not found\""));
    }

    #[test]
    fn test_error_event_json() {
        let event = LogEvent::error("Rate limited", true);
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"message\":\"Rate limited\""));
        assert!(json.contains("\"recoverable\":true"));
    }

    #[test]
    fn test_jsonl_roundtrip() {
        let events = vec![
            LogEvent::system("System prompt"),
            LogEvent::user("Hello"),
            LogEvent::assistant("Hi!"),
            LogEvent::tool_call("t1", "test", serde_json::json!({})),
            LogEvent::tool_result("t1", "result"),
            LogEvent::error("oops", false),
        ];

        for event in events {
            let json = event.to_jsonl().unwrap();
            let parsed = LogEvent::from_jsonl(&json).unwrap();
            let json2 = parsed.to_jsonl().unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn test_parse_example_jsonl() {
        // From the spec. The `assistant` line's `tokens` used to read
        // `{"in":10,"out":5}`, which is *not* a compatibility requirement: no
        // production writer ever emitted a `tokens` field at all
        // (`LogEvent::assistant`, the constructor `inject_context` and both
        // fork paths use, sets `tokens: None`), so that shape never reached
        // disk. It is written here in the canonical field names.
        let lines = [
            r#"{"ts":"2026-01-04T15:30:00Z","type":"system","content":"You are a helpful assistant..."}"#,
            r#"{"ts":"2026-01-04T15:30:01Z","type":"user","content":"Hello"}"#,
            r#"{"ts":"2026-01-04T15:30:02Z","type":"assistant","content":"Hi!","model":"claude-3-haiku","tokens":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#,
            r#"{"ts":"2026-01-04T15:30:03Z","type":"tool_call","id":"tc_001","name":"read_file","args":{"path":"foo.rs"}}"#,
            r#"{"ts":"2026-01-04T15:30:04Z","type":"tool_result","id":"tc_001","result":"fn main()...","truncated":false}"#,
            r#"{"ts":"2026-01-04T15:30:05Z","type":"error","message":"Rate limited","recoverable":true}"#,
        ];

        for line in lines {
            let event = LogEvent::from_jsonl(line).unwrap();
            assert!(event.timestamp().year() == 2026);
        }
    }

    #[test]
    fn test_bash_spawned_json() {
        let event = LogEvent::bash_spawned("task-001", "cargo build");
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"bash_spawned\""));
        assert!(json.contains("\"id\":\"task-001\""));
        assert!(json.contains("\"command\":\"cargo build\""));
    }

    #[test]
    fn test_bash_completed_json() {
        let event = LogEvent::bash_completed("task-001", "Build succeeded", 0);
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"bash_completed\""));
        assert!(json.contains("\"exit_code\":0"));
    }

    #[test]
    fn test_bash_failed_json() {
        let event = LogEvent::bash_failed("task-001", "Command failed", Some(1));
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"bash_failed\""));
        assert!(json.contains("\"exit_code\":1"));
    }

    #[test]
    fn test_subagent_spawned_json() {
        let event = LogEvent::subagent_spawned(
            "sub-20260124-1432-beef",
            "[[.subagents/sub-20260124-1432-beef/session]]",
            "Research topic X",
        );
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"subagent_spawned\""));
        assert!(json.contains("\"session_link\":\"[[.subagents/sub-20260124-1432-beef/session]]\""));
        assert!(json.contains("\"description\":\"Research topic X\""));
    }

    #[test]
    fn test_subagent_completed_json() {
        let event = LogEvent::subagent_completed(
            "sub-20260124-1432-beef",
            "[[.subagents/sub-20260124-1432-beef/session]]",
            "Found 5 relevant files",
        );
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"subagent_completed\""));
        assert!(json.contains("\"summary\":\"Found 5 relevant files\""));
    }

    #[test]
    fn test_subagent_failed_json() {
        let event = LogEvent::subagent_failed(
            "sub-20260124-1432-beef",
            "[[.subagents/sub-20260124-1432-beef/session]]",
            "Timeout",
        );
        let json = event.to_jsonl().unwrap();

        assert!(json.contains("\"type\":\"subagent_failed\""));
        assert!(json.contains("\"error\":\"Timeout\""));
    }

    #[test]
    fn test_background_events_roundtrip() {
        let events = vec![
            LogEvent::bash_spawned("t1", "ls -la"),
            LogEvent::bash_completed("t1", "output", 0),
            LogEvent::bash_failed("t2", "error", Some(1)),
            LogEvent::subagent_spawned("t3", "[[.subagents/t3/session]]", "prompt"),
            LogEvent::subagent_completed("t3", "[[.subagents/t3/session]]", "result"),
            LogEvent::subagent_failed("t4", "[[.subagents/t4/session]]", "failed"),
        ];

        for event in events {
            let json = event.to_jsonl().unwrap();
            let parsed = LogEvent::from_jsonl(&json).unwrap();
            let json2 = parsed.to_jsonl().unwrap();
            assert_eq!(json, json2);
        }
    }
}
