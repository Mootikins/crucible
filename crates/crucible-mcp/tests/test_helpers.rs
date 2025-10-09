// crates/crucible-mcp/tests/test_helpers.rs
//
// Helper functions and utilities for MCP protocol testing

use async_trait::async_trait;
use crucible_mcp::embeddings::{EmbeddingProvider, EmbeddingResponse, EmbeddingResult};
use crucible_mcp::protocol::McpProtocolHandler;
use serde_json::{json, Value};
use std::fs;
use std::sync::Arc;

/// Helper function to create test markdown files in a directory
pub fn create_test_vault(vault_path: &std::path::Path) {
    // Create some test markdown files
    let files = vec![
        ("file0.md", "Content for file: file0.md"),
        ("file1.md", "Content for file: file1.md"),
        ("file2.md", "Content for file: file2.md"),
        ("file3.md", "Content for file: file3.md"),
        ("file4.md", "Content for file: file4.md"),
    ];

    for (filename, content) in files {
        let file_path = vault_path.join(filename);
        fs::write(file_path, content).unwrap();
    }
}

/// Mock embedding provider for testing
///
/// Returns dummy embeddings with configurable dimensions.
/// This provider is used in tests where actual embedding generation is not needed.
pub struct MockEmbeddingProvider {
    dimensions: usize,
    model_name: String,
}

impl MockEmbeddingProvider {
    pub fn new() -> Self {
        Self {
            dimensions: 384, // Standard dimension for many embedding models
            model_name: "mock-test-model".to_string(),
        }
    }

    pub fn with_dimensions(dimensions: usize) -> Self {
        Self {
            dimensions,
            model_name: "mock-test-model".to_string(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> EmbeddingResult<EmbeddingResponse> {
        // Generate a simple deterministic embedding based on text length
        // This is not a real embedding, but suitable for testing
        let mut embedding = vec![0.1; self.dimensions];

        // Add some variation based on text content for determinism in tests
        let text_hash = text.len() as f32 / 100.0;
        for (i, val) in embedding.iter_mut().enumerate() {
            *val += (i as f32 * text_hash).sin() * 0.1;
        }

        Ok(EmbeddingResponse::new(embedding, self.model_name.clone()))
    }

    async fn embed_batch(&self, texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        let mut results = Vec::new();
        for text in texts {
            results.push(self.embed(&text).await?);
        }
        Ok(results)
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        "MockProvider"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        Ok(true)
    }
}

/// Create a mock embedding provider for testing
pub fn create_test_provider() -> Arc<dyn EmbeddingProvider> {
    Arc::new(MockEmbeddingProvider::new())
}

/// Create a mock embedding provider with custom dimensions
pub fn create_test_provider_with_dimensions(dimensions: usize) -> Arc<dyn EmbeddingProvider> {
    Arc::new(MockEmbeddingProvider::with_dimensions(dimensions))
}

/// Mock embedding provider that always fails with a specific error
///
/// This provider is used to test error handling when embedding generation fails
/// due to network errors, API failures, rate limits, etc.
pub struct FailingEmbeddingProvider {
    error_message: String,
}

impl FailingEmbeddingProvider {
    pub fn new(error_message: String) -> Self {
        Self { error_message }
    }
}

#[async_trait]
impl EmbeddingProvider for FailingEmbeddingProvider {
    async fn embed(&self, _text: &str) -> EmbeddingResult<EmbeddingResponse> {
        Err(crucible_mcp::embeddings::EmbeddingError::ProviderError {
            provider: "FailingProvider".to_string(),
            message: self.error_message.clone(),
        })
    }

    async fn embed_batch(&self, _texts: Vec<String>) -> EmbeddingResult<Vec<EmbeddingResponse>> {
        Err(crucible_mcp::embeddings::EmbeddingError::ProviderError {
            provider: "FailingProvider".to_string(),
            message: self.error_message.clone(),
        })
    }

    fn model_name(&self) -> &str {
        "failing-test-model"
    }

    fn dimensions(&self) -> usize {
        384 // Doesn't matter since it always fails
    }

    fn provider_name(&self) -> &str {
        "FailingProvider"
    }

    async fn health_check(&self) -> EmbeddingResult<bool> {
        Ok(false)
    }
}

/// Create a failing embedding provider for testing error scenarios
pub fn create_failing_provider(error_message: &str) -> Arc<dyn EmbeddingProvider> {
    Arc::new(FailingEmbeddingProvider::new(error_message.to_string()))
}

/// Create a standard initialized handler ready for tool calls
pub async fn create_initialized_handler() -> McpProtocolHandler {
    let mut handler = McpProtocolHandler::new("test".into(), "1.0".into());

    // Send initialize request
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
            "protocolVersion": protocol_version,
            "capabilities": {},
            "clientInfo": {
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
    pub async fn send_initialized(&mut self) -> Result<(), String> {
        let notification = create_notification("initialized", None);

        // Per MCP spec, server does NOT send a response to notifications/initialized
        let response = self
            .handler
            .handle_message(&notification)
            .await
            .map_err(|e| format!("Initialized notification failed: {}", e))?;

        // Should be None (no response)
        if response.is_some() {
            return Err("Server should not respond to notifications/initialized per MCP spec".to_string());
        }

        Ok(())
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
        assert!(init_response["result"]["protocolVersion"].is_string());
        assert!(init_response["result"]["serverInfo"].is_object());

        // Step 2: Send initialized notification
        self.send_initialized().await?;

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
        assert_eq!(init_response["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(
            init_response["result"]["serverInfo"]["name"],
            "test-server"
        );

        // Step 2: Send initialized notification
        scenario.send_initialized().await.unwrap();

        // Step 3: List tools
        let tools = scenario.list_tools().await.unwrap();
        let tools_array = tools["result"]["tools"].as_array().unwrap();
        assert_eq!(tools_array.len(), 13);
    }
}
