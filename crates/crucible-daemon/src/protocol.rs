//! JSON-RPC 2.0 protocol types
//!
//! Uses serde for serialization - can swap to bincode/messagepack later.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl Response {
    pub fn success(id: Option<u64>, result: impl Into<Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result.into()),
            error: None,
        }
    }

    pub fn error(id: Option<u64>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError { code, message: message.into() }),
        }
    }
}

// Standard JSON-RPC error codes
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_response_success_serialization() {
        let resp = Response::success(Some(1), "pong");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\":\"pong\""));
        assert!(json.contains("\"id\":1"));
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_response_error_serialization() {
        let resp = Response::error(Some(1), METHOD_NOT_FOUND, "Unknown method");
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
        assert_eq!(req.id, Some(1));
    }

    #[test]
    fn test_request_with_params_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":2,"method":"kiln.open","params":{"path":"/tmp/test"}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "kiln.open");
        assert_eq!(req.id, Some(2));
        assert_eq!(req.params["path"], "/tmp/test");
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
}
