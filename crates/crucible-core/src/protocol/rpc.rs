use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(u64),
    String(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<RequestId>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Response {
    pub fn success(id: Option<RequestId>, result: impl Into<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result.into()),
            error: None,
        }
    }

    pub fn error(id: Option<RequestId>, code: i32, message: impl Into<String>) -> Self {
        Self::error_with_data(id, code, message, None)
    }

    pub fn error_with_data(
        id: Option<RequestId>,
        code: i32,
        message: impl Into<String>,
        data: Option<Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data,
            }),
        }
    }
}

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEventMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub session_id: String,
    pub event: String,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

impl SessionEventMessage {
    pub fn new(session_id: impl Into<String>, event: impl Into<String>, data: Value) -> Self {
        Self {
            msg_type: "event".to_string(),
            session_id: session_id.into(),
            event: event.into(),
            data,
            timestamp: None,
            seq: None,
        }
    }

    pub fn with_timestamp(mut self) -> Self {
        self.timestamp = Some(Utc::now());
        self
    }

    pub fn text_delta(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "text_delta",
            serde_json::json!({ "content": content.into() }),
        )
    }

    pub fn user_message(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        Self::new(
            session_id,
            "user_message",
            serde_json::json!({
                "message_id": message_id.into(),
                "content": content.into(),
            }),
        )
    }
    pub fn thinking(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "thinking",
            serde_json::json!({ "content": content.into() }),
        )
    }

    pub fn tool_call(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        args: Value,
    ) -> Self {
        Self::tool_call_with_metadata(
            session_id,
            call_id,
            tool,
            args,
            None,
            None,
            None,
            Vec::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn tool_call_with_metadata(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        args: Value,
        description: Option<String>,
        source: Option<String>,
        lua_primary_arg: Option<String>,
        diffs: Vec<crate::types::acp::FileDiff>,
    ) -> Self {
        let mut data = serde_json::json!({
            "call_id": call_id.into(),
            "tool": tool.into(),
            "args": args,
        });
        if let Some(description) = description {
            data["description"] = serde_json::json!(description);
        }
        if let Some(source) = source {
            data["source"] = serde_json::json!(source);
        }
        if let Some(pa) = lua_primary_arg {
            data["lua_primary_arg"] = serde_json::json!(pa);
        }
        if !diffs.is_empty() {
            data["diffs"] = serde_json::to_value(&diffs).unwrap_or(Value::Null);
        }

        Self::new(session_id, "tool_call", data)
    }

    /// Late file-diff content for a tool call that was already announced
    /// via a prior `tool_call` event. Produced when an ACP agent (e.g.
    /// Claude Code) defers diff content until a follow-up
    /// `ToolCallUpdate` frame. Subscribers should merge `diffs` into the
    /// existing tool entry keyed by `call_id`.
    pub fn tool_call_diff_update(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        diffs: Vec<crate::types::acp::FileDiff>,
    ) -> Self {
        Self::new(
            session_id,
            "tool_call_diff_update",
            serde_json::json!({
                "call_id": call_id.into(),
                "diffs": diffs,
            }),
        )
    }

    pub fn tool_result(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        result: Value,
    ) -> Self {
        Self::tool_result_with_terminate(session_id, call_id, tool, result, false)
    }

    /// Same as [`tool_result`] but with the terminate flag carried in the
    /// payload. Used by tool handlers that signal early-stop via the
    /// conjunctive batch-terminate path; UI surfaces it as a badge.
    pub fn tool_result_with_terminate(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        result: Value,
        terminate: bool,
    ) -> Self {
        Self::new(
            session_id,
            "tool_result",
            serde_json::json!({
                "call_id": call_id.into(),
                "tool": tool.into(),
                "result": result,
                "terminate": terminate,
            }),
        )
    }

    pub fn ended(session_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "ended",
            serde_json::json!({ "reason": reason.into() }),
        )
    }

    pub fn model_switched(
        session_id: impl Into<String>,
        model_id: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self::new(
            session_id,
            "model_switched",
            serde_json::json!({
                "model_id": model_id.into(),
                "provider": provider.into(),
            }),
        )
    }

    /// Session mode changed (normal/plan/auto). `data.mode` is the field the
    /// web SSE mapper and TUI reducers read — keep the name stable.
    pub fn mode_changed(session_id: impl Into<String>, mode: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "mode_changed",
            serde_json::json!({ "mode": mode.into() }),
        )
    }

    pub fn message_complete(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        full_response: impl Into<String>,
        usage: Option<&crate::traits::llm::TokenUsage>,
    ) -> Self {
        let mut data = serde_json::json!({
            "message_id": message_id.into(),
            "full_response": full_response.into(),
        });
        if let Some(u) = usage {
            data["prompt_tokens"] = serde_json::json!(u.prompt_tokens);
            data["completion_tokens"] = serde_json::json!(u.completion_tokens);
            data["total_tokens"] = serde_json::json!(u.total_tokens);
            if let Some(cached) = u.cache_read_tokens {
                data["cache_read_tokens"] = serde_json::json!(cached);
            }
            if let Some(created) = u.cache_creation_tokens {
                data["cache_creation_tokens"] = serde_json::json!(created);
            }
        }
        Self::new(session_id, "message_complete", data)
    }

    /// A text segment that streamed before a tool call, emitted at the
    /// text→tool boundary. `message_id` is the turn id (shared with
    /// `user_message` and `message_complete`); `index` is the 0-based
    /// segment position within the turn; `content` is the segment's text
    /// (the delta accumulated since the previous boundary). Lets viewers
    /// converge on canonical per-segment bubbles across live streaming and
    /// history reload. `message_complete` still carries the WHOLE turn's
    /// accumulated text — segments are additive, not a replacement.
    pub fn segment_complete(
        session_id: impl Into<String>,
        message_id: impl Into<String>,
        index: usize,
        content: impl Into<String>,
    ) -> Self {
        Self::new(
            session_id,
            "segment_complete",
            serde_json::json!({
                "message_id": message_id.into(),
                "index": index,
                "content": content.into(),
            }),
        )
    }

    pub fn interaction_requested(
        session_id: impl Into<String>,
        request_id: impl Into<String>,
        request: &crate::interaction::InteractionRequest,
    ) -> Self {
        Self::new(
            session_id,
            "interaction_requested",
            serde_json::json!({
                "request_id": request_id.into(),
                "request": request,
            }),
        )
    }

    // ---- workflow events (Phase 3a) ----

    pub fn workflow_step_started(
        session_id: impl Into<String>,
        step_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self::new(
            session_id,
            "workflow.step_started",
            serde_json::json!({
                "step_id": step_id.into(),
                "title": title.into(),
            }),
        )
    }

    pub fn workflow_step_completed(
        session_id: impl Into<String>,
        step_id: impl Into<String>,
        output_name: Option<String>,
    ) -> Self {
        Self::new(
            session_id,
            "workflow.step_completed",
            serde_json::json!({
                "step_id": step_id.into(),
                "output_name": output_name,
            }),
        )
    }

    pub fn workflow_gate_reached(
        session_id: impl Into<String>,
        gate_id: impl Into<String>,
        title: Option<String>,
        owner: impl Into<String>,
    ) -> Self {
        Self::new(
            session_id,
            "workflow.gate_reached",
            serde_json::json!({
                "gate_id": gate_id.into(),
                "title": title,
                "owner": owner.into(),
            }),
        )
    }

    pub fn workflow_gate_approved(
        session_id: impl Into<String>,
        gate_id: impl Into<String>,
    ) -> Self {
        Self::new(
            session_id,
            "workflow.gate_approved",
            serde_json::json!({ "gate_id": gate_id.into() }),
        )
    }

    pub fn workflow_completed(session_id: impl Into<String>) -> Self {
        Self::new(session_id, "workflow.completed", serde_json::json!({}))
    }

    pub fn workflow_assessed(
        session_id: impl Into<String>,
        runnable_passed: &[crate::workflow::AssessmentOutcome],
        runnable_failed: &[crate::workflow::AssessmentOutcome],
        manual_entries: &[String],
    ) -> Self {
        Self::new(
            session_id,
            "workflow.assessed",
            serde_json::json!({
                "runnable_passed": runnable_passed,
                "runnable_failed": runnable_failed,
                "manual_entries": manual_entries,
            }),
        )
    }

    pub fn workflow_failed(
        session_id: impl Into<String>,
        reason: impl Into<String>,
        at_step: Option<String>,
    ) -> Self {
        Self::new(
            session_id,
            "workflow.failed",
            serde_json::json!({
                "reason": reason.into(),
                "at_step": at_step,
            }),
        )
    }

    pub fn workflow_cancelled(session_id: impl Into<String>) -> Self {
        Self::new(session_id, "workflow.cancelled", serde_json::json!({}))
    }

    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_session_event_message_timestamp_roundtrip() {
        let mut event = SessionEventMessage::text_delta("chat-test", "hello");
        let now = Utc::now();
        event.timestamp = Some(now);

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: SessionEventMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.timestamp, Some(now));
        assert_eq!(deserialized.session_id, "chat-test");
    }

    #[test]
    fn test_session_event_message_seq_roundtrip() {
        let mut event = SessionEventMessage::text_delta("chat-test", "hello");
        event.seq = Some(42);

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: SessionEventMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.seq, Some(42));
    }

    #[test]
    fn test_session_event_message_backward_compat() {
        let json =
            r#"{"type":"event","session_id":"s1","event":"text_delta","data":{"content":"hi"}}"#;
        let deserialized: SessionEventMessage = serde_json::from_str(json).unwrap();

        assert_eq!(deserialized.timestamp, None);
        assert_eq!(deserialized.seq, None);
        assert_eq!(deserialized.session_id, "s1");
        assert_eq!(deserialized.event, "text_delta");
    }

    #[test]
    fn test_session_event_message_omits_none_fields() {
        let event = SessionEventMessage::text_delta("chat-test", "hello");
        let json = serde_json::to_string(&event).unwrap();

        assert!(!json.contains("\"timestamp\""));
        assert!(!json.contains("\"seq\""));
    }

    #[test]
    fn test_response_success_serialization() {
        let resp = Response::success(Some(RequestId::Number(1)), "pong");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\":\"pong\""));
        assert!(json.contains("\"id\":1"));
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_response_error_serialization() {
        let resp = Response::error(
            Some(RequestId::Number(1)),
            METHOD_NOT_FOUND,
            "Unknown method",
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("-32601"));
        assert!(!json.contains("result"));
    }

    #[test]
    fn test_request_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "ping");
        assert_eq!(req.id, Some(RequestId::Number(1)));
    }

    #[test]
    fn test_session_event_text_delta() {
        let event = SessionEventMessage::text_delta("chat-test", "streaming content");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"text_delta\""));
        assert!(json.contains("\"content\":\"streaming content\""));
    }

    #[test]
    fn test_session_event_to_json_line() {
        let event = SessionEventMessage::text_delta("chat-test", "hello");
        let line = event.to_json_line().unwrap();
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn test_session_event_message_complete() {
        let event =
            SessionEventMessage::message_complete("chat-test", "msg-123", "Hello World!", None);
        let json = serde_json::to_string(&event).unwrap();
        println!("message_complete JSON: {}", json);
        assert!(json.contains("\"event\":\"message_complete\""));
        assert!(json.contains("\"full_response\":\"Hello World!\""));
        assert!(json.contains("\"message_id\":\"msg-123\""));
    }

    // ── Golden regression tests ──────────────────────────────────────

    #[test]
    fn error_code_constants_match_jsonrpc_spec() {
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
    }

    #[test]
    fn request_id_number_json_format() {
        let id = RequestId::Number(42);
        let json = serde_json::to_value(&id).unwrap();
        assert_eq!(json, serde_json::json!(42));
    }

    #[test]
    fn request_id_string_json_format() {
        let id = RequestId::String("abc".to_string());
        let json = serde_json::to_value(&id).unwrap();
        assert_eq!(json, serde_json::json!("abc"));
    }

    #[test]
    fn request_deser_string_id() {
        let json = r#"{"jsonrpc":"2.0","id":"abc-123","method":"test"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, Some(RequestId::String("abc-123".to_string())));
    }

    #[test]
    fn request_deser_no_id() {
        let json = r#"{"jsonrpc":"2.0","method":"notify"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, None);
    }

    // GOLDEN: captures current behavior — missing params deserializes to Value::Null
    #[test]
    fn request_deser_no_params() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.params, Value::Null);
    }

    #[test]
    fn response_success_omits_error() {
        let resp = Response::success(Some(RequestId::Number(1)), "ok");
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("result").is_some());
        assert!(json.get("error").is_none());
    }

    #[test]
    fn response_error_omits_result() {
        let resp = Response::error(Some(RequestId::Number(1)), INTERNAL_ERROR, "boom");
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json.get("error").is_some());
        assert!(json.get("result").is_none());
    }

    #[test]
    fn response_error_with_data() {
        let data = serde_json::json!({"detail": "bad field"});
        let resp = Response::error_with_data(
            Some(RequestId::Number(1)),
            INVALID_PARAMS,
            "invalid",
            Some(data.clone()),
        );
        let json = serde_json::to_value(&resp).unwrap();
        let err = json.get("error").unwrap();
        assert_eq!(err.get("data").unwrap(), &data);
    }

    #[test]
    fn response_error_without_data() {
        let resp = Response::error(Some(RequestId::Number(1)), INVALID_PARAMS, "invalid");
        let json = serde_json::to_value(&resp).unwrap();
        let err = json.get("error").unwrap();
        assert!(err.get("data").is_none());
    }

    #[test]
    fn event_thinking_factory() {
        let evt = SessionEventMessage::thinking("s1", "let me think...");
        assert_eq!(evt.event, "thinking");
        assert_eq!(evt.data["content"], "let me think...");
    }

    #[test]
    fn event_tool_call_factory() {
        let args = serde_json::json!({"path": "/tmp"});
        let evt = SessionEventMessage::tool_call("s1", "call-1", "read_file", args.clone());
        assert_eq!(evt.event, "tool_call");
        assert_eq!(evt.data["call_id"], "call-1");
        assert_eq!(evt.data["tool"], "read_file");
        assert_eq!(evt.data["args"], args);
    }

    #[test]
    fn event_tool_result_factory() {
        let result = serde_json::json!({"content": "file contents"});
        let evt = SessionEventMessage::tool_result("s1", "call-1", "read_file", result.clone());
        assert_eq!(evt.event, "tool_result");
        assert_eq!(evt.data["call_id"], "call-1");
        assert_eq!(evt.data["tool"], "read_file");
        assert_eq!(evt.data["result"], result);
    }

    #[test]
    fn event_ended_factory() {
        let evt = SessionEventMessage::ended("s1", "user_cancel");
        assert_eq!(evt.event, "ended");
        assert_eq!(evt.data["reason"], "user_cancel");
    }

    #[test]
    fn event_model_switched_factory() {
        let evt = SessionEventMessage::model_switched("s1", "gpt-4o", "openai");
        assert_eq!(evt.event, "model_switched");
        assert_eq!(evt.data["model_id"], "gpt-4o");
        assert_eq!(evt.data["provider"], "openai");
    }

    #[test]
    fn event_mode_changed_factory() {
        let evt = SessionEventMessage::mode_changed("s1", "plan");
        assert_eq!(evt.event, "mode_changed");
        // Wire contract: web events.rs and the SSE reducer read data["mode"].
        assert_eq!(evt.data["mode"], "plan");
    }

    #[test]
    fn event_message_complete_with_usage() {
        let usage = crate::traits::llm::TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        };
        let evt = SessionEventMessage::message_complete("s1", "msg-1", "done", Some(&usage));
        assert_eq!(evt.event, "message_complete");
        assert_eq!(evt.data["prompt_tokens"], 100);
        assert_eq!(evt.data["completion_tokens"], 50);
        assert_eq!(evt.data["total_tokens"], 150);
        assert_eq!(evt.data["message_id"], "msg-1");
        assert_eq!(evt.data["full_response"], "done");
    }

    // GOLDEN: captures current behavior — no usage means no token keys at all
    #[test]
    fn event_message_complete_without_usage() {
        let evt = SessionEventMessage::message_complete("s1", "msg-1", "done", None);
        assert_eq!(evt.event, "message_complete");
        assert!(evt.data.get("prompt_tokens").is_none());
        assert!(evt.data.get("completion_tokens").is_none());
        assert!(evt.data.get("total_tokens").is_none());
        assert_eq!(evt.data["message_id"], "msg-1");
        assert_eq!(evt.data["full_response"], "done");
    }

    #[test]
    fn event_user_message_factory() {
        let evt = SessionEventMessage::user_message("s1", "msg-42", "hello agent");
        assert_eq!(evt.event, "user_message");
        assert_eq!(evt.data["message_id"], "msg-42");
        assert_eq!(evt.data["content"], "hello agent");
    }

    #[test]
    fn tool_call_with_diffs_roundtrip() {
        use crate::types::acp::FileDiff;

        let diffs = vec![
            FileDiff::from_contents(
                "src/foo.rs",
                Some("fn old() {}\n".to_string()),
                "fn new() {}\n",
            ),
            FileDiff::new("src/bar.rs", "// brand new file\n"),
        ];

        // Wire-side construction (daemon path).
        let evt = SessionEventMessage::tool_call_with_metadata(
            "s1",
            "call-1",
            "edit",
            serde_json::json!({"path": "src/foo.rs"}),
            None,
            None,
            None,
            diffs.clone(),
        );

        // Round-trip the JSON line as the daemon emits and TUI parses.
        let line = evt.to_json_line().unwrap();
        let parsed: SessionEventMessage = serde_json::from_str(line.trim()).unwrap();

        assert_eq!(parsed.event, "tool_call");
        let parsed_diffs: Vec<FileDiff> = serde_json::from_value(
            parsed
                .data
                .get("diffs")
                .cloned()
                .expect("diffs key must round-trip"),
        )
        .expect("diffs must deserialize as Vec<FileDiff>");
        assert_eq!(parsed_diffs, diffs);
    }

    #[test]
    fn tool_call_without_diffs_omits_diffs_key() {
        // Back-compat: empty diffs must not appear in the JSON payload.
        let evt = SessionEventMessage::tool_call(
            "s1",
            "call-1",
            "read_file",
            serde_json::json!({"path": "/tmp/x"}),
        );
        let json = serde_json::to_string(&evt).unwrap();
        assert!(
            !json.contains("\"diffs\""),
            "tool_call without diffs should omit the key, got: {json}"
        );
    }

    #[test]
    fn tool_call_legacy_payload_parses_with_empty_diffs() {
        // An old daemon emitting tool_call without "diffs" must parse cleanly.
        let json = r#"{
            "type":"event",
            "session_id":"s1",
            "event":"tool_call",
            "data":{"call_id":"c","tool":"t","args":{}}
        }"#;
        let parsed: SessionEventMessage = serde_json::from_str(json).unwrap();
        assert!(parsed.data.get("diffs").is_none());
    }

    #[test]
    fn turn_event_tool_call_diffs_roundtrip_json() {
        use crate::turn::TurnEvent;
        use crate::types::acp::FileDiff;

        let diffs = vec![FileDiff::from_contents(
            "src/foo.rs",
            Some("a\n".to_string()),
            "b\n",
        )];
        let ev = TurnEvent::ToolCall {
            id: "call-1".into(),
            name: "edit".into(),
            args: serde_json::json!({"path": "src/foo.rs"}),
            diffs: diffs.clone(),
        };
        let s = serde_json::to_string(&ev).unwrap();
        let r: TurnEvent = serde_json::from_str(&s).unwrap();
        match r {
            TurnEvent::ToolCall {
                diffs: parsed_diffs,
                ..
            } => assert_eq!(parsed_diffs, diffs),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn turn_event_tool_call_legacy_json_parses_with_empty_diffs() {
        use crate::turn::TurnEvent;
        // A snapshot from before the diffs field existed.
        let json = r#"{"ToolCall":{"id":"c","name":"t","args":null}}"#;
        let r: TurnEvent = serde_json::from_str(json).unwrap();
        match r {
            TurnEvent::ToolCall { diffs, .. } => assert!(diffs.is_empty()),
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn event_msg_type_always_event() {
        let factories: Vec<SessionEventMessage> = vec![
            SessionEventMessage::text_delta("s1", "x"),
            SessionEventMessage::thinking("s1", "x"),
            SessionEventMessage::tool_call("s1", "c", "t", Value::Null),
            SessionEventMessage::tool_result("s1", "c", "t", Value::Null),
            SessionEventMessage::ended("s1", "done"),
            SessionEventMessage::model_switched("s1", "m", "p"),
            SessionEventMessage::message_complete("s1", "m", "r", None),
            SessionEventMessage::user_message("s1", "m", "c"),
        ];
        for (i, evt) in factories.iter().enumerate() {
            assert_eq!(
                evt.msg_type, "event",
                "factory index {} produced msg_type {:?} instead of \"event\"",
                i, evt.msg_type
            );
        }
    }
}
