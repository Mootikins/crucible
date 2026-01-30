//! JSON-RPC 2.0 protocol types
//!
//! Uses serde for serialization - can swap to bincode/messagepack later.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 request ID (can be string or number)
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

// Standard JSON-RPC error codes
pub const PARSE_ERROR: i32 = -32700;
#[allow(dead_code)]
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

// ─────────────────────────────────────────────────────────────────────────────
// Daemon Event Protocol (async notifications from daemon to client)
// ─────────────────────────────────────────────────────────────────────────────

/// Session event sent from daemon to client (async, no response expected).
///
/// Events are pushed to subscribed clients when session state changes occur.
/// Clients can subscribe to specific sessions or all sessions.
#[derive(Debug, Clone, Serialize)]
pub struct SessionEventMessage {
    /// Message type (always "event")
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    /// The session ID this event belongs to
    pub session_id: String,
    /// Event type (e.g., "text_delta", "tool_call", "state_changed")
    pub event: String,
    /// Event-specific data
    pub data: Value,
}

impl SessionEventMessage {
    pub fn new(session_id: impl Into<String>, event: impl Into<String>, data: Value) -> Self {
        Self {
            msg_type: "event",
            session_id: session_id.into(),
            event: event.into(),
            data,
        }
    }

    /// Create a text delta event (streaming response)
    pub fn text_delta(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "text_delta",
            serde_json::json!({ "content": content.into() }),
        )
    }

    /// Create a state changed event
    #[allow(dead_code)]
    pub fn state_changed(session_id: impl Into<String>, state: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "state_changed",
            serde_json::json!({ "state": state.into() }),
        )
    }

    /// Create a thinking event
    pub fn thinking(session_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "thinking",
            serde_json::json!({ "content": content.into() }),
        )
    }

    /// Create a tool call event
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

    /// Create a tool result event
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

    /// Create a session ended event
    pub fn ended(session_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "ended",
            serde_json::json!({ "reason": reason.into() }),
        )
    }

    /// Create a model switched event
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
        usage: Option<&crucible_core::traits::llm::TokenUsage>,
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

    /// Create a terminal output event
    #[allow(dead_code)]
    pub fn terminal_output(
        session_id: impl Into<String>,
        stream: impl Into<String>,
        content_base64: impl Into<String>,
    ) -> Self {
        Self::new(
            session_id,
            "terminal_output",
            serde_json::json!({
                "stream": stream.into(),
                "content_base64": content_base64.into(),
            }),
        )
    }

    pub fn interaction_requested(
        session_id: impl Into<String>,
        request_id: impl Into<String>,
        request: &crucible_core::interaction::InteractionRequest,
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

    /// Serialize to JSON string with newline
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_request_with_params_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":2,"method":"kiln.open","params":{"path":"/tmp/test"}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "kiln.open");
        assert_eq!(req.id, Some(RequestId::Number(2)));
        assert_eq!(req.params["path"], "/tmp/test");
    }

    #[test]
    fn test_request_id_string_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":"req-123","method":"ping"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "ping");
        assert_eq!(req.id, Some(RequestId::String("req-123".to_string())));
    }

    #[test]
    fn test_response_with_string_id() {
        let resp = Response::success(Some(RequestId::String("req-abc".to_string())), "result");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"id\":\"req-abc\""));
    }

    #[test]
    fn test_error_with_data() {
        let data = serde_json::json!({"trace": "stack trace here"});
        let resp = Response::error_with_data(
            Some(RequestId::Number(1)),
            INTERNAL_ERROR,
            "Something went wrong",
            Some(data),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"data\""));
        assert!(json.contains("stack trace here"));
    }

    #[test]
    fn test_request_without_id_deserialization() {
        let json = r#"{"jsonrpc":"2.0","method":"ping"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "ping");
        assert_eq!(req.id, None);
    }

    #[test]
    fn test_error_codes_are_standard() {
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Session event protocol tests
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_session_event_message_serialization() {
        let event = SessionEventMessage::new(
            "chat-2025-01-08T1530-abc123",
            "text_delta",
            serde_json::json!({ "content": "Hello" }),
        );

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"event\""));
        assert!(json.contains("\"session_id\":\"chat-2025-01-08T1530-abc123\""));
        assert!(json.contains("\"event\":\"text_delta\""));
        assert!(json.contains("\"content\":\"Hello\""));
    }

    #[test]
    fn test_session_event_text_delta() {
        let event = SessionEventMessage::text_delta("chat-test", "streaming content");
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"event\":\"text_delta\""));
        assert!(json.contains("\"content\":\"streaming content\""));
    }

    #[test]
    fn test_session_event_state_changed() {
        let event = SessionEventMessage::state_changed("chat-test", "paused");
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"event\":\"state_changed\""));
        assert!(json.contains("\"state\":\"paused\""));
    }

    #[test]
    fn test_session_event_thinking() {
        let event = SessionEventMessage::thinking("agent-test", "Analyzing request...");
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"event\":\"thinking\""));
        assert!(json.contains("\"content\":\"Analyzing request...\""));
    }

    #[test]
    fn test_session_event_tool_call() {
        let event = SessionEventMessage::tool_call(
            "chat-test",
            "tc-123",
            "search",
            serde_json::json!({ "query": "test" }),
        );
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"event\":\"tool_call\""));
        assert!(json.contains("\"call_id\":\"tc-123\""));
        assert!(json.contains("\"tool\":\"search\""));
        assert!(json.contains("\"query\":\"test\""));
    }

    #[test]
    fn test_session_event_tool_result() {
        let event = SessionEventMessage::tool_result(
            "chat-test",
            "tc-123",
            "read_file",
            serde_json::json!({ "count": 5 }),
        );
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"event\":\"tool_result\""));
        assert!(json.contains("\"call_id\":\"tc-123\""));
        assert!(json.contains("\"tool\":\"read_file\""));
        assert!(json.contains("\"count\":5"));
    }

    #[test]
    fn test_session_event_ended() {
        let event = SessionEventMessage::ended("chat-test", "user_requested");
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"event\":\"ended\""));
        assert!(json.contains("\"reason\":\"user_requested\""));
    }

    #[test]
    fn test_session_event_terminal_output() {
        let event =
            SessionEventMessage::terminal_output("workflow-test", "stdout", "SGVsbG8gV29ybGQK");
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"event\":\"terminal_output\""));
        assert!(json.contains("\"stream\":\"stdout\""));
        assert!(json.contains("\"content_base64\":\"SGVsbG8gV29ybGQK\""));
    }

    #[test]
    fn test_session_event_to_json_line() {
        let event = SessionEventMessage::text_delta("chat-test", "hello");
        let line = event.to_json_line().unwrap();

        assert!(line.ends_with('\n'));
        let trimmed = line.trim_end();
        let _: serde_json::Value = serde_json::from_str(trimmed).unwrap();
    }

    #[test]
    fn test_message_complete_without_usage() {
        let event = SessionEventMessage::message_complete("s1", "m1", "response text", None);
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"event\":\"message_complete\""));
        assert!(json.contains("\"full_response\":\"response text\""));
        assert!(!json.contains("total_tokens"));
        assert!(!json.contains("prompt_tokens"));
    }

    #[test]
    fn test_message_complete_with_usage() {
        let usage = crucible_core::traits::llm::TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        let event = SessionEventMessage::message_complete("s1", "m1", "done", Some(&usage));
        let json = serde_json::to_string(&event).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["data"]["prompt_tokens"], 100);
        assert_eq!(parsed["data"]["completion_tokens"], 50);
        assert_eq!(parsed["data"]["total_tokens"], 150);
        assert_eq!(parsed["data"]["full_response"], "done");
    }
}
