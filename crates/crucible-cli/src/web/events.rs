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

    MessageComplete {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        tool_calls: Vec<ToolCallSummary>,
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

    SessionEvent {
        event_type: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallSummary {
    pub id: String,
    pub title: String,
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
            ChatEvent::SessionEvent { .. } => "session_event",
        }
    }

    pub fn from_daemon_event(event: &SessionEvent) -> Self {
        let data = &event.data;

        match event.event_type.as_str() {
            "text_delta" => ChatEvent::Token {
                content: data["content"].as_str().unwrap_or("").to_string(),
            },

            // The daemon broadcast name is `thinking` (SessionEventMessage::
            // thinking); `thinking_delta` kept for older recordings.
            "thinking" | "thinking_delta" => ChatEvent::Thinking {
                content: data["content"].as_str().unwrap_or("").to_string(),
            },

            // Canonical payload is `{call_id, tool, args}` (SessionEvent
            // Message::tool_call); id/name/arguments kept for older
            // recordings.
            "tool_call_start" | "tool_call" => ChatEvent::ToolCall {
                id: data["call_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                title: data["tool"]
                    .as_str()
                    .or_else(|| data["name"].as_str())
                    .or_else(|| data["title"].as_str())
                    .unwrap_or("")
                    .to_string(),
                arguments: data.get("args").or_else(|| data.get("arguments")).cloned(),
            },

            "tool_result" => ChatEvent::ToolResult {
                id: data["id"]
                    .as_str()
                    .or_else(|| data["call_id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                // Results are arbitrary JSON; pass strings through verbatim
                // and stringify anything else rather than dropping it.
                result: match &data["result"] {
                    serde_json::Value::Null => None,
                    serde_json::Value::String(s) => Some(s.clone()),
                    other => Some(other.to_string()),
                },
                terminate: data["terminate"].as_bool().unwrap_or(false),
            },

            "tool_result_delta" => ChatEvent::ToolResultDelta {
                id: data["id"]
                    .as_str()
                    .or_else(|| data["call_id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                delta: data["delta"]
                    .as_str()
                    .or_else(|| data["content"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "tool_result_complete" => ChatEvent::ToolResultComplete {
                id: data["id"]
                    .as_str()
                    .or_else(|| data["call_id"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "tool_result_error" => ChatEvent::ToolResultError {
                id: data["id"]
                    .as_str()
                    .or_else(|| data["call_id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                error: data["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },

            "turn_complete" | "message_complete" => ChatEvent::MessageComplete {
                id: data["message_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                content: data["full_response"]
                    .as_str()
                    .or_else(|| data["content"].as_str())
                    .unwrap_or("")
                    .to_string(),
                tool_calls: Vec::new(),
                prompt_tokens: data["prompt_tokens"].as_u64(),
                completion_tokens: data["completion_tokens"].as_u64(),
                total_tokens: data["total_tokens"].as_u64(),
                cache_read_tokens: data["cache_read_tokens"].as_u64(),
                cache_creation_tokens: data["cache_creation_tokens"].as_u64(),
            },

            "error" => ChatEvent::Error {
                code: data["code"].as_str().unwrap_or("unknown").to_string(),
                message: data["message"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },

            "interaction_requested" => ChatEvent::InteractionRequested {
                id: data["request_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                request: normalize_interaction(data),
            },

            "subagent_spawned" => ChatEvent::SubagentSpawned {
                id: data["id"].as_str().unwrap_or("").to_string(),
                prompt: data["prompt"]
                    .as_str()
                    .or_else(|| data["description"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "subagent_completed" => ChatEvent::SubagentCompleted {
                id: data["id"].as_str().unwrap_or("").to_string(),
                summary: data["summary"]
                    .as_str()
                    .or_else(|| data["result"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "subagent_failed" => ChatEvent::SubagentFailed {
                id: data["id"].as_str().unwrap_or("").to_string(),
                error: data["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },

            "delegation_spawned" => ChatEvent::DelegationSpawned {
                id: data["delegation_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                prompt: data["prompt"].as_str().unwrap_or("").to_string(),
                target_agent: data["target_agent"].as_str().map(String::from),
            },

            "delegation_completed" => ChatEvent::DelegationCompleted {
                id: data["delegation_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                summary: data["result_summary"]
                    .as_str()
                    .or_else(|| data["summary"].as_str())
                    .unwrap_or("")
                    .to_string(),
            },

            "delegation_failed" => ChatEvent::DelegationFailed {
                id: data["delegation_id"]
                    .as_str()
                    .or_else(|| data["id"].as_str())
                    .unwrap_or("")
                    .to_string(),
                error: data["error"]
                    .as_str()
                    .unwrap_or("Unknown error")
                    .to_string(),
            },

            // Daemon emits "precognition_complete"; we normalize to "precognition_result" for frontend
            "precognition_complete" => {
                let notes = data
                    .get("notes")
                    .and_then(|n| {
                        n.as_array().map(|arr| {
                            arr.iter()
                                .filter_map(|note| {
                                    let name = note
                                        .get("title")
                                        .or_else(|| note.get("name"))
                                        .and_then(|v| v.as_str())?;
                                    let relevance = note
                                        .get("relevance")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    Some(PrecognitionNote {
                                        name: name.to_string(),
                                        relevance,
                                    })
                                })
                                .collect::<Vec<_>>()
                        })
                    })
                    .unwrap_or_default();
                let notes_count = data["notes_count"]
                    .as_u64()
                    .map(|n| n as usize)
                    .unwrap_or(notes.len());

                ChatEvent::PrecognitionResult { notes_count, notes }
            }

            "context_usage" => ChatEvent::ContextUsage {
                used: data["used"].as_u64().unwrap_or(0),
                total: data["total"].as_u64().unwrap_or(0),
            },

            "mode_changed" => ChatEvent::ModeChanged {
                mode: data["mode"].as_str().unwrap_or("normal").to_string(),
            },

            _ => ChatEvent::SessionEvent {
                event_type: event.event_type.clone(),
                data: data.clone(),
            },
        }
    }
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

    /// Real tool results are arbitrary JSON (SessionEventMessage::tool_result
    /// takes a Value); non-string results must be stringified, not dropped.
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
        let event = make_event(
            "precognition_complete",
            serde_json::json!({
                "notes_count": 2,
                "notes": [
                    { "name": "Note A", "relevance": 0.9 },
                    { "name": "Note B", "relevance": 0.7 },
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
    /// + tagged PermAction). The frontend renders the FLAT shape `{kind, id,
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
}
