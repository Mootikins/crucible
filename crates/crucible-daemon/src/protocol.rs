use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// Create a success response
    pub fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: Value, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
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
    use serde_json::json;

    #[test]
    fn test_request_serialization() {
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: json!(1),
            method: "ping".to_string(),
            params: None,
        };

        let json_str = serde_json::to_string(&req).unwrap();
        assert!(json_str.contains(r#""jsonrpc":"2.0""#));
        assert!(json_str.contains(r#""method":"ping""#));
        assert!(json_str.contains(r#""id":1"#));
    }

    #[test]
    fn test_request_deserialization() {
        let json_str = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let req: Request = serde_json::from_str(json_str).unwrap();

        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "ping");
        assert_eq!(req.id, json!(1));
        assert_eq!(req.params, None);
    }

    #[test]
    fn test_request_with_params() {
        let json_str = r#"{"jsonrpc":"2.0","id":1,"method":"test","params":{"key":"value"}}"#;
        let req: Request = serde_json::from_str(json_str).unwrap();

        assert_eq!(req.params, Some(json!({"key": "value"})));
    }

    #[test]
    fn test_success_response() {
        let resp = Response::success(json!(1), json!("pong"));

        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, json!(1));
        assert_eq!(resp.result, Some(json!("pong")));
        assert_eq!(resp.error, None);

        let json_str = serde_json::to_string(&resp).unwrap();
        assert!(json_str.contains(r#""result":"pong""#));
        assert!(!json_str.contains("error"));
    }

    #[test]
    fn test_error_response() {
        let error = RpcError::new(METHOD_NOT_FOUND, "Method not found");
        let resp = Response::error(json!(1), error);

        assert_eq!(resp.jsonrpc, "2.0");
        assert_eq!(resp.id, json!(1));
        assert_eq!(resp.result, None);
        assert!(resp.error.is_some());

        let err = resp.error.unwrap();
        assert_eq!(err.code, METHOD_NOT_FOUND);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn test_error_with_data() {
        let error = RpcError::new(INTERNAL_ERROR, "Internal error")
            .with_data(json!({"details": "stack trace"}));

        assert_eq!(error.code, INTERNAL_ERROR);
        assert_eq!(error.data, Some(json!({"details": "stack trace"})));
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(PARSE_ERROR, -32700);
        assert_eq!(INVALID_REQUEST, -32600);
        assert_eq!(METHOD_NOT_FOUND, -32601);
        assert_eq!(INVALID_PARAMS, -32602);
        assert_eq!(INTERNAL_ERROR, -32603);
    }

    #[test]
    fn test_round_trip_request() {
        let original = Request {
            jsonrpc: "2.0".to_string(),
            id: json!("test-id"),
            method: "shutdown".to_string(),
            params: Some(json!({"graceful": true})),
        };

        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Request = serde_json::from_str(&json_str).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_round_trip_response() {
        let original = Response::success(json!(42), json!({"status": "ok"}));

        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Response = serde_json::from_str(&json_str).unwrap();

        assert_eq!(original, deserialized);
    }
}
