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

#[derive(Debug, Clone, Serialize)]
pub struct SessionEventMessage {
    #[serde(rename = "type")]
    pub msg_type: &'static str,
    pub session_id: String,
    pub event: String,
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

    #[allow(dead_code)]
    pub fn state_changed(session_id: impl Into<String>, state: impl Into<String>) -> Self {
        Self::new(
            session_id,
            "state_changed",
            serde_json::json!({ "state": state.into() }),
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
}
