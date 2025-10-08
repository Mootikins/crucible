// crates/crucible-mcp/tests/test_helpers.rs
//
// Helper functions and utilities for MCP protocol testing

use crucible_mcp::protocol::McpProtocolHandler;
use serde_json::{json, Value};

/// Create a standard initialized handler ready for tool calls
pub async fn create_initialized_handler() -> McpProtocolHandler {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // Send initialize request
    let init_request = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocol_version": "2024-11-05",
            "capabilities": {},
            "client_info": {"name": "test", "version": "1.0"}
        }
    }"#;

    handler
        .handle_message(init_request)
        .await
        .expect("Initialize request should succeed");

    // Send initialized notification
    let initialized = r#"{"jsonrpc": "2.0", "method": "initialized"}"#;
    handler
        .handle_message(initialized)
        .await
        .expect("Initialized notification should succeed");

    handler
}

/// Create a JSON-RPC request message
pub fn create_request(id: i64, method: &str, params: Option<Value>) -> String {
    let mut request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method
    });

    if let Some(p) = params {
        request["params"] = p;
    }

    serde_json::to_string(&request).unwrap()
}

/// Create a JSON-RPC notification message
pub fn create_notification(method: &str, params: Option<Value>) -> String {
    let mut notification = json!({
        "jsonrpc": "2.0",
        "method": method
    });

    if let Some(p) = params {
        notification["params"] = p;
    }

    serde_json::to_string(&notification).unwrap()
}

/// Parse a response string as JSON and verify it's a successful response
pub fn parse_success_response(response_str: &str) -> Value {
    let response: Value =
        serde_json::from_str(response_str).expect("Response should be valid JSON");

    assert_eq!(
        response["jsonrpc"], "2.0",
        "Response should have jsonrpc 2.0"
    );
    assert!(
        response["error"].is_null(),
        "Response should not have error"
    );
    assert!(
        response["result"].is_object() || response["result"].is_array(),
        "Response should have result"
    );

    response
}

/// Parse a response string as JSON and verify it's an error response
pub fn parse_error_response(response_str: &str) -> Value {
    let response: Value =
        serde_json::from_str(response_str).expect("Response should be valid JSON");

    assert_eq!(
        response["jsonrpc"], "2.0",
        "Response should have jsonrpc 2.0"
    );
    assert!(
        response["error"].is_object(),
        "Response should have error object"
    );

    response
}

/// Verify a notification has the expected format
pub fn verify_notification(notification_str: &str, expected_method: &str) {
    let notification: Value =
        serde_json::from_str(notification_str).expect("Notification should be valid JSON");

    assert_eq!(notification["jsonrpc"], "2.0");
    assert_eq!(notification["method"], expected_method);
    assert!(
        notification.get("id").is_none(),
        "Notification should not have id field"
    );
}

/// Create an initialize request with custom parameters
pub fn create_initialize_request(id: i64, client_name: &str, protocol_version: &str) -> String {
    create_request(
        id,
        "initialize",
        Some(json!({
            "protocol_version": protocol_version,
            "capabilities": {},
            "client_info": {
                "name": client_name,
                "version": "1.0.0"
            }
        })),
    )
}

/// Test scenario builder for initialization handshake
pub struct InitializationScenario {
    handler: McpProtocolHandler,
    request_id_counter: i64,
}

impl InitializationScenario {
    pub fn new(server_name: &str, server_version: &str) -> Self {
        Self {
            handler: McpProtocolHandler::new(server_name.into(), server_version.into()),
            request_id_counter: 1,
        }
    }

    /// Execute the initialize request step
    pub async fn initialize(&mut self) -> Result<Value, String> {
        let request =
            create_initialize_request(self.request_id_counter, "test-client", "2024-11-05");
        self.request_id_counter += 1;

        let response_str = self
            .handler
            .handle_message(&request)
            .await
            .map_err(|e| format!("Initialize failed: {}", e))?
            .ok_or("Initialize should return response")?;

        let response = parse_success_response(&response_str);
        Ok(response)
    }

    /// Execute the initialized notification step
    pub async fn send_initialized(&mut self) -> Result<Value, String> {
        let notification = create_notification("initialized", None);

        let response_str = self
            .handler
            .handle_message(&notification)
            .await
            .map_err(|e| format!("Initialized notification failed: {}", e))?
            .ok_or("Initialized should return ready notification")?;

        let notification: Value = serde_json::from_str(&response_str)
            .map_err(|e| format!("Failed to parse ready notification: {}", e))?;

        Ok(notification)
    }

    /// Send a tools/list request
    pub async fn list_tools(&mut self) -> Result<Value, String> {
        let request = create_request(self.request_id_counter, "tools/list", None);
        self.request_id_counter += 1;

        let response_str = self
            .handler
            .handle_message(&request)
            .await
            .map_err(|e| format!("Tools/list failed: {}", e))?
            .ok_or("Tools/list should return response")?;

        let response = parse_success_response(&response_str);
        Ok(response)
    }

    /// Execute the complete initialization sequence
    pub async fn complete_initialization(&mut self) -> Result<(), String> {
        // Step 1: Initialize
        let init_response = self.initialize().await?;
        assert!(init_response["result"]["protocol_version"].is_string());
        assert!(init_response["result"]["server_info"].is_object());

        // Step 2: Send initialized notification and get ready
        let ready_notification = self.send_initialized().await?;
        assert_eq!(ready_notification["method"], "notifications/ready");

        Ok(())
    }

    /// Get a mutable reference to the handler for custom operations
    pub fn handler_mut(&mut self) -> &mut McpProtocolHandler {
        &mut self.handler
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_initialized_handler() {
        let mut handler = create_initialized_handler().await;

        // Verify it's initialized by calling tools/list
        let request = create_request(100, "tools/list", None);
        let response = handler.handle_message(&request).await.unwrap().unwrap();

        let response_json: Value = serde_json::from_str(&response).unwrap();
        assert!(response_json["error"].is_null());
    }

    #[tokio::test]
    async fn test_create_request() {
        let request = create_request(1, "test_method", None);
        let json: Value = serde_json::from_str(&request).unwrap();

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "test_method");
        assert!(json.get("params").is_none());
    }

    #[tokio::test]
    async fn test_create_request_with_params() {
        let params = json!({"key": "value"});
        let request = create_request(1, "test_method", Some(params.clone()));
        let json: Value = serde_json::from_str(&request).unwrap();

        assert_eq!(json["params"], params);
    }

    #[tokio::test]
    async fn test_create_notification() {
        let notification = create_notification("test_notification", None);
        let json: Value = serde_json::from_str(&notification).unwrap();

        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["method"], "test_notification");
        assert!(json.get("id").is_none());
    }

    #[tokio::test]
    async fn test_parse_success_response() {
        let response_str = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"data": "test"},
            "error": null
        }"#;

        let response = parse_success_response(response_str);
        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["data"], "test");
    }

    #[tokio::test]
    #[should_panic(expected = "Response should not have error")]
    async fn test_parse_success_response_with_error_panics() {
        let response_str = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": null,
            "error": {"code": -32600, "message": "Invalid request"}
        }"#;

        parse_success_response(response_str);
    }

    #[tokio::test]
    async fn test_parse_error_response() {
        let response_str = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": null,
            "error": {"code": -32600, "message": "Invalid request"}
        }"#;

        let response = parse_error_response(response_str);
        assert_eq!(response["id"], 1);
        assert_eq!(response["error"]["code"], -32600);
    }

    #[tokio::test]
    async fn test_verify_notification() {
        let notification_str = r#"{
            "jsonrpc": "2.0",
            "method": "notifications/ready"
        }"#;

        verify_notification(notification_str, "notifications/ready");
    }

    #[tokio::test]
    async fn test_initialization_scenario() {
        let mut scenario = InitializationScenario::new("test-server", "1.0.0");

        // Execute complete initialization
        scenario
            .complete_initialization()
            .await
            .expect("Initialization should succeed");

        // Verify we can list tools
        let tools_response = scenario
            .list_tools()
            .await
            .expect("Should be able to list tools after initialization");

        assert!(tools_response["result"]["tools"].is_array());
    }

    #[tokio::test]
    async fn test_initialization_scenario_step_by_step() {
        let mut scenario = InitializationScenario::new("test-server", "1.0.0");

        // Step 1: Initialize
        let init_response = scenario.initialize().await.unwrap();
        assert_eq!(init_response["result"]["protocol_version"], "2024-11-05");
        assert_eq!(
            init_response["result"]["server_info"]["name"],
            "test-server"
        );

        // Step 2: Send initialized and get ready
        let ready = scenario.send_initialized().await.unwrap();
        assert_eq!(ready["method"], "notifications/ready");

        // Step 3: List tools
        let tools = scenario.list_tools().await.unwrap();
        let tools_array = tools["result"]["tools"].as_array().unwrap();
        assert_eq!(tools_array.len(), 13);
    }
}
