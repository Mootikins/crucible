// tests/test_protocol_format.rs
//
// Comprehensive tests to validate JSON-RPC 2.0 protocol format compliance
// These tests ensure responses match the spec that Claude Desktop's Zod schema expects
//
// Critical requirement from JSON-RPC 2.0 spec:
// - Success responses MUST have "result" and MUST NOT have "error"
// - Error responses MUST have "error" and MUST NOT have "result"
// - Responses cannot have both "result" and "error"
// - All responses MUST have "jsonrpc": "2.0" and "id"

mod test_helpers;

use crucible_mcp::protocol::McpProtocolHandler;
use serde_json::{json, Value};
use test_helpers::{create_initialized_handler, create_request};

/// Validation errors for JSON-RPC responses
#[derive(Debug)]
struct ValidationError {
    field: String,
    issue: String,
}

/// Validate a JSON-RPC 2.0 response against the specification
///
/// JSON-RPC 2.0 Response spec:
/// - MUST have "jsonrpc": "2.0"
/// - MUST have "id" (matching the request, or null for errors)
/// - MUST have EITHER "result" OR "error" (not both, not neither)
/// - MUST NOT have request-only fields like "method" or "params"
fn validate_jsonrpc_response(json_str: &str) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    let json: Value = serde_json::from_str(json_str)
        .map_err(|e| vec![ValidationError {
            field: "parse".to_string(),
            issue: format!("Invalid JSON: {}", e),
        }])?;

    // Check jsonrpc version
    match json.get("jsonrpc") {
        Some(Value::String(v)) if v == "2.0" => {},
        Some(v) => errors.push(ValidationError {
            field: "jsonrpc".to_string(),
            issue: format!("Expected \"2.0\", got: {:?}", v),
        }),
        None => errors.push(ValidationError {
            field: "jsonrpc".to_string(),
            issue: "Missing required field".to_string(),
        }),
    }

    // Check id field exists (can be string, number, or null)
    if !json.get("id").is_some() {
        errors.push(ValidationError {
            field: "id".to_string(),
            issue: "Missing required field".to_string(),
        });
    }

    // Check for EITHER result OR error (not both, not neither)
    let has_result = json.get("result").is_some();
    let has_error = json.get("error").is_some();

    match (has_result, has_error) {
        (false, false) => errors.push(ValidationError {
            field: "result/error".to_string(),
            issue: "Response must have either 'result' or 'error'".to_string(),
        }),
        (true, true) => errors.push(ValidationError {
            field: "result/error".to_string(),
            issue: "Response must have either 'result' or 'error', not both".to_string(),
        }),
        _ => {}, // Valid: has exactly one
    }

    // Check for forbidden request-only fields
    if json.get("method").is_some() {
        errors.push(ValidationError {
            field: "method".to_string(),
            issue: "Response must NOT have 'method' field (that's for requests/notifications)".to_string(),
        });
    }

    if json.get("params").is_some() {
        errors.push(ValidationError {
            field: "params".to_string(),
            issue: "Response must NOT have 'params' field (that's for requests/notifications)".to_string(),
        });
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validate that a success response has result and NO error field
fn validate_success_response(json_str: &str) -> Result<Value, String> {
    let json: Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    // Must have result
    if !json.get("result").is_some() {
        return Err(format!("Success response missing 'result' field. Response: {}", json_str));
    }

    // Must NOT have error (even if null)
    if json.get("error").is_some() {
        return Err(format!(
            "Success response MUST NOT have 'error' field (even null). Response: {}",
            json_str
        ));
    }

    // Must have id
    if !json.get("id").is_some() {
        return Err(format!("Response missing 'id' field. Response: {}", json_str));
    }

    // Must have jsonrpc: "2.0"
    if json.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        return Err(format!("Response must have 'jsonrpc': '2.0'. Response: {}", json_str));
    }

    Ok(json)
}

/// Validate that an error response has error and NO result field
fn validate_error_response(json_str: &str) -> Result<Value, String> {
    let json: Value = serde_json::from_str(json_str)
        .map_err(|e| format!("Invalid JSON: {}", e))?;

    // Must have error
    if !json.get("error").is_some() || !json["error"].is_object() {
        return Err(format!("Error response missing 'error' object. Response: {}", json_str));
    }

    // Must NOT have result (even if null)
    if json.get("result").is_some() {
        return Err(format!(
            "Error response MUST NOT have 'result' field (even null). Response: {}",
            json_str
        ));
    }

    // Must have id
    if !json.get("id").is_some() {
        return Err(format!("Response missing 'id' field. Response: {}", json_str));
    }

    // Must have jsonrpc: "2.0"
    if json.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        return Err(format!("Response must have 'jsonrpc': '2.0'. Response: {}", json_str));
    }

    // Validate error object structure
    let error = &json["error"];
    if !error.get("code").is_some() || !error["code"].is_i64() {
        return Err(format!("Error object missing 'code' (integer). Response: {}", json_str));
    }

    if !error.get("message").is_some() || !error["message"].is_string() {
        return Err(format!("Error object missing 'message' (string). Response: {}", json_str));
    }

    Ok(json)
}

// ============================================================================
// SUCCESS RESPONSE TESTS
// ============================================================================

#[tokio::test]
async fn test_success_response_initialize() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let request = create_request(1, "initialize", Some(json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "test", "version": "1.0"}
    })));

    let response = handler.handle_message(&request).await
        .expect("Initialize should succeed")
        .expect("Initialize should return a response");

    // Validate the response format
    let json = validate_success_response(&response)
        .expect("Initialize response should be valid success response");

    // Additional checks specific to initialize
    assert_eq!(json["id"], 1, "Response id should match request id");
    assert!(json["result"].is_object(), "Initialize should have result object");
    assert!(json["result"]["protocolVersion"].is_string(), "Should have protocolVersion");
    assert!(json["result"]["serverInfo"].is_object(), "Should have serverInfo");

    // Verify NO error field exists
    assert!(json.get("error").is_none(), "Success response must not have error field");
}

#[tokio::test]
async fn test_success_response_tools_list() {
    let mut handler = create_initialized_handler().await;

    let request = create_request(2, "tools/list", None);

    let response = handler.handle_message(&request).await
        .expect("tools/list should succeed")
        .expect("tools/list should return a response");

    // Validate the response format
    let json = validate_success_response(&response)
        .expect("tools/list response should be valid success response");

    // Additional checks specific to tools/list
    assert_eq!(json["id"], 2, "Response id should match request id");
    assert!(json["result"].is_object(), "tools/list should have result object");
    assert!(json["result"]["tools"].is_array(), "Should have tools array");

    // Verify NO error field exists
    assert!(json.get("error").is_none(), "Success response must not have error field");
}

#[tokio::test]
async fn test_success_response_prompts_list() {
    let mut handler = create_initialized_handler().await;

    let request = create_request(3, "prompts/list", None);

    let response = handler.handle_message(&request).await
        .expect("prompts/list should succeed")
        .expect("Should return response");

    let json = validate_success_response(&response)
        .expect("prompts/list response should be valid success response");

    assert_eq!(json["id"], 3);
    assert!(json.get("error").is_none(), "Success response must not have error field");
}

#[tokio::test]
async fn test_success_response_resources_list() {
    let mut handler = create_initialized_handler().await;

    let request = create_request(4, "resources/list", None);

    let response = handler.handle_message(&request).await
        .expect("resources/list should succeed")
        .expect("Should return response");

    let json = validate_success_response(&response)
        .expect("resources/list response should be valid success response");

    assert_eq!(json["id"], 4);
    assert!(json.get("error").is_none(), "Success response must not have error field");
}

// ============================================================================
// ERROR RESPONSE TESTS
// ============================================================================

#[tokio::test]
async fn test_error_response_unknown_method() {
    let mut handler = create_initialized_handler().await;

    let request = create_request(10, "unknown/method", None);

    let response = handler.handle_message(&request).await
        .expect("Should handle unknown method")
        .expect("Should return error response");

    // Validate the error response format
    let json = validate_error_response(&response)
        .expect("Unknown method response should be valid error response");

    // Verify error structure
    assert_eq!(json["id"], 10, "Response id should match request id");
    assert_eq!(json["error"]["code"], -32601, "Should be method not found error");
    assert!(json["error"]["message"].as_str().unwrap().contains("not found"));

    // Verify NO result field exists
    assert!(json.get("result").is_none(), "Error response must not have result field");
}

#[tokio::test]
async fn test_error_response_uninitialized_server() {
    // Create handler WITHOUT initialization
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    let request = create_request(11, "tools/list", None);

    let response = handler.handle_message(&request).await
        .expect("Should handle request")
        .expect("Should return error response");

    // Validate the error response format
    let json = validate_error_response(&response)
        .expect("Uninitialized error should be valid error response");

    // Verify error structure
    assert_eq!(json["id"], 11);
    assert_eq!(json["error"]["code"], -32002, "Should be server not initialized error");
    assert!(json["error"]["message"].as_str().unwrap().contains("not initialized"));

    // Verify NO result field exists
    assert!(json.get("result").is_none(), "Error response must not have result field");
}

#[tokio::test]
async fn test_error_response_invalid_params() {
    let mut handler = create_initialized_handler().await;

    // Call tools/call without required params
    let request = create_request(12, "tools/call", None);

    let response = handler.handle_message(&request).await
        .expect("Should handle request")
        .expect("Should return error response");

    let json = validate_error_response(&response)
        .expect("Invalid params error should be valid error response");

    assert_eq!(json["id"], 12);
    assert_eq!(json["error"]["code"], -32602, "Should be invalid params error");
    assert!(json.get("result").is_none(), "Error response must not have result field");
}

#[tokio::test]
async fn test_error_response_initialize_without_params() {
    let mut handler = McpProtocolHandler::new("test-server".to_string(), "1.0.0".to_string());

    // Initialize without params
    let request = create_request(13, "initialize", None);

    let response = handler.handle_message(&request).await
        .expect("Should handle request")
        .expect("Should return error response");

    let json = validate_error_response(&response)
        .expect("Invalid params error should be valid error response");

    assert_eq!(json["id"], 13);
    assert_eq!(json["error"]["code"], -32602);
    assert!(json.get("result").is_none(), "Error response must not have result field");
}

// ============================================================================
// SEMANTIC SEARCH ERROR SCENARIO (Critical for Claude Desktop)
// ============================================================================

#[tokio::test]
async fn test_semantic_search_uninitialized_error() {
    use crucible_mcp::StdioMcpServer;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let _db_path = temp_dir.path().join("test.db");

    // Create server WITHOUT initializing embedding provider
    let mut server = StdioMcpServer::new("test-server".to_string(), "1.0.0".to_string());

    // Initialize the protocol layer but NOT the MCP server instance
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
    server.handle_line(init_request).await.unwrap();

    // Send initialized notification
    let initialized = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
    server.handle_line(initialized).await.unwrap();

    // Try to call semantic_search tool without MCP server instance initialized
    // This simulates the error scenario where server.initialize() wasn't called
    let search_request = r#"{"jsonrpc":"2.0","id":100,"method":"tools/call","params":{"name":"semantic_search","arguments":{"query":"test query","top_k":5}}}"#;

    let response = server.handle_line(search_request).await
        .expect("Should handle request")
        .expect("Should return response");

    println!("Semantic search uninitialized response: {}", response);

    // NOTE: Current behavior is that when StdioMcpServer.mcp_server is None,
    // the protocol handler returns a SUCCESS response with a tool result containing
    // is_error: true, rather than a JSON-RPC error response.
    //
    // This is actually acceptable per MCP spec - tool execution errors are wrapped
    // in success responses. A JSON-RPC error would indicate a protocol-level error.
    //
    // However, this may be the source of the Claude Desktop issue if the validator
    // is strict about what it expects.

    validate_jsonrpc_response(&response)
        .expect("Response should be valid JSON-RPC");

    let json: Value = serde_json::from_str(&response).unwrap();

    // The current implementation returns a success response with error content
    if json.get("result").is_some() {
        // This is the current behavior: success response with tool error
        validate_success_response(&response)
            .expect("Should be valid success response");

        assert_eq!(json["id"], 100);
        assert!(json["result"]["isError"].as_bool().unwrap_or(false),
            "Tool result should indicate error");
        assert!(json["result"]["content"].is_array(),
            "Tool result should have content array");

        // Verify NO error field in JSON-RPC response
        assert!(json.get("error").is_none(),
            "Success response must not have error field");
    } else {
        // If this changes to return a JSON-RPC error (which might be better),
        // validate that instead
        validate_error_response(&response)
            .expect("Should be valid error response");

        assert_eq!(json["id"], 100);
        assert!(json.get("result").is_none(),
            "Error response must not have result field");
    }
}

#[tokio::test]
async fn test_semantic_search_invalid_query_error() {
    use crucible_mcp::StdioMcpServer;
    use test_helpers::create_test_provider;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut server = StdioMcpServer::new("test-server".to_string(), "1.0.0".to_string());
    server.initialize(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Complete initialization handshake
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
    server.handle_line(init_request).await.unwrap();

    let initialized = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
    server.handle_line(initialized).await.unwrap();

    // Call semantic_search without required query parameter
    let search_request = r#"{"jsonrpc":"2.0","id":101,"method":"tools/call","params":{"name":"semantic_search","arguments":{}}}"#;

    let response = server.handle_line(search_request).await
        .expect("Should handle request")
        .expect("Should return response");

    println!("Semantic search invalid query response: {}", response);

    // Verify this is a properly formatted error OR success response with is_error flag
    validate_jsonrpc_response(&response)
        .expect("Response should be valid JSON-RPC");

    let json: Value = serde_json::from_str(&response).unwrap();

    // Either it's an error response, or it's a success response with tool result
    if json.get("error").is_some() {
        validate_error_response(&response)
            .expect("Should be valid error response");
    } else if json.get("result").is_some() {
        validate_success_response(&response)
            .expect("Should be valid success response");

        // Tool call responses wrap errors in the result
        assert!(json["result"]["content"].is_array() || json["result"]["is_error"].as_bool() == Some(true));
    } else {
        panic!("Response must have either error or result");
    }
}

// ============================================================================
// EXACT JSON STRUCTURE VALIDATION
// ============================================================================

#[tokio::test]
async fn test_exact_json_structure_success() {
    let mut handler = create_initialized_handler().await;

    let request = create_request(50, "tools/list", None);
    let response = handler.handle_message(&request).await
        .expect("Should succeed")
        .expect("Should return response");

    let json: Value = serde_json::from_str(&response).unwrap();
    let obj = json.as_object().expect("Response should be an object");

    // Verify EXACT structure
    assert!(obj.contains_key("jsonrpc"), "Must have jsonrpc");
    assert!(obj.contains_key("id"), "Must have id");
    assert!(obj.contains_key("result"), "Must have result");
    assert!(!obj.contains_key("error"), "Must NOT have error key (not even null)");
    assert!(!obj.contains_key("method"), "Must NOT have method");
    assert!(!obj.contains_key("params"), "Must NOT have params");

    // Should only have these 3 keys
    assert_eq!(obj.len(), 3, "Success response should have exactly 3 keys: jsonrpc, id, result");
}

#[tokio::test]
async fn test_exact_json_structure_error() {
    let mut handler = create_initialized_handler().await;

    let request = create_request(51, "unknown/method", None);
    let response = handler.handle_message(&request).await
        .expect("Should handle")
        .expect("Should return response");

    let json: Value = serde_json::from_str(&response).unwrap();
    let obj = json.as_object().expect("Response should be an object");

    // Verify EXACT structure
    assert!(obj.contains_key("jsonrpc"), "Must have jsonrpc");
    assert!(obj.contains_key("id"), "Must have id");
    assert!(obj.contains_key("error"), "Must have error");
    assert!(!obj.contains_key("result"), "Must NOT have result key (not even null)");
    assert!(!obj.contains_key("method"), "Must NOT have method");
    assert!(!obj.contains_key("params"), "Must NOT have params");

    // Should only have these 3 keys
    assert_eq!(obj.len(), 3, "Error response should have exactly 3 keys: jsonrpc, id, error");
}

// ============================================================================
// NOTIFICATION TESTS (No response)
// ============================================================================

#[tokio::test]
async fn test_notification_no_response() {
    let mut handler = create_initialized_handler().await;

    // Send notification (has no id field)
    let notification = r#"{"jsonrpc": "2.0", "method": "notifications/cancelled"}"#;

    let response = handler.handle_message(notification).await
        .expect("Should handle notification");

    // Notifications should return None (no response)
    assert!(response.is_none(), "Notifications should not return a response");
}

// ============================================================================
// MULTIPLE REQUESTS ID CORRESPONDENCE
// ============================================================================

#[tokio::test]
async fn test_multiple_requests_maintain_id_correspondence() {
    let mut handler = create_initialized_handler().await;

    // Send multiple requests with different IDs
    let requests = vec![
        (100, "tools/list"),
        (200, "prompts/list"),
        (300, "resources/list"),
    ];

    for (id, method) in requests {
        let request = create_request(id, method, None);
        let response = handler.handle_message(&request).await
            .expect("Request should succeed")
            .expect("Should return response");

        // Validate format
        validate_jsonrpc_response(&response)
            .expect(&format!("Response for {} should be valid", method));

        // Verify ID matches
        let json: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(json["id"], id, "Response id must match request id for {}", method);
    }
}

// ============================================================================
// COMPREHENSIVE STDIO SERVER TEST
// ============================================================================

#[tokio::test]
async fn test_stdio_server_response_format() {
    use crucible_mcp::StdioMcpServer;
    use test_helpers::create_test_provider;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut server = StdioMcpServer::new("test-server".to_string(), "1.0.0".to_string());
    server.initialize(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Test initialize
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;

    let response = server.handle_line(init_request).await
        .expect("Initialize should succeed")
        .expect("Should return response");

    println!("Initialize response: {}", response);

    // Validate response
    validate_success_response(&response)
        .expect("Initialize response should be valid success response");

    // Send initialized notification
    let initialized = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
    let response_after_init = server.handle_line(initialized).await
        .expect("Should handle initialized");

    // Per MCP spec, notifications/initialized should NOT receive a response
    assert!(response_after_init.is_none(),
        "Server should not send response to notifications/initialized per MCP spec");

    // Test tools/list
    let tools_request = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let tools_response = server.handle_line(tools_request).await
        .expect("tools/list should succeed")
        .expect("Should return response");

    println!("Tools response: {}", tools_response);

    validate_success_response(&tools_response)
        .expect("Tools response should be valid success response");
}

// ============================================================================
// SEMANTIC SEARCH EMBEDDING PROVIDER FAILURE TEST
// ============================================================================

/// Test that semantic_search tool calls with failing embedding provider
/// return a proper JSON-RPC success response with tool error, not a protocol error.
///
/// This test verifies the critical fix for Claude Desktop compatibility:
/// - When embeddings fail (network, API key, rate limit, etc.)
/// - The MCP server returns a JSON-RPC success response (with "result", not "error")
/// - Inside the result, the tool's isError: true is set
/// - The error message is user-friendly and mentions embedding failure
/// - No Rust errors are propagated as JSON-RPC protocol errors
#[tokio::test]
async fn test_semantic_search_embedding_provider_failure() {
    use crucible_mcp::StdioMcpServer;
    use tempfile::tempdir;

    // Create a failing embedding provider
    let failing_provider = test_helpers::create_failing_provider("API rate limit exceeded");

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut server = StdioMcpServer::new("test-server".to_string(), "1.0.0".to_string());
    server.initialize(db_path.to_str().unwrap(), failing_provider).await.unwrap();

    // Complete initialization handshake
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
    server.handle_line(init_request).await.unwrap();

    let initialized = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
    server.handle_line(initialized).await.unwrap();

    // Call semantic_search tool - this should trigger the embedding provider failure
    let search_request = r#"{"jsonrpc":"2.0","id":100,"method":"tools/call","params":{"name":"semantic_search","arguments":{"query":"test query","top_k":5}}}"#;

    let response = server.handle_line(search_request).await
        .expect("Should handle request without protocol error")
        .expect("Should return response");

    println!("Semantic search embedding failure response: {}", response);

    // CRITICAL: This must be a JSON-RPC SUCCESS response, not an error response
    let json = validate_success_response(&response)
        .expect("Response should be a JSON-RPC success response (with 'result', not 'error')");

    // Verify response structure
    assert_eq!(json["id"], 100, "Response id should match request id");
    assert_eq!(json["jsonrpc"], "2.0", "Should have JSON-RPC 2.0");

    // Verify NO error field in JSON-RPC response
    assert!(json.get("error").is_none(),
        "JSON-RPC response must NOT have 'error' field - tool errors are wrapped in 'result'");

    // Verify result contains tool response with error indication
    assert!(json["result"].is_object(), "Result should be an object (CallToolResponse)");

    // Check isError flag
    assert_eq!(json["result"]["isError"], true,
        "Tool result should have isError: true when embedding fails");

    // Check content array exists and contains error message
    assert!(json["result"]["content"].is_array(),
        "Tool result should have content array");

    let content_array = json["result"]["content"].as_array().unwrap();
    assert!(!content_array.is_empty(), "Content array should not be empty");

    // Extract error message from first content item
    let error_message = content_array[0]["text"].as_str()
        .expect("Content should have text field with error message");

    // Verify error message is user-friendly and mentions embedding failure
    assert!(
        error_message.contains("embedding") || error_message.contains("Failed to generate"),
        "Error message should mention embedding failure. Got: {}",
        error_message
    );

    // The error message should contain the actual provider error
    assert!(
        error_message.contains("API rate limit exceeded") || error_message.contains("rate limit"),
        "Error message should contain the underlying provider error. Got: {}",
        error_message
    );

    println!("✓ Verified: Embedding provider failure returns JSON-RPC success with tool error");
    println!("✓ Verified: isError flag is set to true");
    println!("✓ Verified: Error message is user-friendly: {}", error_message);
}

/// Test semantic_search with network timeout error
#[tokio::test]
async fn test_semantic_search_embedding_network_timeout() {
    use crucible_mcp::StdioMcpServer;
    use tempfile::tempdir;

    let failing_provider = test_helpers::create_failing_provider("Connection timeout after 30s");

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut server = StdioMcpServer::new("test-server".to_string(), "1.0.0".to_string());
    server.initialize(db_path.to_str().unwrap(), failing_provider).await.unwrap();

    // Complete initialization
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
    server.handle_line(init_request).await.unwrap();
    let initialized = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
    server.handle_line(initialized).await.unwrap();

    let search_request = r#"{"jsonrpc":"2.0","id":101,"method":"tools/call","params":{"name":"semantic_search","arguments":{"query":"timeout test","top_k":3}}}"#;

    let response = server.handle_line(search_request).await
        .expect("Should handle request")
        .expect("Should return response");

    let json = validate_success_response(&response)
        .expect("Should be JSON-RPC success response");

    assert_eq!(json["result"]["isError"], true);

    let content = json["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content.contains("timeout") || content.contains("Failed to generate"));
}

/// Test semantic_search with authentication error
#[tokio::test]
async fn test_semantic_search_embedding_auth_error() {
    use crucible_mcp::StdioMcpServer;
    use tempfile::tempdir;

    let failing_provider = test_helpers::create_failing_provider("Invalid API key");

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut server = StdioMcpServer::new("test-server".to_string(), "1.0.0".to_string());
    server.initialize(db_path.to_str().unwrap(), failing_provider).await.unwrap();

    // Complete initialization
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
    server.handle_line(init_request).await.unwrap();
    let initialized = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
    server.handle_line(initialized).await.unwrap();

    let search_request = r#"{"jsonrpc":"2.0","id":102,"method":"tools/call","params":{"name":"semantic_search","arguments":{"query":"auth test","top_k":5}}}"#;

    let response = server.handle_line(search_request).await
        .expect("Should handle request")
        .expect("Should return response");

    let json = validate_success_response(&response)
        .expect("Should be JSON-RPC success response");

    assert_eq!(json["result"]["isError"], true);

    let content = json["result"]["content"][0]["text"].as_str().unwrap();
    assert!(content.contains("API key") || content.contains("Failed to generate"));
}

/// Test that successful semantic_search still works with working provider
#[tokio::test]
async fn test_semantic_search_success_with_working_provider() {
    use crucible_mcp::StdioMcpServer;
    use tempfile::tempdir;
    use test_helpers::create_test_provider;

    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut server = StdioMcpServer::new("test-server".to_string(), "1.0.0".to_string());
    server.initialize(db_path.to_str().unwrap(), create_test_provider()).await.unwrap();

    // Complete initialization
    let init_request = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
    server.handle_line(init_request).await.unwrap();
    let initialized = r#"{"jsonrpc":"2.0","method":"initialized"}"#;
    server.handle_line(initialized).await.unwrap();

    // This should succeed with a working provider
    let search_request = r#"{"jsonrpc":"2.0","id":103,"method":"tools/call","params":{"name":"semantic_search","arguments":{"query":"success test","top_k":5}}}"#;

    let response = server.handle_line(search_request).await
        .expect("Should handle request")
        .expect("Should return response");

    let json = validate_success_response(&response)
        .expect("Should be JSON-RPC success response");

    // With working provider, isError should be false or absent
    let is_error = json["result"]["isError"].as_bool().unwrap_or(false);
    assert!(!is_error, "Tool result should NOT have isError: true with working provider");

    // Content should contain the search results
    assert!(json["result"]["content"].is_array());
}
