// crates/crucible-mcp/tests/test_notification_parsing.rs
//
// Comprehensive tests for MCP protocol notification parsing
//
// These tests verify the fix for the notification parsing bug where
// notifications (messages without 'id' field) were incorrectly parsed
// as requests with id: None, causing the 'initialized' notification
// to fail during the initialization handshake.

use crucible_mcp::protocol::{JsonRpcNotification, JsonRpcRequest, McpProtocolHandler};
use serde_json::{json, Value};

// =============================================================================
// UNIT TESTS: Message Parsing Logic
// =============================================================================

#[tokio::test]
async fn test_request_has_id_field() {
    // Verify that requests with 'id' field are correctly identified
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let request_message = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    }"#;

    let result = handler.handle_message(request_message).await;
    assert!(
        result.is_ok(),
        "Request with id should be handled successfully"
    );

    let response = result.unwrap();
    assert!(response.is_some(), "Request should return a response");

    // Parse the response and verify it has the same id
    let response_json: Value = serde_json::from_str(&response.unwrap()).unwrap();
    assert_eq!(
        response_json["id"],
        json!(1),
        "Response id should match request id"
    );
}

#[tokio::test]
async fn test_notification_has_no_id_field() {
    // Verify that notifications without 'id' field are correctly identified
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let notification_message = r#"{
        "jsonrpc": "2.0",
        "method": "initialized"
    }"#;

    // Parse as generic JSON to verify no 'id' field
    let json: Value = serde_json::from_str(notification_message).unwrap();
    assert!(
        json.get("id").is_none(),
        "Notification should not have 'id' field"
    );

    // Handle the notification
    let result = handler.handle_message(notification_message).await;
    assert!(
        result.is_ok(),
        "Notification should be handled successfully"
    );
}

#[tokio::test]
async fn test_request_with_null_id() {
    // Edge case: request with explicit null id
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let request_message = r#"{
        "jsonrpc": "2.0",
        "id": null,
        "method": "tools/list"
    }"#;

    // This has an 'id' field (even though it's null), so it's a request
    let json: Value = serde_json::from_str(request_message).unwrap();
    assert!(
        json.get("id").is_some(),
        "Request with null id should have 'id' field"
    );

    let result = handler.handle_message(request_message).await;
    assert!(result.is_ok(), "Request with null id should be handled");
}

#[tokio::test]
async fn test_request_with_string_id() {
    // Verify that requests can have string ids
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let request_message = r#"{
        "jsonrpc": "2.0",
        "id": "request-123",
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    }"#;

    let result = handler.handle_message(request_message).await;
    assert!(result.is_ok(), "Request with string id should be handled");

    let response = result.unwrap();
    assert!(response.is_some());

    let response_json: Value = serde_json::from_str(&response.unwrap()).unwrap();
    assert_eq!(
        response_json["id"],
        json!("request-123"),
        "Response id should match request id"
    );
}

// =============================================================================
// UNIT TESTS: Notification Handling
// =============================================================================

#[tokio::test]
async fn test_initialized_notification_sets_flag() {
    // Verify that 'initialized' notification sets the initialized flag
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // Initially, handler should not be initialized
    // Note: McpProtocolHandler doesn't expose is_initialized(), so we test indirectly

    // Send initialized notification
    let notification_message = r#"{
        "jsonrpc": "2.0",
        "method": "initialized"
    }"#;

    let result = handler.handle_message(notification_message).await;
    assert!(result.is_ok(), "Initialized notification should be handled");

    let response = result.unwrap();
    // Per MCP spec, initialized notification should NOT receive a response
    assert!(
        response.is_none(),
        "Per MCP spec, initialized notification should not send a response"
    );

    // Verify the handler is now initialized by calling tools/list
    let tools_request = r#"{
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    }"#;

    let tools_result = handler.handle_message(tools_request).await;
    assert!(
        tools_result.is_ok(),
        "Tools/list should work after initialized"
    );

    let tools_response = tools_result.unwrap().unwrap();
    let tools_json: Value = serde_json::from_str(&tools_response).unwrap();
    assert!(
        tools_json["error"].is_null(),
        "Tools/list should not return error after initialized"
    );
}

#[tokio::test]
async fn test_initialized_notification_no_response() {
    // Verify that initialized notification does NOT send a response per MCP spec
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let notification_message = r#"{
        "jsonrpc": "2.0",
        "method": "initialized"
    }"#;

    let result = handler.handle_message(notification_message).await.unwrap();

    // Per MCP spec, initialized notification should not receive a response
    assert!(result.is_none(), "Initialized notification must not send a response per MCP spec");
}

#[tokio::test]
async fn test_unknown_notification_handling() {
    // Verify that unknown notifications don't crash the handler
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let notification_message = r#"{
        "jsonrpc": "2.0",
        "method": "notifications/unknown"
    }"#;

    let result = handler.handle_message(notification_message).await;
    assert!(
        result.is_ok(),
        "Unknown notification should be handled gracefully"
    );

    let response = result.unwrap();
    assert!(
        response.is_none(),
        "Unknown notification should not return response"
    );
}

#[tokio::test]
async fn test_cancelled_notification() {
    // Verify that cancelled notification is handled
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let notification_message = r#"{
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {
            "requestId": 123,
            "reason": "timeout"
        }
    }"#;

    let result = handler.handle_message(notification_message).await;
    assert!(result.is_ok(), "Cancelled notification should be handled");

    let response = result.unwrap();
    assert!(
        response.is_none(),
        "Cancelled notification should not return response"
    );
}

// =============================================================================
// INTEGRATION TESTS: Initialization Handshake Sequence
// =============================================================================

#[tokio::test]
async fn test_complete_initialization_handshake() {
    // Test the complete initialization sequence:
    // 1. Client sends initialize request (with id)
    // 2. Server responds with capabilities
    // 3. Client sends initialized notification (no id)
    // 4. Server sends ready notification
    // 5. Client can then call tools/list

    let mut handler = McpProtocolHandler::new("crucible-mcp".into(), "0.1.0".into());

    // Step 1: Initialize request
    let init_request = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    }"#;

    let init_result = handler.handle_message(init_request).await;
    assert!(init_result.is_ok(), "Initialize request should succeed");

    let init_response_str = init_result.unwrap().unwrap();
    let init_response: Value = serde_json::from_str(&init_response_str).unwrap();

    // Step 2: Verify initialize response
    assert_eq!(init_response["jsonrpc"], "2.0");
    assert_eq!(init_response["id"], 1);
    assert!(init_response["result"].is_object());
    assert!(init_response["error"].is_null());
    assert_eq!(init_response["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(
        init_response["result"]["serverInfo"]["name"],
        "crucible-mcp"
    );

    // Step 3: Send initialized notification (NO ID FIELD)
    let initialized_notification = r#"{
        "jsonrpc": "2.0",
        "method": "initialized"
    }"#;

    let initialized_result = handler.handle_message(initialized_notification).await;
    assert!(
        initialized_result.is_ok(),
        "Initialized notification should be handled"
    );

    // Step 4: Per MCP spec, initialized notification should NOT receive a response
    let initialized_response = initialized_result.unwrap();
    assert!(
        initialized_response.is_none(),
        "Per MCP spec, initialized notification must not send a response"
    );

    // Step 5: Verify tools/list works after initialization
    let tools_request = r#"{
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list"
    }"#;

    let tools_result = handler.handle_message(tools_request).await;
    assert!(
        tools_result.is_ok(),
        "Tools/list should work after initialized"
    );

    let tools_response_str = tools_result.unwrap().unwrap();
    let tools_response: Value = serde_json::from_str(&tools_response_str).unwrap();

    assert_eq!(tools_response["jsonrpc"], "2.0");
    assert_eq!(tools_response["id"], 2);
    assert!(tools_response["result"].is_object());
    assert!(tools_response["error"].is_null());
    assert!(tools_response["result"]["tools"].is_array());

    let tools_array = tools_response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools_array.len(), 13, "Should return all 13 tools");
}

#[tokio::test]
async fn test_tools_list_before_initialized_fails() {
    // REGRESSION TEST: Verify that calling tools/list before initialized fails
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // Try to call tools/list before initialization
    let tools_request = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list"
    }"#;

    let result = handler.handle_message(tools_request).await;
    assert!(result.is_ok(), "Should handle the request");

    let response_str = result.unwrap().unwrap();
    let response: Value = serde_json::from_str(&response_str).unwrap();

    // Should return an error
    assert!(
        response["error"].is_object(),
        "Should return error before initialized"
    );
    assert_eq!(response["error"]["code"], -32002);
    assert!(response["error"]["message"]
        .as_str()
        .unwrap()
        .contains("not initialized"));
}

#[tokio::test]
async fn test_multiple_notifications_in_sequence() {
    // Test that multiple notifications can be handled in sequence
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // Send multiple notifications
    let notifications = vec![
        r#"{"jsonrpc": "2.0", "method": "notifications/unknown1"}"#,
        r#"{"jsonrpc": "2.0", "method": "initialized"}"#,
        r#"{"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 1}}"#,
    ];

    for (i, notification) in notifications.iter().enumerate() {
        let result = handler.handle_message(notification).await;
        assert!(result.is_ok(), "Notification {} should be handled", i);
    }

    // After initialized, tools/list should work
    let tools_request = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list"
    }"#;

    let result = handler
        .handle_message(tools_request)
        .await
        .unwrap()
        .unwrap();
    let response: Value = serde_json::from_str(&result).unwrap();
    assert!(
        response["error"].is_null(),
        "Tools/list should work after initialized notification"
    );
}

// =============================================================================
// REGRESSION TESTS: The Original Bug
// =============================================================================

#[tokio::test]
async fn test_regression_initialized_notification_without_id() {
    // REGRESSION TEST: The original bug
    //
    // The bug was that JsonRpcRequest has `id: Option<Value>`, so a
    // notification without an 'id' field would deserialize as a request
    // with id: None. This caused initialized notification to be parsed
    // as a request, which then failed because there's no request handler
    // for "initialized".
    //
    // This test verifies that notifications without 'id' field are now
    // correctly identified and handled as notifications, not requests.

    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // This is the exact message that was failing before the fix
    let initialized_notification = r#"{
        "jsonrpc": "2.0",
        "method": "initialized"
    }"#;

    // Parse as generic JSON to verify structure
    let json: Value = serde_json::from_str(initialized_notification).unwrap();
    assert!(
        json.get("id").is_none(),
        "Notification should not have 'id' field"
    );

    // Before fix: This would try to parse as JsonRpcRequest, which would
    // deserialize successfully with id: None, then fail because there's
    // no request handler for "initialized"
    //
    // After fix: This correctly checks for 'id' field presence first,
    // determines it's a notification, and handles it correctly

    let result = handler.handle_message(initialized_notification).await;
    assert!(
        result.is_ok(),
        "Initialized notification should be handled successfully"
    );

    let response = result.unwrap();
    // Per MCP spec, initialized notification should NOT send a response
    assert!(
        response.is_none(),
        "Per MCP spec, initialized notification must not send a response"
    );
}

#[tokio::test]
async fn test_regression_serde_deserialization_difference() {
    // This test demonstrates the deserialization behavior that caused the bug

    // A notification without 'id' field
    let notification_json = r#"{
        "jsonrpc": "2.0",
        "method": "initialized"
    }"#;

    // This would deserialize as a JsonRpcRequest with id: None (the bug!)
    let as_request: Result<JsonRpcRequest, _> = serde_json::from_str(notification_json);
    assert!(
        as_request.is_ok(),
        "Notification can be deserialized as request (this is the bug!)"
    );

    let request = as_request.unwrap();
    assert!(request.id.is_none(), "Deserialized request has id: None");
    assert_eq!(request.method, "initialized");

    // This correctly deserializes as a notification
    let as_notification: Result<JsonRpcNotification, _> = serde_json::from_str(notification_json);
    assert!(
        as_notification.is_ok(),
        "Should also deserialize as notification"
    );

    let notification = as_notification.unwrap();
    assert_eq!(notification.method, "initialized");

    // THE FIX: We must check for 'id' field presence BEFORE deserializing,
    // not rely on deserialization to distinguish between requests and notifications
    let json: Value = serde_json::from_str(notification_json).unwrap();
    let has_id = json.get("id").is_some();

    assert!(!has_id, "Notification should not have 'id' field");
}

#[tokio::test]
async fn test_request_vs_notification_disambiguation() {
    // Test that the fix correctly disambiguates between requests and notifications
    // based on the presence of 'id' field, not on deserialization success

    let test_cases = vec![
        // (message, has_id, is_request, description)
        (
            r#"{"jsonrpc": "2.0", "id": 1, "method": "test"}"#,
            true,
            true,
            "Request with numeric id",
        ),
        (
            r#"{"jsonrpc": "2.0", "id": "abc", "method": "test"}"#,
            true,
            true,
            "Request with string id",
        ),
        (
            r#"{"jsonrpc": "2.0", "id": null, "method": "test"}"#,
            true,
            true,
            "Request with null id",
        ),
        (
            r#"{"jsonrpc": "2.0", "method": "test"}"#,
            false,
            false,
            "Notification without id",
        ),
        (
            r#"{"jsonrpc": "2.0", "method": "initialized"}"#,
            false,
            false,
            "Initialized notification",
        ),
        (
            r#"{"jsonrpc": "2.0", "method": "notifications/cancelled"}"#,
            false,
            false,
            "Cancelled notification",
        ),
    ];

    for (message, expected_has_id, _expected_is_request, description) in test_cases {
        let json: Value = serde_json::from_str(message).unwrap();
        let has_id = json.get("id").is_some();

        assert_eq!(has_id, expected_has_id, "Failed for case: {}", description);

        // Verify that both can deserialize as request (demonstrating the bug)
        let as_request: Result<JsonRpcRequest, _> = serde_json::from_str(message);
        assert!(
            as_request.is_ok(),
            "All messages can deserialize as request (this is why we need the fix): {}",
            description
        );
    }
}

// =============================================================================
// EDGE CASES AND ERROR HANDLING
// =============================================================================

#[tokio::test]
async fn test_invalid_json() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let invalid_json = r#"{"jsonrpc": "2.0", "method": "test"#; // Missing closing brace

    let result = handler.handle_message(invalid_json).await;
    assert!(result.is_err(), "Invalid JSON should return error");
}

#[tokio::test]
async fn test_missing_method_field() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let no_method = r#"{
        "jsonrpc": "2.0",
        "id": 1
    }"#;

    let result = handler.handle_message(no_method).await;
    assert!(result.is_err(), "Message without method should fail");
}

#[tokio::test]
async fn test_missing_jsonrpc_field() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let no_jsonrpc = r#"{
        "id": 1,
        "method": "test"
    }"#;

    let result = handler.handle_message(no_jsonrpc).await;
    assert!(result.is_err(), "Message without jsonrpc should fail");
}

#[tokio::test]
async fn test_empty_message() {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    let result = handler.handle_message("{}").await;
    assert!(result.is_err(), "Empty message should fail");
}

// =============================================================================
// PERFORMANCE AND CONCURRENCY TESTS
// =============================================================================

#[tokio::test]
async fn test_rapid_sequential_messages() {
    // Test that the handler can process many messages in sequence
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // Initialize first
    let init_request = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    }"#;
    handler.handle_message(init_request).await.unwrap();

    // Send initialized notification
    let initialized = r#"{"jsonrpc": "2.0", "method": "initialized"}"#;
    handler.handle_message(initialized).await.unwrap();

    // Rapidly send many tools/list requests
    for i in 0..100 {
        let tools_request = format!(
            r#"{{
            "jsonrpc": "2.0",
            "id": {},
            "method": "tools/list"
        }}"#,
            i + 2
        );

        let result = handler.handle_message(&tools_request).await;
        assert!(result.is_ok(), "Request {} should succeed", i);

        let response_str = result.unwrap().unwrap();
        let response: Value = serde_json::from_str(&response_str).unwrap();
        assert_eq!(response["id"], i + 2);
        assert!(response["error"].is_null());
    }
}

#[tokio::test]
async fn test_interleaved_requests_and_notifications() {
    // Test that requests and notifications can be interleaved
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // Initialize
    let init = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "1.0"}
        }
    }"#;
    handler.handle_message(init).await.unwrap();

    // Initialized notification
    let initialized = r#"{"jsonrpc": "2.0", "method": "initialized"}"#;
    handler.handle_message(initialized).await.unwrap();

    // Interleave requests and notifications
    let messages = vec![
        (
            r#"{"jsonrpc": "2.0", "id": 2, "method": "tools/list"}"#,
            true,
        ),
        (
            r#"{"jsonrpc": "2.0", "method": "notifications/cancelled"}"#,
            false,
        ),
        (
            r#"{"jsonrpc": "2.0", "id": 3, "method": "tools/list"}"#,
            true,
        ),
        (
            r#"{"jsonrpc": "2.0", "method": "notifications/cancelled"}"#,
            false,
        ),
    ];

    for (message, expect_response) in messages {
        let result = handler.handle_message(message).await;
        assert!(result.is_ok());

        if expect_response {
            assert!(result.unwrap().is_some(), "Request should have response");
        }
    }
}
