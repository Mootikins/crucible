use crucible_core::protocol::session_events::turn::ToolResultBody;
use crucible_daemon::SessionEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
    Token {
        content: String,
    },

    ToolCall {
        id: String,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments: Option<serde_json::Value>,
    },

    ToolResult {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        /// True if this tool requested an agent-turn early-stop
        /// (daemon's conjunctive batch-terminate check fired). UI renders
        /// a "Terminated" badge on the tool card.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        terminate: bool,
    },

    ToolResultDelta {
        id: String,
        delta: String,
    },

    ToolResultComplete {
        id: String,
    },

    ToolResultError {
        id: String,
        error: String,
    },

    Thinking {
        content: String,
    },

    /// A text segment that streamed before a tool call, carrying the turn's
    /// `message_id`, its 0-based `index`, and the segment text. Lets the
    /// frontend materialize a canonical per-segment bubble that matches
    /// history reconstruction.
    SegmentComplete {
        message_id: String,
        index: u64,
        content: String,
    },

    MessageComplete {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt_tokens: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        completion_tokens: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_tokens: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_read_tokens: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_creation_tokens: Option<u64>,
    },

    Error {
        code: String,
        message: String,
    },

    InteractionRequested {
        id: String,
        #[serde(flatten)]
        request: serde_json::Value,
    },

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
        #[serde(skip_serializing_if = "Option::is_none")]
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

    ContextUsage {
        used: u64,
        total: u64,
    },

    PrecognitionResult {
        notes_count: usize,
        #[serde(default)]
        notes: Vec<PrecognitionNote>,
    },

    ModeChanged {
        mode: String,
    },

    TitleChanged {
        title: String,
    },

    SessionEvent {
        event_type: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecognitionNote {
    pub name: String,
    #[serde(default)]
    pub relevance: f64,
}

impl ChatEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            ChatEvent::Token { .. } => "token",
            ChatEvent::ToolCall { .. } => "tool_call",
            ChatEvent::ToolResult { .. } => "tool_result",
            ChatEvent::ToolResultDelta { .. } => "tool_result_delta",
            ChatEvent::ToolResultComplete { .. } => "tool_result_complete",
            ChatEvent::ToolResultError { .. } => "tool_result_error",
            ChatEvent::Thinking { .. } => "thinking",
            ChatEvent::SegmentComplete { .. } => "segment_complete",
            ChatEvent::MessageComplete { .. } => "message_complete",
            ChatEvent::Error { .. } => "error",
            ChatEvent::InteractionRequested { .. } => "interaction_requested",
            ChatEvent::SubagentSpawned { .. } => "subagent_spawned",
            ChatEvent::SubagentCompleted { .. } => "subagent_completed",
            ChatEvent::SubagentFailed { .. } => "subagent_failed",
            ChatEvent::DelegationSpawned { .. } => "delegation_spawned",
            ChatEvent::DelegationCompleted { .. } => "delegation_completed",
            ChatEvent::DelegationFailed { .. } => "delegation_failed",
            ChatEvent::ContextUsage { .. } => "context_usage",
            ChatEvent::PrecognitionResult { .. } => "precognition_result",
            ChatEvent::ModeChanged { .. } => "mode_changed",
            ChatEvent::TitleChanged { .. } => "title_changed",
            ChatEvent::SessionEvent { .. } => "session_event",
        }
    }

    /// Project a daemon wire event onto the browser's presentation enum.
    ///
    /// Decodes the typed payload rather than digging keys out of `data`, so the
    /// field names here cannot drift from the producer's. Seven input arms went
    /// with the change — `thinking_delta`, `tool_call_start`, `turn_complete`,
    /// `tool_result_delta`, `tool_result_complete`, `tool_result_error` and
    /// `context_usage` — because no daemon or core code emits any of them.
    /// `tool_call_start` was the one already provably drifted: `api.ts` had a
    /// listener for an SSE name `event_name()` could not produce.
    ///
    /// Two renames survive and are deliberate: `text_delta` → `token` and
    /// `precognition_complete` → `precognition_result`. Everything the enum does
    /// not model reaches the browser as the `session_event` passthrough, which is
    /// exactly the `Err(UnknownEvent)` path.
    /// Tool output as display text: strings verbatim, anything else as JSON,
    /// and `null` as absent. Shared by all three arms above so the envelope
    /// decision never changes how the payload itself is rendered.
    fn stringify_result(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        }
    }

    pub fn from_daemon_event(event: &SessionEvent) -> Self {
        use crucible_core::protocol::session_events::{
            JobPayload, SessionEventPayload, SettingsPayload, TurnPayload,
        };

        let passthrough = || ChatEvent::SessionEvent {
            event_type: event.event_type.clone(),
            data: event.data.clone(),
        };

        // Pre-flattened interaction payloads (older recordings, e2e mock frames)
        // carry `kind` at the top level and no `request`, so they do not decode
        // as `TurnPayload::InteractionRequested`. `normalize_interaction` already
        // passes them through unchanged; keep accepting them, because dropping
        // one means no permission prompt renders at all.
        if event.event_type == "interaction_requested" && event.data.get("kind").is_some() {
            return ChatEvent::InteractionRequested {
                id: event.data["id"]
                    .as_str()
                    .or_else(|| event.data["request_id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                request: normalize_interaction(&event.data),
            };
        }

        let payload = match SessionEventPayload::from_wire(&event.event_type, &event.data) {
            Ok(p) => p,
            Err(_) => return passthrough(),
        };

        match payload {
            SessionEventPayload::Turn(turn) => match turn {
                TurnPayload::TextDelta { content } => ChatEvent::Token { content },

                TurnPayload::Thinking { content } => ChatEvent::Thinking { content },

                TurnPayload::ToolCall {
                    call_id,
                    tool,
                    args,
                    ..
                } => ChatEvent::ToolCall {
                    id: call_id,
                    title: tool,
                    arguments: Some(args).filter(|a| !a.is_null()),
                },

                TurnPayload::ToolResult {
                    call_id,
                    result,
                    terminate,
                    ..
                } => {
                    // The wire carries success and failure in the SAME slot,
                    // discriminated only by which key is present:
                    // `{"result": …}` vs `{"error": …}`. `ToolResultBody` is
                    // the decoder for that envelope, and skipping it stringified
                    // an error object into the success field — the browser then
                    // rendered a failed tool as a completed one, while the TUI
                    // (which does decode) showed the error.
                    match ToolResultBody::of(&result) {
                        Some(ToolResultBody::Err { error, .. }) => {
                            ChatEvent::ToolResultError {
                                id: call_id,
                                error,
                            }
                        }
                        // Unwrap the envelope so the UI gets the tool's own
                        // output rather than the `{"result": …}` wrapper.
                        Some(ToolResultBody::Ok { result, .. }) => ChatEvent::ToolResult {
                            id: call_id,
                            result: Self::stringify_result(&result),
                            terminate,
                        },
                        // A shape neither variant covers — a bare string, or an
                        // object with neither key. Keep the existing fallback
                        // rather than inventing one: a body that will not decode
                        // is still a tool result the user ran.
                        None => ChatEvent::ToolResult {
                            id: call_id,
                            result: Self::stringify_result(&result),
                            terminate,
                        },
                    }
                }

                TurnPayload::SegmentComplete {
                    message_id,
                    index,
                    content,
                } => ChatEvent::SegmentComplete {
                    message_id,
                    index: index as u64,
                    content,
                },

                TurnPayload::MessageComplete {
                    message_id,
                    full_response,
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                    cache_read_tokens,
                    cache_creation_tokens,
                } => ChatEvent::MessageComplete {
                    id: message_id,
                    content: full_response,
                    prompt_tokens: prompt_tokens.map(u64::from),
                    completion_tokens: completion_tokens.map(u64::from),
                    total_tokens: total_tokens.map(u64::from),
                    cache_read_tokens: cache_read_tokens.map(u64::from),
                    cache_creation_tokens: cache_creation_tokens.map(u64::from),
                },

                // `ended` carries the turn's failure as an `"error: "`-prefixed
                // reason; nothing else on the wire produces `ChatEvent::Error`,
                // so without this arm a browser user saw a stalled turn where
                // the TUI showed a message.
                TurnPayload::Ended { ref reason } => match chat_error_from_ended_reason(reason) {
                    Some(message) => ChatEvent::Error {
                        code: "turn_failed".to_string(),
                        message: message.to_string(),
                    },
                    None => passthrough(),
                },

                TurnPayload::InteractionRequested { ref request_id, .. } => {
                    ChatEvent::InteractionRequested {
                        id: request_id.clone(),
                        request: normalize_interaction(&event.data),
                    }
                }

                TurnPayload::PrecognitionComplete {
                    notes_count,
                    ref notes,
                    ..
                } => ChatEvent::PrecognitionResult {
                    notes_count,
                    notes: notes
                        .iter()
                        .map(|n| PrecognitionNote {
                            name: n.title.clone(),
                            relevance: n.score,
                        })
                        .collect(),
                },

                // Not modelled by the browser's presentation enum; the raw
                // envelope still reaches it through the passthrough.
                TurnPayload::UserMessage { .. }
                | TurnPayload::ToolCallArgsUpdate { .. }
                | TurnPayload::ToolCallDiffUpdate { .. }
                | TurnPayload::InteractionCompleted { .. }
                | TurnPayload::InjectionPending { .. }
                | TurnPayload::ContextInjected { .. }
                | TurnPayload::PostLlmCall { .. } => passthrough(),
            },

            SessionEventPayload::Settings(SettingsPayload::ModeChanged { mode }) => {
                ChatEvent::ModeChanged { mode }
            }
            SessionEventPayload::Settings(SettingsPayload::TitleChanged { title }) => {
                ChatEvent::TitleChanged { title }
            }

            SessionEventPayload::Job(JobPayload::DelegationSpawned {
                delegation_id,
                prompt,
                target_agent,
                ..
            }) => ChatEvent::DelegationSpawned {
                id: delegation_id,
                prompt,
                target_agent,
            },
            SessionEventPayload::Job(JobPayload::DelegationCompleted {
                delegation_id,
                result_summary,
                ..
            }) => ChatEvent::DelegationCompleted {
                id: delegation_id,
                summary: result_summary,
            },
            SessionEventPayload::Job(JobPayload::DelegationFailed {
                delegation_id,
                error,
                ..
            }) => ChatEvent::DelegationFailed {
                id: delegation_id,
                error,
            },

            _ => passthrough(),
        }
    }
}

/// The message inside an `"error: "`-prefixed `ended` reason, with the
/// `ChatError` `Display` prefix stripped.
fn chat_error_from_ended_reason(reason: &str) -> Option<&str> {
    let inner = reason.strip_prefix("error: ")?;
    const PREFIXES: &[&str] = &[
        "Connection error: ",
        "Communication error: ",
        "Mode change error: ",
        "Command execution failed: ",
        "Invalid input: ",
        "Agent not available: ",
        "Internal error: ",
        "Invalid mode: ",
        "Operation not supported: ",
    ];
    Some(
        PREFIXES
            .iter()
            .find_map(|p| inner.strip_prefix(p))
            .unwrap_or(inner),
    )
}

/// Flatten a daemon `interaction_requested` payload into the shape the
/// frontend renders (`InteractionRequest` in web/src/lib/types.ts).
///
/// The daemon broadcasts `{request_id, request: {kind, ...}}` where a
/// permission's action is the tagged enum `{type: "bash"|"read"|"write"|
/// "tool", ...}` (crucible_core::interaction::PermAction). The frontend
/// expects the flat `{kind, id, action_type, tokens, tool_name?,
/// tool_args?}` — passing the nested wire shape through renders NO
/// interaction prompt at all. Payloads that already carry a top-level
/// `kind` (older recordings, e2e mock frames) pass through unchanged.
pub(crate) fn normalize_interaction(data: &serde_json::Value) -> serde_json::Value {
    use serde_json::{json, Value};

    if data.get("kind").is_some() {
        return data.clone();
    }

    let id = data["request_id"]
        .as_str()
        .or_else(|| data["id"].as_str())
        .unwrap_or("");
    let Some(request) = data.get("request") else {
        return data.clone();
    };
    let kind = request["kind"].as_str().unwrap_or("");

    if kind == "permission" {
        let action = &request["action"];
        let action_type = action["type"].as_str().unwrap_or("tool");
        let str_array = |v: &Value| -> Vec<String> {
            v.as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut out = json!({
            "kind": "permission",
            "id": id,
            "action_type": action_type,
        });
        match action_type {
            "bash" => {
                out["tokens"] = json!(str_array(&action["tokens"]));
            }
            // Path segments are stored piecewise for the vim-style pattern
            // builder; the web modal wants the whole path as one token
            // (same join as PermissionBridge::to_engine_input).
            "read" | "write" => {
                out["tokens"] = json!([str_array(&action["segments"]).join("/")]);
            }
            _ => {
                let name = action["name"].as_str().unwrap_or("unknown");
                out["tokens"] = json!([name]);
                out["tool_name"] = json!(name);
                if !action["args"].is_null() {
                    out["tool_args"] = action["args"].clone();
                }
            }
        }
        if let Some(diffs) = request.get("diffs") {
            out["diffs"] = diffs.clone();
        }
        return out;
    }

    // ask/popup/…: the inner request fields already match the frontend
    // types — flatten them next to kind and inject the request id.
    let mut out = request.clone();
    if let Some(obj) = out.as_object_mut() {
        obj.insert("id".into(), json!(id));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::protocol::SessionEventMessage;
    use crucible_daemon::SessionEvent;

    fn make_event(event_type: &str, data: serde_json::Value) -> SessionEvent {
        SessionEvent {
            event_type: event_type.to_string(),
            session_id: "test-session".to_string(),
            data,
        }
    }

    /// Convert a canonical daemon broadcast message into the client-side
    /// `SessionEvent` exactly as the subscription path does (event →
    /// event_type, data verbatim) — so these tests consume the REAL wire
    /// shapes, not hand-invented ones.
    fn from_wire(msg: SessionEventMessage) -> SessionEvent {
        SessionEvent {
            event_type: msg.event,
            session_id: msg.session_id,
            data: msg.data,
        }
    }

    /// The daemon announces tool calls as `tool_call` with
    /// `{call_id, tool, args}` (SessionEventMessage::tool_call). The web
    /// mapping must read THOSE fields — reading `id`/`name`/`arguments`
    /// renders every live tool card blank.
    #[test]
    fn real_tool_call_event_maps_id_title_and_arguments() {
        let event = from_wire(SessionEventMessage::tool_call(
            "s1",
            "call-1",
            "read_file",
            serde_json::json!({ "path": "foo.rs" }),
        ));

        let chat_event = ChatEvent::from_daemon_event(&event);
        assert_eq!(chat_event.event_name(), "tool_call");
        match chat_event {
            ChatEvent::ToolCall {
                id,
                title,
                arguments,
            } => {
                assert_eq!(id, "call-1");
                assert_eq!(title, "read_file");
                assert_eq!(arguments, Some(serde_json::json!({ "path": "foo.rs" })));
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    /// The daemon's thinking stream event is named `thinking`
    /// (SessionEventMessage::thinking) — not `thinking_delta`. Falling
    /// through to the generic passthrough silently drops thinking from
    /// the web UI.
    #[test]
    fn real_thinking_event_maps_to_thinking() {
        let event = from_wire(SessionEventMessage::thinking("s1", "pondering…"));

        let chat_event = ChatEvent::from_daemon_event(&event);
        assert_eq!(chat_event.event_name(), "thinking");
        match chat_event {
            ChatEvent::Thinking { content } => assert_eq!(content, "pondering…"),
            other => panic!("expected Thinking, got {other:?}"),
        }
    }

    #[test]
    fn real_text_delta_event_maps_to_token() {
        let event = from_wire(SessionEventMessage::text_delta("s1", "hel"));

        let chat_event = ChatEvent::from_daemon_event(&event);
        assert_eq!(chat_event.event_name(), "token");
        match chat_event {
            ChatEvent::Token { content } => assert_eq!(content, "hel"),
            other => panic!("expected Token, got {other:?}"),
        }
    }

    #[test]
    fn real_message_complete_event_maps_content_and_usage() {
        let usage = crucible_core::traits::llm::TokenUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        };
        let event = from_wire(SessionEventMessage::message_complete(
            "s1",
            "msg-1",
            "final answer",
            Some(&usage),
        ));

        let chat_event = ChatEvent::from_daemon_event(&event);
        assert_eq!(chat_event.event_name(), "message_complete");
        match chat_event {
            ChatEvent::MessageComplete {
                id,
                content,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                ..
            } => {
                assert_eq!(id, "msg-1");
                assert_eq!(content, "final answer");
                assert_eq!(prompt_tokens, Some(10));
                assert_eq!(completion_tokens, Some(5));
                assert_eq!(total_tokens, Some(15));
            }
            other => panic!("expected MessageComplete, got {other:?}"),
        }
    }

    /// The daemon emits `segment_complete` at each text→tool boundary
    /// (SessionEventMessage::segment_complete). The web mapping must carry
    /// message_id/index/content through under a dedicated SSE event name so
    /// the frontend can build a canonical segment bubble — falling through to
    /// the generic `session_event` passthrough would strand it.
    #[test]
    fn real_segment_complete_event_maps_fields_and_name() {
        let event = from_wire(SessionEventMessage::segment_complete(
            "s1",
            "msg-7",
            0,
            "Let me look that up.",
        ));

        let chat_event = ChatEvent::from_daemon_event(&event);
        assert_eq!(chat_event.event_name(), "segment_complete");
        match chat_event {
            ChatEvent::SegmentComplete {
                message_id,
                index,
                content,
            } => {
                assert_eq!(message_id, "msg-7");
                assert_eq!(index, 0);
                assert_eq!(content, "Let me look that up.");
            }
            other => panic!("expected SegmentComplete, got {other:?}"),
        }
    }

    /// Real tool results are arbitrary JSON (SessionEventMessage::tool_result
    /// takes a Value); non-string results must be stringified, not dropped.
    /// A tool that FAILED must reach the browser as an error.
    ///
    /// The daemon writes a failure as `{"error": …}` in the same `data.result`
    /// slot a success uses `{"result": …}` for — `ToolResultBody` is the
    /// decoder for that envelope. Skipping it stringified the error object into
    /// the success field, `chatEventReducer` marked the card `complete`, and
    /// every part of `ToolCard.tsx`'s error presentation became unreachable:
    /// failed tools rendered as successes. The TUI decoded it correctly the
    /// whole time (`chat_runner/commands.rs`), so the two clients disagreed
    /// about whether the tool worked.
    #[test]
    fn a_failed_tool_reaches_the_browser_as_an_error() {
        let event = from_wire(SessionEventMessage::tool_result(
            "s1",
            "call-1",
            "read_file",
            serde_json::json!({ "error": "No such file (os error 2)" }),
        ));

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::ToolResultError { id, error } => {
                assert_eq!(id, "call-1");
                assert_eq!(error, "No such file (os error 2)");
            }
            other => panic!(
                "a failed tool must map to ToolResultError; got {other:?} — \
                 the browser renders this as a successful result"
            ),
        }
    }

    /// ...and the success envelope still unwraps, rather than reaching the UI
    /// as the raw `{"result": …}` wrapper. Pins that fixing the failure case
    /// did not start double-wrapping the success case.
    #[test]
    fn a_successful_tool_result_is_unwrapped_from_its_envelope() {
        let event = from_wire(SessionEventMessage::tool_result(
            "s1",
            "call-2",
            "read_file",
            serde_json::json!({ "result": "file contents" }),
        ));

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::ToolResult { id, result, .. } => {
                assert_eq!(id, "call-2");
                assert_eq!(result.as_deref(), Some("file contents"));
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn real_tool_result_with_object_payload_is_stringified() {
        let event = from_wire(SessionEventMessage::tool_result(
            "s1",
            "call-1",
            "search",
            serde_json::json!({ "matches": 3 }),
        ));

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::ToolResult { id, result, .. } => {
                assert_eq!(id, "call-1");
                assert_eq!(
                    result.as_deref(),
                    Some(r#"{"matches":3}"#),
                    "object results must reach the UI as JSON text, not None"
                );
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_carries_terminate_flag() {
        let event = make_event(
            "tool_result",
            serde_json::json!({
                "call_id": "tc-1",
                "tool": "submit_answer",
                "result": "final",
                "terminate": true,
            }),
        );

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::ToolResult { id, terminate, .. } => {
                assert_eq!(id, "tc-1");
                assert!(
                    terminate,
                    "terminate must propagate through to the wire ChatEvent"
                );
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn tool_result_falls_back_to_call_id_when_id_absent() {
        let event = make_event(
            "tool_result",
            serde_json::json!({
                "call_id": "tc-fallback",
                "tool": "x",
                "result": "ok",
            }),
        );

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::ToolResult { id, terminate, .. } => {
                assert_eq!(id, "tc-fallback");
                assert!(!terminate, "terminate defaults to false when absent");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
    }

    /// The daemon emits `precognition_complete`; the frontend listens for
    /// `precognition_result`. If anyone renames the daemon side, this test
    /// breaks loudly instead of the precognition badge silently disappearing.
    #[test]
    fn precognition_complete_translates_to_precognition_result() {
        // The real wire shape: `PrecognitionNoteInfo` carries `title` and
        // `score`. `name`/`relevance` are the SSE *output* field names, and this
        // test used to feed them back in as input — a shape the daemon has never
        // emitted.
        let event = make_event(
            "precognition_complete",
            serde_json::json!({
                "notes_count": 2,
                "query_summary": "tokio pinning",
                "notes": [
                    { "title": "Note A", "kiln": "docs", "score": 0.9 },
                    { "title": "Note B", "kiln": "docs", "score": 0.7 },
                ],
            }),
        );

        let chat_event = ChatEvent::from_daemon_event(&event);
        assert_eq!(chat_event.event_name(), "precognition_result");
        match chat_event {
            ChatEvent::PrecognitionResult { notes_count, notes } => {
                assert_eq!(notes_count, 2);
                assert_eq!(notes.len(), 2);
                assert_eq!(notes[0].name, "Note A");
            }
            other => panic!("expected PrecognitionResult, got {other:?}"),
        }
    }

    /// The daemon wraps interactions as `{request_id, request: {kind,
    /// action: {type, tokens}}}` (SessionEventMessage::interaction_requested
    /// plus tagged PermAction). The frontend renders the FLAT shape `{kind, id,
    /// action_type, tokens}` — forwarding the nested wire shape means no
    /// permission prompt ever renders in the browser.
    #[test]
    fn real_bash_permission_flattens_to_frontend_shape() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let request = InteractionRequest::Permission(PermRequest::bash(["cargo", "test"]));
        let event = from_wire(SessionEventMessage::interaction_requested(
            "s1", "perm-1", &request,
        ));

        let chat_event = ChatEvent::from_daemon_event(&event);
        match chat_event {
            ChatEvent::InteractionRequested { id, request } => {
                assert_eq!(id, "perm-1");
                assert_eq!(request["kind"], "permission");
                assert_eq!(request["id"], "perm-1");
                assert_eq!(request["action_type"], "bash");
                assert_eq!(request["tokens"], serde_json::json!(["cargo", "test"]));
            }
            other => panic!("expected InteractionRequested, got {other:?}"),
        }
    }

    #[test]
    fn real_tool_permission_carries_name_and_args() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let request = InteractionRequest::Permission(PermRequest::tool(
            "web_search",
            serde_json::json!({ "query": "weighted aging" }),
        ));
        let event = from_wire(SessionEventMessage::interaction_requested(
            "s1", "perm-2", &request,
        ));

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::InteractionRequested { request, .. } => {
                assert_eq!(request["action_type"], "tool");
                assert_eq!(request["tool_name"], "web_search");
                assert_eq!(request["tool_args"]["query"], "weighted aging");
            }
            other => panic!("expected InteractionRequested, got {other:?}"),
        }
    }

    #[test]
    fn real_write_permission_joins_segments_into_one_path_token() {
        use crucible_core::interaction::{InteractionRequest, PermRequest};

        let request = InteractionRequest::Permission(PermRequest::write(["src", "main.rs"]));
        let event = from_wire(SessionEventMessage::interaction_requested(
            "s1", "perm-3", &request,
        ));

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::InteractionRequested { request, .. } => {
                assert_eq!(request["action_type"], "write");
                assert_eq!(request["tokens"], serde_json::json!(["src/main.rs"]));
            }
            other => panic!("expected InteractionRequested, got {other:?}"),
        }
    }

    /// Ask requests flatten too: the inner fields already match the frontend
    /// type, they just need `kind` kept and the request id injected.
    #[test]
    fn real_ask_interaction_flattens_with_id() {
        use crucible_core::interaction::{AskRequest, InteractionRequest};

        let ask = AskRequest::new("Pin tokio at 1.43?");
        let request = InteractionRequest::Ask(ask);
        let event = from_wire(SessionEventMessage::interaction_requested(
            "s1", "ask-1", &request,
        ));

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::InteractionRequested { id, request } => {
                assert_eq!(id, "ask-1");
                assert_eq!(request["kind"], "ask");
                assert_eq!(request["id"], "ask-1");
                assert_eq!(request["question"], "Pin tokio at 1.43?");
            }
            other => panic!("expected InteractionRequested, got {other:?}"),
        }
    }

    /// Already-flat payloads (older recordings, e2e mock frames) pass
    /// through untouched.
    #[test]
    fn flat_interaction_payload_passes_through() {
        let event = make_event(
            "interaction_requested",
            serde_json::json!({
                "kind": "permission",
                "id": "perm-9",
                "action_type": "bash",
                "tokens": ["ls"],
            }),
        );

        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::InteractionRequested { id, request } => {
                assert_eq!(id, "perm-9");
                assert_eq!(request["kind"], "permission");
                assert_eq!(request["tokens"], serde_json::json!(["ls"]));
            }
            other => panic!("expected InteractionRequested, got {other:?}"),
        }
    }

    // ── Cross-language drift guard ───────────────────────────────────

    /// Text between two markers, or a panic naming the marker that moved.
    fn between<'a>(src: &'a str, start: &str, end: &str) -> &'a str {
        let from = src
            .find(start)
            .unwrap_or_else(|| panic!("scan marker `{start}` not found — fix this test"))
            + start.len();
        let rest = &src[from..];
        let to = rest
            .find(end)
            .unwrap_or_else(|| panic!("scan marker `{end}` not found — fix this test"));
        &rest[..to]
    }

    /// Every quoted lowercase-ish literal in `src`, in either language: Rust
    /// `"…"` and TypeScript `'…'` both yield their contents.
    fn quoted_wire_literals(src: &str, quote: char) -> std::collections::BTreeSet<String> {
        let mut out = std::collections::BTreeSet::new();
        let mut rest = src;
        while let Some(open) = rest.find(quote) {
            rest = &rest[open + 1..];
            let Some(close) = rest.find(quote) else { break };
            let lit = &rest[..close];
            rest = &rest[close + 1..];
            if !lit.is_empty()
                && lit
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c == '.' || c == ':')
            {
                out.insert(lit.to_string());
            }
        }
        out
    }

    /// `event_name()` decides the SSE event name; `SSE_EVENT_TYPES` decides which
    /// names the browser registers a listener for. A name in one and not the
    /// other is a silently undelivered event — that is how `tool_call_start`
    /// ended up in `api.ts` with no Rust arm, and the existing TS-side guard
    /// (`chatEventReducer.test.ts`) compares TS to TS so it passed throughout.
    ///
    /// Source-scanning both is ugly; it is also the only check that spans the
    /// languages, and `rpc/dispatch.rs` sets the precedent.
    #[test]
    fn sse_event_names_match_the_frontend_listener_list() {
        let rust = quoted_wire_literals(
            between(
                include_str!("events.rs"),
                "pub fn event_name(&self) -> &'static str {",
                "\n    }\n",
            ),
            '"',
        );
        let ts = quoted_wire_literals(
            between(
                include_str!("../web/src/lib/api.ts"),
                "export const SSE_EVENT_TYPES = [",
                "] as const;",
            ),
            '\'',
        );
        assert!(
            !rust.is_empty() && !ts.is_empty(),
            "source markers moved — fix this test (rust: {}, ts: {})",
            rust.len(),
            ts.len(),
        );

        let rust_only: Vec<_> = rust.difference(&ts).collect();
        let ts_only: Vec<_> = ts.difference(&rust).collect();
        assert!(
            rust_only.is_empty(),
            "emitted by event_name() but no browser listener: {rust_only:?}"
        );
        assert!(
            ts_only.is_empty(),
            "browser listens but Rust never emits: {ts_only:?}"
        );
    }

    /// `ended` with an `"error: "` reason is the only thing on the wire that
    /// produces `ChatEvent::Error`. Before this, `ChatEvent::Error` was
    /// unreachable and the reducer's `case 'error'` never fired from the server,
    /// so a browser user saw a stalled turn where the TUI showed a message.
    #[test]
    fn an_ended_turn_that_failed_becomes_an_error_event() {
        let event = from_wire(SessionEventMessage::ended(
            "s1",
            "error: Communication error: LLM timeout",
        ));
        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::Error { code, message } => {
                assert_eq!(code, "turn_failed");
                // The ChatError Display prefix is stripped, same as convert.rs.
                assert_eq!(message, "LLM timeout");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    /// A clean `ended` is not an error, and the browser still sees the raw
    /// envelope through the passthrough.
    #[test]
    fn a_clean_ended_turn_stays_a_passthrough() {
        let event = from_wire(SessionEventMessage::ended("s1", "complete"));
        match ChatEvent::from_daemon_event(&event) {
            ChatEvent::SessionEvent { event_type, .. } => assert_eq!(event_type, "ended"),
            other => panic!("expected passthrough, got {other:?}"),
        }
    }
}
