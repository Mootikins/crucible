//! Session log events for JSONL persistence
//!
//! These events are for session persistence/resume, separate from
//! `crucible_core::events::SessionEvent`, the canonical event type.

use chrono::{DateTime, Utc};
use crucible_core::protocol::session_events::{SessionEventPayload, TurnPayload};
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
    System {
        ts: DateTime<Utc>,
        content: String,
        /// What kind of block this content is, for the handler that wants to
        /// find it again.
        ///
        /// The tags travel to the `ContextMessage` the turn assembles, so a
        /// `transform_context` handler identifies an injected block by its
        /// kind instead of by a substring of its text. A record written
        /// before this field loads with no tag, which is what an untagged
        /// system message means.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        /// The `kind` and `source` of an injection. The turn wraps the
        /// content in one `<system-message>` element with them, live and on
        /// replay. `None`: plain system text, as every record before this
        /// field.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        injection: Option<(String, String)>,
    },

    /// User message
    User {
        ts: DateTime<Utc>,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
    },

    /// Conversation context was cleared at this point in the same session.
    Clear {
        ts: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
    },

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

    /// Error during session
    Error {
        ts: DateTime<Utc>,
        message: String,
        /// Whether the error is recoverable
        #[serde(default)]
        recoverable: bool,
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
            tags: Vec::new(),
            injection: None,
        }
    }

    /// Create a user message event
    pub fn user(content: impl Into<String>) -> Self {
        LogEvent::User {
            ts: Utc::now(),
            content: content.into(),
            plugin: None,
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

    /// Create an error event
    pub fn error(message: impl Into<String>, recoverable: bool) -> Self {
        LogEvent::Error {
            ts: Utc::now(),
            message: message.into(),
            recoverable,
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
            | LogEvent::Clear { ts, .. }
            | LogEvent::Assistant { ts, .. }
            | LogEvent::Thinking { ts, .. }
            | LogEvent::ToolCall { ts, .. }
            | LogEvent::ToolResult { ts, .. }
            | LogEvent::Error { ts, .. }
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

/// One line of `session.jsonl`, across its transport, view and acceptance shapes.
///
/// The log is mixed by construction and always has been. `persist_event`
/// (`server/core.rs`) appends a serialized [`SessionEventMessage`] —
/// `{"type":"event","event":"user_message","data":{…}}` — and that is the
/// overwhelming majority of every real file. The daemon writes the clear and
/// the accepted context in the same wire shape, on its own ordered path.
/// Older daemons wrote [`LogEvent`] lines — `{"type":"user","ts":…,"content":…}`
/// — and [`InjectedContext`] lines, and a stored session keeps them, so this
/// type still reads both. A reader that understands only one shape silently
/// drops the rest of the file. That was the bug this type exists to make
/// unrepresentable.
#[derive(Debug, Clone)]
pub enum SessionLogLine {
    /// The daemon's broadcast event, as persisted.
    Wire(SessionEventMessage),
    /// The presentation-shaped event that older daemons wrote.
    View(LogEvent),
    /// Accepted context, as older daemons wrote it.
    Injection(InjectedContext),
}

/// The anchor is a message id, not the physical log position: the broadcast
/// writer can append an in-flight turn after this synchronous acceptance write.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename = "context_injection")]
pub struct InjectedContext {
    pub after_turn: Option<String>,
    pub message: LogEvent,
}

/// Reads only the discriminator, so no full parse is attempted twice.
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
            Some("context_injection") => serde_json::from_str(line).map(SessionLogLine::Injection),
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
    // `EventBus` owns the only sender, and it stamps each event before the
    // persist task sees it, so a line that this build wrote has a timestamp.
    // A log that an older daemon wrote can hold a line with no timestamp:
    // that daemon also sent some events past the stamp. Sessions are the
    // user's plaintext data and have no rewrite pass, so the fallback stays.
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
        // Typed, so an old flat origin goes through the one migration.
        "user_message" => match msg.payload() {
            Ok(SessionEventPayload::Turn(TurnPayload::UserMessage {
                content, origin, ..
            })) => Some(LogEvent::User {
                ts,
                content: text("content").map(|_| content)?,
                plugin: origin.and_then(|o| o.plugin().map(str::to_owned)),
            }),
            _ => None,
        },
        "thinking" => Some(LogEvent::Thinking {
            ts,
            content: text("content")?,
        }),
        // The clear marker. A log that an older daemon wrote can hold both a
        // `clear` view line and this event for one clear. They are adjacent,
        // and a second clear of an empty context changes nothing.
        "context_cleared" => match msg.payload() {
            Ok(SessionEventPayload::Turn(TurnPayload::ContextCleared { plugin })) => {
                Some(LogEvent::Clear { ts, plugin })
            }
            _ => None,
        },
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
            tags: Vec::new(),
            injection: None,
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

/// The wire payload of an accepted context message, or `None` for an event
/// that is not a context message.
///
/// The one mapping from the read model to the stored event. [`injected_event`]
/// is its inverse.
pub(crate) fn injection_payload(
    message: &LogEvent,
    after_turn: Option<String>,
) -> Option<TurnPayload> {
    let (role, content, tags, kind, source) = match message {
        LogEvent::System {
            content,
            tags,
            injection,
            ..
        } => {
            let (kind, source) = injection.clone().unzip();
            ("system", content, tags.clone(), kind, source)
        }
        LogEvent::User {
            content, plugin, ..
        } => (
            "user",
            content,
            Vec::new(),
            plugin.as_ref().map(|_| INJECTED_PLUGIN_KIND.to_string()),
            plugin.clone(),
        ),
        LogEvent::Assistant { content, .. } => ("assistant", content, Vec::new(), None, None),
        _ => return None,
    };
    Some(TurnPayload::ContextInjected {
        role: role.to_string(),
        content: content.clone(),
        tags,
        kind,
        source,
        after_turn,
    })
}

/// The `kind` of a user message that a plugin injected. Its `source` is the
/// plugin name.
const INJECTED_PLUGIN_KIND: &str = "plugin";

/// The context message of a stored `context_injected` event.
fn injected_event(
    ts: DateTime<Utc>,
    role: &str,
    content: String,
    tags: Vec<String>,
    kind: Option<String>,
    source: Option<String>,
) -> LogEvent {
    match role {
        "user" => LogEvent::User {
            ts,
            content,
            plugin: source.filter(|_| kind.as_deref() == Some(INJECTED_PLUGIN_KIND)),
        },
        "assistant" => LogEvent::Assistant {
            ts,
            content,
            model: None,
            tokens: None,
        },
        _ => LogEvent::System {
            ts,
            content,
            tags,
            injection: kind.zip(source),
        },
    }
}

/// `message` as one `session.jsonl` line, stamped with `ts`.
pub(crate) fn stored_line(
    mut message: SessionEventMessage,
    ts: DateTime<Utc>,
) -> Result<String, serde_json::Error> {
    message.timestamp = Some(ts);
    serde_json::to_string(&message)
}

/// Parse a whole session log into presentation events.
///
/// Pure `&str -> Vec<LogEvent>` so the async file-backed reader
/// (`observe::load_events`) and the sync in-memory one
/// (`observe::rebuild::rebuild_tree_from_str`) share one parser instead of
/// the two divergent line loops they had — which is how only one of them
/// would have been fixed.
///
/// A line that parses as no recognized shape warns and is skipped, keeping a
/// partially-corrupt log recoverable. A line that parses but maps to no
/// presentation event is skipped **silently**: a new persisted event kind is
/// not corruption, and warning on it would put a line in the log for every
/// `segment_complete` of every turn.
pub fn parse_session_log(jsonl: &str) -> Vec<LogEvent> {
    replay_session_log(jsonl)
        .into_iter()
        .map(|row| row.event)
        .collect()
}

/// One replayed message, in the order that a turn reads it.
pub(crate) struct ReplayRow {
    pub event: LogEvent,
    /// Accepted context rather than a turn of the conversation.
    pub injected: bool,
    /// The stored wire event that the row came from, when it came from one.
    /// A fork copies it, so the copy keeps the event's own fields.
    pub wire: Option<SessionEventMessage>,
}

/// Replay preserves whether a message is context rather than a user turn.
pub(crate) fn replay_session_log(jsonl: &str) -> Vec<ReplayRow> {
    let mut events = Vec::new();
    // The model a turn ran under is announced once, by `model_switched`,
    // and not repeated on each `message_complete`. Carry it forward so
    // `## Assistant (model)` in the markdown renderer and `[assistant
    // (model)]` in `cru session show` say something true. Turns before the
    // first switch keep `None`: a session's *starting* model is never
    // emitted as an event, and inventing one would be worse than omitting it.
    let mut current_model: Option<String> = None;

    let lines: Vec<_> = jsonl.lines().enumerate().filter_map(|(line_no, line)| {
        if line.trim().is_empty() { return None; }
        match SessionLogLine::from_jsonl(line.trim()) {
            Ok(event) => Some(event),
            Err(error) => {
                tracing::warn!(line = line_no + 1, %error, "skipping unparseable session log line");
                None
            }
        }
    }).collect();
    let turns: Vec<_> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| match line {
            SessionLogLine::Wire(msg) if msg.event == "user_message" => {
                Some((index, msg.data.get("message_id").and_then(Value::as_str)))
            }
            SessionLogLine::View(LogEvent::User { .. }) => Some((index, None)),
            _ => None,
        })
        .collect();
    let mut injections = std::collections::BTreeMap::<usize, Vec<LogEvent>>::new();
    for (index, line) in lines.iter().enumerate() {
        let accepted = match line {
            SessionLogLine::Injection(injection) => {
                Some((injection.after_turn.clone(), injection.message.clone()))
            }
            SessionLogLine::Wire(msg) => match msg.payload() {
                Ok(SessionEventPayload::Turn(TurnPayload::ContextInjected {
                    role,
                    content,
                    tags,
                    kind,
                    source,
                    after_turn,
                })) => Some((
                    after_turn,
                    injected_event(
                        msg.timestamp.unwrap_or_else(Utc::now),
                        &role,
                        content,
                        tags,
                        kind,
                        source,
                    ),
                )),
                _ => None,
            },
            SessionLogLine::View(_) => None,
        };
        if let Some((after_turn, message)) = accepted {
            let anchor = match &after_turn {
                Some(id) => turns
                    .iter()
                    .find(|(_, turn)| *turn == Some(id.as_str()))
                    .map(|(index, _)| *index)
                    .unwrap_or(lines.len()),
                None => index,
            };
            let boundary = turns
                .iter()
                .find(|(index, _)| *index > anchor)
                .map(|(index, _)| *index)
                .unwrap_or(lines.len());
            injections.entry(boundary).or_default().push(message);
        }
    }
    for (index, line) in lines.into_iter().enumerate() {
        events.extend(
            injections
                .remove(&index)
                .unwrap_or_default()
                .into_iter()
                .map(|event| ReplayRow {
                    event,
                    injected: true,
                    wire: None,
                }),
        );
        match line {
            SessionLogLine::Wire(msg) => {
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
                    events.push(ReplayRow {
                        event,
                        injected: false,
                        wire: Some(msg),
                    });
                }
            }
            SessionLogLine::View(event) => events.push(ReplayRow {
                event,
                injected: false,
                wire: None,
            }),
            SessionLogLine::Injection(_) => {}
        }
    }
    // Accepted but not yet consumed: a resumed turn must see these too.
    events.extend(injections.into_values().flatten().map(|event| ReplayRow {
        event,
        injected: true,
        wire: None,
    }));

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn deferred_context_is_replayed_at_its_turn_boundary_even_when_writers_race() {
        let first = SessionEventMessage::user_message("s", "first", "question");
        let second = SessionEventMessage::user_message("s", "second", "follow-up");
        let injection = serde_json::to_string(&InjectedContext {
            after_turn: Some("first".into()),
            message: LogEvent::system("remember"),
        })
        .unwrap();
        let first = serde_json::to_string(&first).unwrap();
        let second = serde_json::to_string(&second).unwrap();
        let reply = LogEvent::assistant("answer").to_jsonl().unwrap();
        for lines in [
            vec![&injection, &first, &reply, &second],
            vec![&first, &injection, &reply, &second],
            vec![&first, &reply, &injection, &second],
        ] {
            let events =
                parse_session_log(&lines.into_iter().cloned().collect::<Vec<_>>().join("\n"));
            assert!(
                matches!(&events[..], [LogEvent::User { .. }, LogEvent::Assistant { .. }, LogEvent::System { content, .. }, LogEvent::User { .. }] if content == "remember")
            );
        }
        let pending = parse_session_log(&[first, injection, reply].join("\n"));
        assert!(
            matches!(&pending[..], [LogEvent::User { .. }, LogEvent::Assistant { .. }, LogEvent::System { content, .. }] if content == "remember")
        );
    }

    /// The wire `context_injected` line takes the anchor of the old
    /// `context_injection` line, in every order the two writers can race.
    #[test]
    fn a_wire_injection_is_replayed_at_its_turn_boundary_even_when_writers_race() {
        let first =
            serde_json::to_string(&SessionEventMessage::user_message("s", "first", "question"))
                .unwrap();
        let second = serde_json::to_string(&SessionEventMessage::user_message(
            "s",
            "second",
            "follow-up",
        ))
        .unwrap();
        let injection = stored_line(
            SessionEventMessage::typed(
                "s",
                injection_payload(&LogEvent::system("remember"), Some("first".into())).unwrap(),
            ),
            Utc::now(),
        )
        .unwrap();
        let reply = serde_json::to_string(&SessionEventMessage::message_complete(
            "s", "first", "answer", None, None,
        ))
        .unwrap();
        for lines in [
            [&injection, &first, &reply, &second],
            [&first, &injection, &reply, &second],
            [&first, &reply, &injection, &second],
        ] {
            let rows = replay_session_log(&lines.map(String::as_str).join("\n"));
            let shape: Vec<_> = rows.iter().map(|r| (&r.event, r.injected)).collect();
            assert!(
                matches!(&shape[..], [
                    (LogEvent::User { .. }, false),
                    (LogEvent::Assistant { .. }, false),
                    (LogEvent::System { content, .. }, true),
                    (LogEvent::User { .. }, false),
                ] if content == "remember"),
                "{shape:?}"
            );
        }
    }

    /// Each context message survives the trip to the stored event and back,
    /// with its role, tags and provenance.
    #[test]
    fn a_context_message_round_trips_through_the_stored_event() {
        let ts = Utc::now();
        let messages = [
            LogEvent::System {
                ts,
                content: "diff".into(),
                tags: vec!["review".into()],
                injection: Some(("review".into(), "branch:main".into())),
            },
            LogEvent::System {
                ts,
                content: "plain".into(),
                tags: Vec::new(),
                injection: None,
            },
            LogEvent::User {
                ts,
                content: "from a plugin".into(),
                plugin: Some("goal".into()),
            },
            LogEvent::User {
                ts,
                content: "from a person".into(),
                plugin: None,
            },
            LogEvent::Assistant {
                ts,
                content: "said".into(),
                model: None,
                tokens: None,
            },
        ];
        for message in messages {
            let Some(TurnPayload::ContextInjected {
                role,
                content,
                tags,
                kind,
                source,
                after_turn,
            }) = injection_payload(&message, None)
            else {
                panic!("{message:?} is context");
            };
            assert_eq!(after_turn, None);
            let back = injected_event(ts, &role, content, tags, kind, source);
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                serde_json::to_value(&message).unwrap()
            );
        }
        assert!(injection_payload(&LogEvent::Clear { ts, plugin: None }, None).is_none());
    }

    /// The stored `context_cleared` is the clear. A log from an older daemon
    /// holds both a `clear` view line and the broadcast event for one clear;
    /// the two adjacent clears leave the same empty context.
    #[test]
    fn the_stored_clear_marker_is_read_as_a_clear() {
        let cleared = stored_line(
            SessionEventMessage::typed("s", TurnPayload::ContextCleared { plugin: None }),
            Utc::now(),
        )
        .unwrap();
        let old = LogEvent::Clear {
            ts: Utc::now(),
            plugin: None,
        }
        .to_jsonl()
        .unwrap();
        let before =
            serde_json::to_string(&SessionEventMessage::user_message("s", "a", "before")).unwrap();
        let after =
            serde_json::to_string(&SessionEventMessage::user_message("s", "b", "after")).unwrap();
        let events =
            parse_session_log(&[before.clone(), cleared.clone(), after.clone()].join("\n"));
        assert!(matches!(
            &events[..],
            [
                LogEvent::User { .. },
                LogEvent::Clear { .. },
                LogEvent::User { .. }
            ]
        ));
        let tree = crate::observe::rebuild::rebuild_tree_from_str(
            &[before, old, cleared, after].join("\n"),
        );
        let path = tree.path_to_here(tree.current());
        let said = |text: &str| {
            path.iter().any(|id| {
                matches!(&tree.get(*id).content,
                crucible_core::turn::NodeContent::User { text: t } if t == text)
            })
        };
        assert!(
            !said("before") && said("after"),
            "only the turn after the clear stays"
        );
    }

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
