//! The lines of `session.jsonl`, and the messages that a turn reads from them.
//!
//! What a person sees of a session is the transcript
//! ([`crucible_core::transcript`]), which the daemon folds once. This module
//! keeps two other jobs:
//! - [`stored_events`] turns each stored line into its current wire event.
//!   An older daemon wrote [`LogEvent`] and `context_injection` lines, and a
//!   stored session keeps them, so the fold needs them in wire form.
//! - [`replay_session_log`] gives the messages of the model context, for the
//!   conversation tree (`rebuild.rs`) and for a fork (`copy_session`).
//!   [`LogEvent`] is the form of one such message, and the form of accepted
//!   context before the daemon stores it.

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
        result: String,
        /// Error message if tool failed
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

impl LogEvent {
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

    /// Get the timestamp of this event
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            LogEvent::System { ts, .. }
            | LogEvent::User { ts, .. }
            | LogEvent::Clear { ts, .. }
            | LogEvent::Assistant { ts, .. }
            | LogEvent::Thinking { ts, .. }
            | LogEvent::ToolCall { ts, .. }
            | LogEvent::ToolResult { ts, .. } => *ts,
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

    /// [`Self::from_jsonl`] for a line that is already parsed.
    pub fn from_value(line: Value) -> Result<Self, serde_json::Error> {
        match line.get("type").and_then(Value::as_str) {
            Some("context_injection") => {
                serde_json::from_value(line).map(SessionLogLine::Injection)
            }
            Some("event" | "replay_event") => {
                serde_json::from_value(line).map(SessionLogLine::Wire)
            }
            _ => serde_json::from_value(line).map(SessionLogLine::View),
        }
    }
}

/// Project a persisted [`SessionEventMessage`] onto the message form of the
/// model context.
///
/// Only the events `should_persist` (`server/core.rs`) admits can reach
/// a session log, so only those are mapped. `None` means "carries no
/// conversation content" — an ordinary outcome, not a parse failure, so
/// callers must not warn on it.
fn wire_to_log_event(msg: &SessionEventMessage) -> Option<LogEvent> {
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
            // message_id, full_response and usage). `replay_session_log`
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
        // `model_switched` is consumed by `replay_session_log` for the model
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

/// Each stored line as its current wire event, in log order: the input of
/// the transcript fold.
///
/// A wire line takes the migration of old event names (`migrate_history`).
/// An older daemon wrote a [`LogEvent`] line or a `context_injection` line,
/// and each becomes the wire event with the same meaning, so that the fold
/// and every reader of it see the whole log. A line of no known shape is
/// skipped with a warning.
pub fn stored_events(session_id: &str, lines: Vec<Value>) -> Vec<SessionEventMessage> {
    crucible_core::protocol::session_events::migrate_history(lines)
        .into_iter()
        .filter_map(|line| match SessionLogLine::from_value(line) {
            Ok(SessionLogLine::Wire(message)) => Some(message),
            Ok(SessionLogLine::View(event)) => view_event(session_id, event),
            Ok(SessionLogLine::Injection(injection)) => {
                let ts = injection.message.timestamp();
                injection_payload(&injection.message, injection.after_turn)
                    .map(|payload| stamped(SessionEventMessage::typed(session_id, payload), ts))
            }
            Err(error) => {
                tracing::warn!(%error, "skipping unparseable session log line");
                None
            }
        })
        .collect()
}

/// The wire event of an old view line.
fn view_event(session_id: &str, event: LogEvent) -> Option<SessionEventMessage> {
    let ts = event.timestamp();
    let message = match event {
        LogEvent::User {
            content, plugin, ..
        } => SessionEventMessage::typed(
            session_id,
            TurnPayload::UserMessage {
                // The fold numbers a turn with no id.
                message_id: String::new(),
                content,
                origin: plugin.map(crucible_core::turn::TurnOrigin::Plugin),
            },
        ),
        LogEvent::Assistant {
            content, tokens, ..
        } => SessionEventMessage::message_complete(session_id, "", content, tokens.as_ref(), None),
        // A plain system line (a fork of an older daemon wrote one) is
        // context that the session accepted.
        event @ LogEvent::System { .. } => {
            SessionEventMessage::typed(session_id, injection_payload(&event, None)?)
        }
        LogEvent::Clear { plugin, .. } => {
            SessionEventMessage::typed(session_id, TurnPayload::ContextCleared { plugin })
        }
        // No daemon wrote these as view lines. They exist only as the
        // model-context form of a wire event.
        LogEvent::Thinking { .. } | LogEvent::ToolCall { .. } | LogEvent::ToolResult { .. } => {
            return None
        }
    };
    Some(stamped(message, ts))
}

fn stamped(mut message: SessionEventMessage, ts: DateTime<Utc>) -> SessionEventMessage {
    message.timestamp = Some(ts);
    message
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

/// The messages of the model context, in the order that a turn reads them.
///
/// This is not a transcript. It is the input of the conversation tree
/// (`rebuild.rs`) and of a fork (`copy_session`). Replay preserves whether a
/// message is context rather than a user turn.
pub(crate) fn replay_session_log(jsonl: &str) -> Vec<ReplayRow> {
    let mut events = Vec::new();
    // The model a turn ran under is announced once, by `model_switched`,
    // and not repeated on each `message_complete`. The row carries it, so
    // that a fork announces each model again in the copy.
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

    fn replayed(jsonl: &str) -> Vec<LogEvent> {
        replay_session_log(jsonl)
            .into_iter()
            .map(|row| row.event)
            .collect()
    }

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
            let events = replayed(&lines.into_iter().cloned().collect::<Vec<_>>().join("\n"));
            assert!(
                matches!(&events[..], [LogEvent::User { .. }, LogEvent::Assistant { .. }, LogEvent::System { content, .. }, LogEvent::User { .. }] if content == "remember")
            );
        }
        let pending = replayed(&[first, injection, reply].join("\n"));
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
        let events = replayed(&[before.clone(), cleared.clone(), after.clone()].join("\n"));
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
    fn test_assistant_minimal_json() {
        let event = LogEvent::assistant("Hi!");
        let json = event.to_jsonl().unwrap();

        // Should NOT contain model or tokens when None
        assert!(!json.contains("\"model\""));
        assert!(!json.contains("\"tokens\""));
    }

    #[test]
    fn test_jsonl_roundtrip() {
        let events = vec![
            LogEvent::system("System prompt"),
            LogEvent::user("Hello"),
            LogEvent::assistant("Hi!"),
        ];

        for event in events {
            let json = event.to_jsonl().unwrap();
            let parsed = LogEvent::from_jsonl(&json).unwrap();
            let json2 = parsed.to_jsonl().unwrap();
            assert_eq!(json, json2);
        }
    }
}
