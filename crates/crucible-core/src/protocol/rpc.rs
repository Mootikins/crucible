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
        Self::new(
            session_id,
            "tool_call",
            serde_json::json!({
                "call_id": call_id.into(),
                "tool": tool.into(),
                "args": args,
            }),
        )
    }

    pub fn tool_result(
        session_id: impl Into<String>,
        call_id: impl Into<String>,
        tool: impl Into<String>,
        result: Value,
    ) -> Self {
        Self::new(
            session_id,
            "tool_result",
            serde_json::json!({
                "call_id": call_id.into(),
                "tool": tool.into(),
                "result": result,
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
        }
        Self::new(session_id, "message_complete", data)
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
    fn event_message_complete_with_usage() {
        let usage = crate::traits::llm::TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
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
