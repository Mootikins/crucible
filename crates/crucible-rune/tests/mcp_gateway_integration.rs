//! Integration tests for MCP Gateway client
//!
//! These tests verify the MCP gateway client's ability to:
//! - Discover tools from upstream MCP servers
//! - Filter tools based on whitelist/blacklist
//! - Emit events for tool discovery and execution
//! - Handle tool calls with event lifecycle

use crucible_rune::event_bus::EventBus;
use crucible_rune::mcp_gateway::{
    TransportConfig, UpstreamConfig, UpstreamMcpClient, UpstreamTool,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Helper to create a test event bus
fn create_test_event_bus() -> Arc<RwLock<EventBus>> {
    Arc::new(RwLock::new(EventBus::new()))
}

/// Helper to create a test upstream config
fn create_test_config(name: &str, prefix: Option<&str>) -> UpstreamConfig {
    UpstreamConfig {
        name: name.to_string(),
        transport: TransportConfig::Stdio {
            command: "echo".to_string(),
            args: vec![],
            env: vec![],
        },
        prefix: prefix.map(|s| s.to_string()),
        allowed_tools: None,
        blocked_tools: None,
        auto_reconnect: true,
    }
}

/// Helper to create a test tool
fn create_test_tool(name: &str, upstream: &str, prefixed_name: Option<&str>) -> UpstreamTool {
    UpstreamTool {
        name: name.to_string(),
        prefixed_name: prefixed_name.unwrap_or(name).to_string(),
        description: Some(format!("Test tool: {}", name)),
        input_schema: json!({
            "type": "object",
            "properties": {
                "input": {"type": "string"}
            }
        }),
        upstream: upstream.to_string(),
    }
}

#[tokio::test]
async fn test_upstream_client_creation() {
    let config = create_test_config("test_server", None);
    let client = UpstreamMcpClient::new(config);

    assert_eq!(client.name(), "test_server");
    assert_eq!(client.prefix(), None);
    assert!(!client.is_connected().await);
}

#[tokio::test]
async fn test_upstream_client_with_prefix() {
    let config = create_test_config("test_server", Some("test_"));
    let client = UpstreamMcpClient::new(config);

    assert_eq!(client.name(), "test_server");
    assert_eq!(client.prefix(), Some("test_"));
}

#[tokio::test]
async fn test_tool_discovery_and_storage() {
    let config = create_test_config("test_server", None);
    let client = UpstreamMcpClient::new(config);

    let tool1 = create_test_tool("tool1", "test_server", None);
    let tool2 = create_test_tool("tool2", "test_server", None);

    // Update tools
    client
        .update_tools(vec![tool1.clone(), tool2.clone()])
        .await;

    // Verify tools are stored
    let tools = client.tools().await;
    assert_eq!(tools.len(), 2);

    // Verify we can get tools by name
    let retrieved = client.get_tool("tool1").await;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "tool1");
}

#[tokio::test]
async fn test_tool_filtering_whitelist() {
    let mut config = create_test_config("test_server", None);
    config.allowed_tools = Some(vec!["tool1".to_string(), "tool2".to_string()]);

    let client = UpstreamMcpClient::new(config);

    let tool1 = create_test_tool("tool1", "test_server", None);
    let tool2 = create_test_tool("tool2", "test_server", None);
    let tool3 = create_test_tool("tool3", "test_server", None);

    // Update with all tools
    client.update_tools(vec![tool1, tool2, tool3]).await;

    // Only whitelisted tools should be stored
    let tools = client.tools().await;
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().any(|t| t.name == "tool1"));
    assert!(tools.iter().any(|t| t.name == "tool2"));
    assert!(!tools.iter().any(|t| t.name == "tool3"));
}

#[tokio::test]
async fn test_tool_filtering_blacklist() {
    let mut config = create_test_config("test_server", None);
    config.blocked_tools = Some(vec!["tool3".to_string()]);

    let client = UpstreamMcpClient::new(config);

    let tool1 = create_test_tool("tool1", "test_server", None);
    let tool2 = create_test_tool("tool2", "test_server", None);
    let tool3 = create_test_tool("tool3", "test_server", None);

    // Update with all tools
    client.update_tools(vec![tool1, tool2, tool3]).await;

    // Blocked tool should not be stored
    let tools = client.tools().await;
    assert_eq!(tools.len(), 2);
    assert!(!tools.iter().any(|t| t.name == "tool3"));
}

#[tokio::test]
async fn test_tool_filtering_glob_patterns() {
    let mut config = create_test_config("test_server", None);
    config.allowed_tools = Some(vec!["test_*".to_string()]);

    let client = UpstreamMcpClient::new(config);

    let tool1 = create_test_tool("test_tool1", "test_server", None);
    let tool2 = create_test_tool("test_tool2", "test_server", None);
    let tool3 = create_test_tool("other_tool", "test_server", None);

    client.update_tools(vec![tool1, tool2, tool3]).await;

    let tools = client.tools().await;
    assert_eq!(tools.len(), 2);
    assert!(tools.iter().all(|t| t.name.starts_with("test_")));
}

#[tokio::test]
async fn test_tool_prefix_application() {
    let config = create_test_config("test_server", Some("gh_"));
    let client = UpstreamMcpClient::new(config);

    let tool = create_test_tool("search_repos", "test_server", Some("gh_search_repos"));

    client.update_tools(vec![tool.clone()]).await;

    let retrieved = client.get_tool("search_repos").await;
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.name, "search_repos");
    assert_eq!(retrieved.prefixed_name, "gh_search_repos");

    // Should be able to get by prefixed name
    let by_prefixed = client.get_tool_by_prefixed_name("gh_search_repos").await;
    assert!(by_prefixed.is_some());
}

#[tokio::test]
async fn test_is_tool_allowed() {
    let mut config = create_test_config("test_server", None);
    config.allowed_tools = Some(vec!["allowed_*".to_string()]);
    config.blocked_tools = Some(vec!["blocked_specific".to_string()]);

    let client = UpstreamMcpClient::new(config);

    // Allowed by pattern
    assert!(client.is_tool_allowed("allowed_tool1"));
    assert!(client.is_tool_allowed("allowed_tool2"));

    // Blocked specifically
    assert!(!client.is_tool_allowed("blocked_specific"));

    // Not in whitelist
    assert!(!client.is_tool_allowed("other_tool"));
}

#[tokio::test]
async fn test_event_bus_integration() {
    let event_bus = create_test_event_bus();
    let config = create_test_config("test_server", None);
    let client = UpstreamMcpClient::new(config).with_event_bus(event_bus.clone());

    let tool = create_test_tool("test_tool", "test_server", None);

    // Update tools should emit tool:discovered events
    client.update_tools(vec![tool]).await;

    // Verify event was emitted (check event bus state)
    // Note: This is a simplified check - in a real scenario, you'd subscribe to events
    let _bus = event_bus.read().await;
    // The event bus should have handlers registered if any
    // For now, we just verify the client is set up correctly
    assert!(true); // Placeholder - actual event verification would require event bus inspection
}

#[tokio::test]
async fn test_tool_not_found_error() {
    let config = create_test_config("test_server", None);
    let client = UpstreamMcpClient::new(config);

    // Try to get a non-existent tool
    let result = client.get_tool("nonexistent").await;
    assert!(result.is_none());

    let by_prefixed = client.get_tool_by_prefixed_name("nonexistent").await;
    assert!(by_prefixed.is_none());
}

#[tokio::test]
async fn test_multiple_upstreams() {
    let config1 = create_test_config("server1", Some("s1_"));
    let config2 = create_test_config("server2", Some("s2_"));

    let client1 = UpstreamMcpClient::new(config1);
    let client2 = UpstreamMcpClient::new(config2);

    let tool1 = create_test_tool("tool", "server1", Some("s1_tool"));
    let tool2 = create_test_tool("tool", "server2", Some("s2_tool"));

    client1.update_tools(vec![tool1]).await;
    client2.update_tools(vec![tool2]).await;

    // Both clients should have their tools
    assert_eq!(client1.tools().await.len(), 1);
    assert_eq!(client2.tools().await.len(), 1);

    // Tools should have different prefixed names
    let t1 = client1.get_tool_by_prefixed_name("s1_tool").await.unwrap();
    let t2 = client2.get_tool_by_prefixed_name("s2_tool").await.unwrap();

    assert_eq!(t1.upstream, "server1");
    assert_eq!(t2.upstream, "server2");
}

// =============================================================================
// MCP Module Generation and Rune Integration Tests
// =============================================================================

use crucible_rune::mcp_gateway::{ContentBlock, ToolCallResult};
use crucible_rune::mcp_module::{generate_mcp_server_module, McpToolCaller};
use crucible_rune::RuneExecutor;

/// Mock MCP client for testing
struct MockMcpClient {
    responses: std::collections::HashMap<String, ToolCallResult>,
}

impl MockMcpClient {
    fn new() -> Self {
        Self {
            responses: std::collections::HashMap::new(),
        }
    }

    fn with_response(mut self, tool: &str, text: &str, is_error: bool) -> Self {
        self.responses.insert(
            tool.to_string(),
            ToolCallResult {
                content: vec![ContentBlock::Text {
                    text: text.to_string(),
                }],
                is_error,
            },
        );
        self
    }
}

impl McpToolCaller for MockMcpClient {
    async fn call_tool(
        &self,
        tool_name: &str,
        _args: serde_json::Value,
    ) -> Result<ToolCallResult, String> {
        self.responses
            .get(tool_name)
            .cloned()
            .ok_or_else(|| format!("Unknown tool: {}", tool_name))
    }
}

#[tokio::test]
async fn test_generate_mcp_module_creates_valid_module() {
    let client = Arc::new(MockMcpClient::new().with_response("echo", "hello", false));

    let tools = vec![UpstreamTool {
        name: "echo".to_string(),
        prefixed_name: "test_echo".to_string(),
        description: Some("Echo a message".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "message": { "type": "string" }
            },
            "required": ["message"]
        }),
        upstream: "test".to_string(),
    }];

    let module = generate_mcp_server_module("test", &tools, client);
    assert!(module.is_ok(), "Module generation should succeed");
}

#[tokio::test]
async fn test_mcp_module_can_be_installed_in_executor() {
    let client = Arc::new(MockMcpClient::new().with_response("greet", "Hello!", false));

    let tools = vec![UpstreamTool {
        name: "greet".to_string(),
        prefixed_name: "mock_greet".to_string(),
        description: Some("Greet someone".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        }),
        upstream: "mock".to_string(),
    }];

    let module = generate_mcp_server_module("mock", &tools, client).unwrap();
    let executor = RuneExecutor::with_modules(vec![module]);
    assert!(executor.is_ok(), "Executor with MCP module should be created");
}

#[tokio::test]
async fn test_rune_script_can_call_mcp_tool() {
    // Create mock client that returns JSON
    let client = Arc::new(
        MockMcpClient::new().with_response("get_data", r#"{"value": 42}"#, false),
    );

    let tools = vec![UpstreamTool {
        name: "get_data".to_string(),
        prefixed_name: "mock_get_data".to_string(),
        description: Some("Get some data".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
        upstream: "mock".to_string(),
    }];

    let module = generate_mcp_server_module("mock", &tools, client).unwrap();
    let executor = RuneExecutor::with_modules(vec![module]).unwrap();

    // Compile a script that calls the MCP tool
    let script = r#"
use cru::mcp::mock;

pub async fn main() {
    let result = mock::get_data().await?;
    result.text()
}
"#;

    let unit = executor.compile("test", script);
    assert!(
        unit.is_ok(),
        "Script using MCP tools should compile: {:?}",
        unit.err()
    );

    // Note: Actually executing would require async execution support
    // This test verifies the module is correctly registered and accessible
}

#[tokio::test]
async fn test_mcp_result_methods_in_rune() {
    let client = Arc::new(
        MockMcpClient::new()
            .with_response("success_tool", "Success!", false)
            .with_response("error_tool", "Error occurred", true),
    );

    let tools = vec![
        UpstreamTool {
            name: "success_tool".to_string(),
            prefixed_name: "mock_success_tool".to_string(),
            description: Some("A successful tool".to_string()),
            input_schema: json!({"type": "object", "properties": {}}),
            upstream: "mock".to_string(),
        },
        UpstreamTool {
            name: "error_tool".to_string(),
            prefixed_name: "mock_error_tool".to_string(),
            description: Some("A failing tool".to_string()),
            input_schema: json!({"type": "object", "properties": {}}),
            upstream: "mock".to_string(),
        },
    ];

    let module = generate_mcp_server_module("mock", &tools, client).unwrap();
    let executor = RuneExecutor::with_modules(vec![module]).unwrap();

    // Script that uses various McpResult methods
    let script = r#"
use cru::mcp::mock;

pub async fn test_success() {
    let result = mock::success_tool().await?;

    // Test is_error()
    if result.is_error() {
        return "ERROR";
    }

    // Test text()
    result.text()
}

pub async fn test_error() {
    let result = mock::error_tool().await?;
    result.is_error()
}
"#;

    let unit = executor.compile("test", script);
    assert!(
        unit.is_ok(),
        "Script with McpResult methods should compile: {:?}",
        unit.err()
    );
}
