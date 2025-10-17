// tests/test_protocol.rs
//! Comprehensive tests for protocol.rs

use crucible_mcp::protocol::{McpProtocolHandler, JsonRpcRequest, JsonRpcResponse, JsonRpcNotification};
use serde_json::json;

#[tokio::test]
async fn test_protocol_handler_creation() {
    let handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());
    drop(handler);
}

#[tokio::test]
async fn test_handle_initialize_request() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let init_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    });

    let response = handler.handle_message(&init_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert_eq!(response_json["jsonrpc"], "2.0");
    assert_eq!(response_json["id"], 1);
    assert!(response_json["result"].is_object());
}

#[tokio::test]
async fn test_handle_initialize_request_missing_params() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let init_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize"
    });

    let response = handler.handle_message(&init_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert!(response_json["error"].is_object());
    assert_eq!(response_json["error"]["code"], -32602);
}

#[tokio::test]
async fn test_handle_initialized_notification() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    // First initialize
    let init_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    });
    handler.handle_message(&init_message.to_string()).await.unwrap();

    // Then send initialized notification
    let initialized_message = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });

    let response = handler.handle_message(&initialized_message.to_string()).await;
    assert!(response.is_ok());

    // Notifications don't require a response
    let response_str = response.unwrap();
    assert!(response_str.is_none());
}

#[tokio::test]
async fn test_handle_list_tools_before_init() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let list_tools_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list"
    });

    let response = handler.handle_message(&list_tools_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert!(response_json["error"].is_object());
    assert_eq!(response_json["error"]["code"], -32002);
}

#[tokio::test]
async fn test_handle_list_tools_after_init() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    // Initialize first
    let init_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    });
    handler.handle_message(&init_message.to_string()).await.unwrap();

    // Send initialized notification
    let initialized_message = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    handler.handle_message(&initialized_message.to_string()).await.unwrap();

    // Now list tools
    let list_tools_message = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    });

    let response = handler.handle_message(&list_tools_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert_eq!(response_json["jsonrpc"], "2.0");
    assert!(response_json["result"].is_object());
    assert!(response_json["result"]["tools"].is_array());
}

#[tokio::test]
async fn test_handle_unknown_method() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let unknown_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "unknown/method"
    });

    let response = handler.handle_message(&unknown_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert!(response_json["error"].is_object());
    assert_eq!(response_json["error"]["code"], -32601);
    assert_eq!(response_json["error"]["message"], "Method not found");
}

#[tokio::test]
async fn test_handle_invalid_json() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let invalid_message = "not valid json{";

    let response = handler.handle_message(invalid_message).await;
    assert!(response.is_err());
}

#[tokio::test]
async fn test_handle_list_prompts() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let list_prompts_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "prompts/list"
    });

    let response = handler.handle_message(&list_prompts_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert_eq!(response_json["jsonrpc"], "2.0");
    assert!(response_json["result"]["prompts"].is_array());
}

#[tokio::test]
async fn test_handle_list_resources() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let list_resources_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "resources/list"
    });

    let response = handler.handle_message(&list_resources_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert_eq!(response_json["jsonrpc"], "2.0");
    assert!(response_json["result"]["resources"].is_array());
}

#[tokio::test]
async fn test_handle_call_tool_before_init() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let call_tool_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {
            "name": "search_by_tags",
            "arguments": {"tags": ["rust"]}
        }
    });

    let response = handler.handle_message(&call_tool_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert!(response_json["error"].is_object());
    assert_eq!(response_json["error"]["code"], -32002);
}

#[tokio::test]
async fn test_handle_call_tool_missing_params() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    // Initialize first
    let init_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    });
    handler.handle_message(&init_message.to_string()).await.unwrap();

    // Send initialized notification
    let initialized_message = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    handler.handle_message(&initialized_message.to_string()).await.unwrap();

    // Call tool without params
    let call_tool_message = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call"
    });

    let response = handler.handle_message(&call_tool_message.to_string()).await;
    assert!(response.is_ok());

    let response_str = response.unwrap();
    assert!(response_str.is_some());

    let response_json: serde_json::Value = serde_json::from_str(&response_str.unwrap()).unwrap();
    assert!(response_json["error"].is_object());
    assert_eq!(response_json["error"]["code"], -32602);
}

#[tokio::test]
async fn test_handle_cancelled_notification() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let cancelled_message = json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {
            "requestId": "123"
        }
    });

    let response = handler.handle_message(&cancelled_message.to_string()).await;
    assert!(response.is_ok());

    // Notifications don't require a response
    let response_str = response.unwrap();
    assert!(response_str.is_none());
}

#[tokio::test]
async fn test_json_rpc_request_serialization() {
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "test_method".to_string(),
        params: Some(json!({"key": "value"})),
    };

    let json = serde_json::to_string(&request);
    assert!(json.is_ok());

    let deserialized: Result<JsonRpcRequest, _> = serde_json::from_str(&json.unwrap());
    assert!(deserialized.is_ok());
}

#[tokio::test]
async fn test_json_rpc_response_serialization() {
    let response = JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        result: Some(json!({"success": true})),
        error: None,
    };

    let json = serde_json::to_string(&response);
    assert!(json.is_ok());

    let deserialized: Result<JsonRpcResponse, _> = serde_json::from_str(&json.unwrap());
    assert!(deserialized.is_ok());
}

#[tokio::test]
async fn test_json_rpc_notification_serialization() {
    let notification = JsonRpcNotification {
        jsonrpc: "2.0".to_string(),
        method: "test_notification".to_string(),
        params: Some(json!({"key": "value"})),
    };

    let json = serde_json::to_string(&notification);
    assert!(json.is_ok());

    let deserialized: Result<JsonRpcNotification, _> = serde_json::from_str(&json.unwrap());
    assert!(deserialized.is_ok());
}

#[tokio::test]
async fn test_initialize_response_structure() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let init_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    });

    let response = handler.handle_message(&init_message.to_string()).await.unwrap().unwrap();
    let response_json: serde_json::Value = serde_json::from_str(&response).unwrap();

    // Check server info
    assert_eq!(response_json["result"]["serverInfo"]["name"], "test-server");
    assert_eq!(response_json["result"]["serverInfo"]["version"], "1.0.0");

    // Check protocol version
    assert_eq!(response_json["result"]["protocolVersion"], "2024-11-05");

    // Check capabilities
    assert!(response_json["result"]["capabilities"].is_object());
    assert!(response_json["result"]["capabilities"]["tools"].is_object());
}

#[tokio::test]
async fn test_multiple_initialize_calls() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let init_message = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    });

    // First initialize
    let response1 = handler.handle_message(&init_message.to_string()).await;
    assert!(response1.is_ok());

    // Second initialize (should still work)
    let response2 = handler.handle_message(&init_message.to_string()).await;
    assert!(response2.is_ok());
}

#[tokio::test]
async fn test_empty_message() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let response = handler.handle_message("").await;
    assert!(response.is_err());
}

#[tokio::test]
async fn test_message_with_no_method() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let message = json!({
        "jsonrpc": "2.0",
        "id": 1
    });

    let response = handler.handle_message(&message.to_string()).await;
    assert!(response.is_err());
}
