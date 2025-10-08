// crates/crucible-mcp/tests/test_protocol_edge_cases.rs
//
// Edge case tests for MCP protocol that specifically exercise
// the notification vs request disambiguation logic

mod test_helpers;

use crucible_mcp::protocol::{JsonRpcNotification, JsonRpcRequest, McpProtocolHandler};
use serde_json::Value;
use test_helpers::*;

// =============================================================================
// EDGE CASES: ID Field Variations
// =============================================================================

#[tokio::test]
async fn test_id_field_with_zero() {
    // Zero is a valid id
    let mut handler = create_initialized_handler().await;

    let request = create_request(0, "tools/list", None);
    let result = handler.handle_message(&request).await.unwrap().unwrap();

    let response: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["id"], 0);
}

#[tokio::test]
async fn test_id_field_with_negative_number() {
    // Negative numbers are valid ids
    let mut handler = create_initialized_handler().await;

    let request = create_request(-42, "tools/list", None);
    let result = handler.handle_message(&request).await.unwrap().unwrap();

    let response: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["id"], -42);
}

#[tokio::test]
async fn test_id_field_with_large_number() {
    // Large numbers should work
    let mut handler = create_initialized_handler().await;

    let request = create_request(999999999, "tools/list", None);
    let result = handler.handle_message(&request).await.unwrap().unwrap();

    let response: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["id"], 999999999);
}

#[tokio::test]
async fn test_id_field_with_empty_string() {
    // Empty string is a valid id
    let mut handler = create_initialized_handler().await;

    let request = r#"{
        "jsonrpc": "2.0",
        "id": "",
        "method": "tools/list"
    }"#;

    let result = handler.handle_message(request).await.unwrap().unwrap();

    let response: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["id"], "");
}

#[tokio::test]
async fn test_id_field_with_uuid() {
    // UUID strings are common id formats
    let mut handler = create_initialized_handler().await;

    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    let request = format!(
        r#"{{
        "jsonrpc": "2.0",
        "id": "{}",
        "method": "tools/list"
    }}"#,
        uuid
    );

    let result = handler.handle_message(&request).await.unwrap().unwrap();

    let response: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["id"], uuid);
}

#[tokio::test]
async fn test_explicit_null_vs_missing_id() {
    // This is the key distinction:
    // {"id": null} has an id field (request)
    // {} has no id field (notification)

    let with_null = r#"{"jsonrpc": "2.0", "id": null, "method": "test"}"#;
    let without_id = r#"{"jsonrpc": "2.0", "method": "test"}"#;

    let json_with_null: Value = serde_json::from_str(with_null).unwrap();
    let json_without_id: Value = serde_json::from_str(without_id).unwrap();

    // With null: field exists
    assert!(json_with_null.get("id").is_some());
    assert!(json_with_null["id"].is_null());

    // Without id: field doesn't exist
    assert!(json_without_id.get("id").is_none());
}

// =============================================================================
// EDGE CASES: Method Names
// =============================================================================

#[tokio::test]
async fn test_method_with_slashes() {
    // Methods can have slashes (like "tools/list")
    let mut handler = create_initialized_handler().await;

    let request = create_request(1, "tools/list", None);
    let result = handler.handle_message(&request).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_method_with_dots() {
    // Methods could theoretically have dots
    let mut handler = create_initialized_handler().await;

    let request = create_request(1, "unknown.method.name", None);
    let result = handler.handle_message(&request).await.unwrap().unwrap();

    let response: Value = serde_json::from_str(&result).unwrap();
    assert!(response["error"].is_object());
    assert_eq!(response["error"]["code"], -32601); // Method not found
}

#[tokio::test]
async fn test_notification_method_with_slashes() {
    // Notifications can also have slashes
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let notification = create_notification("notifications/cancelled", None);
    let result = handler.handle_message(&notification).await;

    assert!(result.is_ok());
}

// =============================================================================
// EDGE CASES: Parameter Variations
// =============================================================================

#[tokio::test]
async fn test_params_as_array() {
    // JSON-RPC allows params to be an array or object
    let mut handler = create_initialized_handler().await;

    let request = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": []
    }"#;

    // This should work (even though our implementation expects object params)
    let result = handler.handle_message(request).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_params_as_null() {
    // Params can be null
    let mut handler = create_initialized_handler().await;

    let request = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": null
    }"#;

    let result = handler.handle_message(request).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_missing_params() {
    // Params field can be omitted entirely
    let mut handler = create_initialized_handler().await;

    let request = create_request(1, "tools/list", None);
    let result = handler.handle_message(&request).await;

    assert!(result.is_ok());
}

// =============================================================================
// EDGE CASES: Field Ordering
// =============================================================================

#[tokio::test]
async fn test_fields_in_different_order() {
    // JSON object field order shouldn't matter
    let mut handler = create_initialized_handler().await;

    let request = r#"{
        "method": "tools/list",
        "id": 1,
        "jsonrpc": "2.0"
    }"#;

    let result = handler.handle_message(request).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_notification_fields_in_different_order() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let notification = r#"{
        "method": "notifications/cancelled",
        "jsonrpc": "2.0"
    }"#;

    let result = handler.handle_message(notification).await;
    assert!(result.is_ok());
}

// =============================================================================
// EDGE CASES: Extra Fields
// =============================================================================

#[tokio::test]
async fn test_request_with_extra_fields() {
    // Extra fields should be ignored
    let mut handler = create_initialized_handler().await;

    let request = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "extra_field": "should be ignored",
        "another_extra": 123
    }"#;

    let result = handler.handle_message(request).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_notification_with_extra_fields() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let notification = r#"{
        "jsonrpc": "2.0",
        "method": "initialized",
        "extra_field": "should be ignored"
    }"#;

    let result = handler.handle_message(notification).await;
    assert!(result.is_ok());
}

// =============================================================================
// EDGE CASES: Unicode and Special Characters
// =============================================================================

#[tokio::test]
async fn test_unicode_in_method_name() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let notification = r#"{
        "jsonrpc": "2.0",
        "method": "test_😀_method"
    }"#;

    let result = handler.handle_message(notification).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_unicode_in_string_id() {
    let mut handler = create_initialized_handler().await;

    let request = r#"{
        "jsonrpc": "2.0",
        "id": "request_😀_123",
        "method": "tools/list"
    }"#;

    let result = handler.handle_message(request).await.unwrap().unwrap();
    let response: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(response["id"], "request_😀_123");
}

// =============================================================================
// EDGE CASES: Whitespace Variations
// =============================================================================

#[tokio::test]
async fn test_compact_json() {
    // No whitespace
    let mut handler = create_initialized_handler().await;

    let request = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let result = handler.handle_message(request).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_json_with_lots_of_whitespace() {
    let mut handler = create_initialized_handler().await;

    let request = r#"
    {
        "jsonrpc"  :  "2.0"  ,
        "id"       :  1      ,
        "method"   :  "tools/list"
    }
    "#;

    let result = handler.handle_message(request).await;
    assert!(result.is_ok());
}

// =============================================================================
// EDGE CASES: Deserialization Ambiguity
// =============================================================================

#[tokio::test]
async fn test_demonstrate_deserialization_ambiguity() {
    // This test demonstrates that the same JSON can deserialize as
    // both JsonRpcRequest and JsonRpcNotification when id is missing

    let messages = vec![
        r#"{"jsonrpc": "2.0", "method": "test"}"#,
        r#"{"jsonrpc": "2.0", "method": "initialized"}"#,
        r#"{"jsonrpc": "2.0", "method": "notifications/cancelled"}"#,
    ];

    for msg in messages {
        // Can deserialize as request (with id: None)
        let as_request: Result<JsonRpcRequest, _> = serde_json::from_str(msg);
        assert!(as_request.is_ok());
        assert!(as_request.unwrap().id.is_none());

        // Can also deserialize as notification
        let as_notification: Result<JsonRpcNotification, _> = serde_json::from_str(msg);
        assert!(as_notification.is_ok());

        // This is why we need to check the raw JSON structure!
        let json: Value = serde_json::from_str(msg).unwrap();
        assert!(json.get("id").is_none());
    }
}

#[tokio::test]
async fn test_request_with_id_only_deserializes_as_request() {
    let messages = vec![
        r#"{"jsonrpc": "2.0", "id": 1, "method": "test"}"#,
        r#"{"jsonrpc": "2.0", "id": "abc", "method": "test"}"#,
        r#"{"jsonrpc": "2.0", "id": null, "method": "test"}"#,
    ];

    for msg in messages {
        // Can deserialize as request
        let as_request: Result<JsonRpcRequest, _> = serde_json::from_str(msg);
        assert!(as_request.is_ok());

        // Note: JsonRpcNotification can also deserialize with extra fields (serde ignores them)
        // This is why we must check the raw JSON structure, not rely on deserialization
        let as_notification: Result<JsonRpcNotification, _> = serde_json::from_str(msg);
        assert!(
            as_notification.is_ok(),
            "Serde allows extra fields, demonstrating the ambiguity"
        );

        // Has id field in raw JSON
        let json: Value = serde_json::from_str(msg).unwrap();
        assert!(json.get("id").is_some());
    }
}

// =============================================================================
// EDGE CASES: Version String Variations
// =============================================================================

#[tokio::test]
async fn test_different_jsonrpc_version() {
    // Our implementation expects "2.0", but let's test other values
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // JSON-RPC 1.0 format (should fail or be handled gracefully)
    let request = r#"{
        "jsonrpc": "1.0",
        "id": 1,
        "method": "tools/list"
    }"#;

    let result = handler.handle_message(request).await;
    // Should still parse (serde doesn't validate the version)
    assert!(result.is_ok());
}

// =============================================================================
// EDGE CASES: Concurrent Message Handling
// =============================================================================

#[tokio::test]
async fn test_alternating_requests_and_notifications() {
    let mut handler = create_initialized_handler().await;

    for i in 0..50 {
        if i % 2 == 0 {
            // Request
            let request = create_request(i, "tools/list", None);
            let result = handler.handle_message(&request).await;
            assert!(result.is_ok());
        } else {
            // Notification
            let notification = create_notification("notifications/cancelled", None);
            let result = handler.handle_message(&notification).await;
            assert!(result.is_ok());
        }
    }
}

// =============================================================================
// EDGE CASES: State Management
// =============================================================================

#[tokio::test]
async fn test_multiple_initialization_attempts() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // First initialization
    let init1 = create_initialize_request(1, "client1", "2024-11-05");
    handler.handle_message(&init1).await.unwrap();

    let initialized1 = create_notification("initialized", None);
    handler.handle_message(&initialized1).await.unwrap();

    // Try to initialize again (should still work, just re-initialize)
    let init2 = create_initialize_request(2, "client2", "2024-11-05");
    let result = handler.handle_message(&init2).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_initialized_notification_can_be_sent_multiple_times() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // Initialize first
    let init = create_initialize_request(1, "client", "2024-11-05");
    handler.handle_message(&init).await.unwrap();

    // Send initialized multiple times
    for _ in 0..3 {
        let notification = create_notification("initialized", None);
        let result = handler.handle_message(&notification).await;
        assert!(result.is_ok());
    }
}
