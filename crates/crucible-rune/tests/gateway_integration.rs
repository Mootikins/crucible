//! Integration tests for MCP Gateway functionality
//!
//! These tests verify the gateway's tool filtering, prefix handling,
//! event emission, and manager coordination.

use crucible_rune::event_bus::{EventBus, EventType, Handler};
use crucible_rune::mcp_gateway::{
    ContentBlock, McpGatewayManager, ToolCallResult, TransportConfig, UpstreamConfig,
    UpstreamMcpClient, UpstreamTool,
};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

/// Create a test upstream config with stdio transport
fn test_config(name: &str, prefix: Option<&str>) -> UpstreamConfig {
    UpstreamConfig {
        name: name.to_string(),
        transport: TransportConfig::Stdio {
            command: "echo".to_string(),
            args: vec!["test".to_string()],
            env: vec![],
        },
        prefix: prefix.map(String::from),
        allowed_tools: None,
        blocked_tools: None,
        auto_reconnect: false,
    }
}

/// Create a test tool for mocking
fn test_tool(name: &str, prefix: Option<&str>, upstream: &str) -> UpstreamTool {
    let prefixed = match prefix {
        Some(p) => format!("{}{}", p, name),
        None => name.to_string(),
    };
    UpstreamTool {
        name: name.to_string(),
        prefixed_name: prefixed,
        description: Some(format!("Test tool: {}", name)),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"}
            }
        }),
        upstream: upstream.to_string(),
    }
}

// =============================================================================
// Client Configuration Tests
// =============================================================================

#[test]
fn test_client_creation_with_config() {
    let config = test_config("github", Some("gh_"));
    let client = UpstreamMcpClient::new(config);

    assert_eq!(client.name(), "github");
    assert_eq!(client.prefix(), Some("gh_"));
}

#[test]
fn test_client_without_prefix() {
    let config = test_config("filesystem", None);
    let client = UpstreamMcpClient::new(config);

    assert_eq!(client.name(), "filesystem");
    assert_eq!(client.prefix(), None);
}

#[tokio::test]
async fn test_client_initial_state() {
    let config = test_config("test", None);
    let client = UpstreamMcpClient::new(config);

    assert!(!client.is_connected().await);
    assert!(client.server_info().is_none());
    assert!(client.tools().await.is_empty());
}

// =============================================================================
// Tool Filtering Tests
// =============================================================================

#[test]
fn test_tool_allowed_no_filters() {
    let config = test_config("test", None);
    let client = UpstreamMcpClient::new(config);

    // With no filters, all tools should be allowed
    assert!(client_allows_tool(&client, "any_tool"));
    assert!(client_allows_tool(&client, "search_code"));
    assert!(client_allows_tool(&client, "delete_repo"));
}

#[test]
fn test_tool_whitelist_filtering() {
    let config = UpstreamConfig {
        name: "github".to_string(),
        transport: TransportConfig::Stdio {
            command: "test".to_string(),
            args: vec![],
            env: vec![],
        },
        prefix: Some("gh_".to_string()),
        allowed_tools: Some(vec!["search_*".to_string(), "get_*".to_string()]),
        blocked_tools: None,
        auto_reconnect: false,
    };
    let client = UpstreamMcpClient::new(config);

    // Matching patterns should be allowed
    assert!(client_allows_tool(&client, "search_code"));
    assert!(client_allows_tool(&client, "search_repositories"));
    assert!(client_allows_tool(&client, "get_user"));
    assert!(client_allows_tool(&client, "get_repo"));

    // Non-matching should be blocked
    assert!(!client_allows_tool(&client, "delete_repo"));
    assert!(!client_allows_tool(&client, "create_issue"));
}

#[test]
fn test_tool_blacklist_filtering() {
    let config = UpstreamConfig {
        name: "github".to_string(),
        transport: TransportConfig::Stdio {
            command: "test".to_string(),
            args: vec![],
            env: vec![],
        },
        prefix: None,
        allowed_tools: None,
        blocked_tools: Some(vec!["delete_*".to_string(), "dangerous".to_string()]),
        auto_reconnect: false,
    };
    let client = UpstreamMcpClient::new(config);

    // Non-blocked tools should be allowed
    assert!(client_allows_tool(&client, "search_code"));
    assert!(client_allows_tool(&client, "get_user"));
    assert!(client_allows_tool(&client, "create_issue"));

    // Blocked tools should be denied
    assert!(!client_allows_tool(&client, "delete_repo"));
    assert!(!client_allows_tool(&client, "delete_user"));
    assert!(!client_allows_tool(&client, "dangerous"));
}

#[test]
fn test_blacklist_overrides_whitelist() {
    let config = UpstreamConfig {
        name: "test".to_string(),
        transport: TransportConfig::Stdio {
            command: "test".to_string(),
            args: vec![],
            env: vec![],
        },
        prefix: None,
        allowed_tools: Some(vec!["*".to_string()]), // Allow all
        blocked_tools: Some(vec!["dangerous_*".to_string()]), // But block dangerous
        auto_reconnect: false,
    };
    let client = UpstreamMcpClient::new(config);

    assert!(client_allows_tool(&client, "safe_tool"));
    assert!(client_allows_tool(&client, "search_code"));
    assert!(!client_allows_tool(&client, "dangerous_delete"));
    assert!(!client_allows_tool(&client, "dangerous_admin"));
}

/// Helper to check if a client allows a tool
fn client_allows_tool(client: &UpstreamMcpClient, tool_name: &str) -> bool {
    client.is_tool_allowed(tool_name)
}

// =============================================================================
// Tool Discovery and Lookup Tests
// =============================================================================

#[tokio::test]
async fn test_update_tools() {
    let config = test_config("github", Some("gh_"));
    let client = UpstreamMcpClient::new(config);

    let tools = vec![
        test_tool("search_code", Some("gh_"), "github"),
        test_tool("get_user", Some("gh_"), "github"),
        test_tool("create_issue", Some("gh_"), "github"),
    ];

    client.update_tools(tools).await;

    let discovered = client.tools().await;
    assert_eq!(discovered.len(), 3);
}

#[tokio::test]
async fn test_update_tools_filters_blocked() {
    let config = UpstreamConfig {
        name: "github".to_string(),
        transport: TransportConfig::Stdio {
            command: "test".to_string(),
            args: vec![],
            env: vec![],
        },
        prefix: Some("gh_".to_string()),
        allowed_tools: None,
        blocked_tools: Some(vec!["delete_*".to_string()]),
        auto_reconnect: false,
    };
    let client = UpstreamMcpClient::new(config);

    let tools = vec![
        test_tool("search_code", Some("gh_"), "github"),
        test_tool("delete_repo", Some("gh_"), "github"), // Should be filtered
        test_tool("get_user", Some("gh_"), "github"),
    ];

    client.update_tools(tools).await;

    let discovered = client.tools().await;
    assert_eq!(discovered.len(), 2);
    assert!(discovered.iter().all(|t| t.name != "delete_repo"));
}

#[tokio::test]
async fn test_get_tool_by_name() {
    let config = test_config("github", Some("gh_"));
    let client = UpstreamMcpClient::new(config);

    let tools = vec![test_tool("search_code", Some("gh_"), "github")];
    client.update_tools(tools).await;

    // Get by original name
    let tool = client.get_tool("search_code").await;
    assert!(tool.is_some());
    assert_eq!(tool.unwrap().name, "search_code");
}

#[tokio::test]
async fn test_get_tool_by_prefixed_name() {
    let config = test_config("github", Some("gh_"));
    let client = UpstreamMcpClient::new(config);

    let tools = vec![test_tool("search_code", Some("gh_"), "github")];
    client.update_tools(tools).await;

    // Get by prefixed name
    let tool = client.get_tool_by_prefixed_name("gh_search_code").await;
    assert!(tool.is_some());
    assert_eq!(tool.unwrap().prefixed_name, "gh_search_code");
}

// =============================================================================
// Manager Tests
// =============================================================================

#[tokio::test]
async fn test_manager_add_and_get_client() {
    let bus = EventBus::new();
    let mut manager = McpGatewayManager::new(bus);

    let config = test_config("github", Some("gh_"));
    manager.add_client(config);

    assert!(manager.get_client("github").is_some());
    assert!(manager.get_client("nonexistent").is_none());
}

#[tokio::test]
async fn test_manager_multiple_clients() {
    let bus = EventBus::new();
    let mut manager = McpGatewayManager::new(bus);

    manager.add_client(test_config("github", Some("gh_")));
    manager.add_client(test_config("filesystem", Some("fs_")));
    manager.add_client(test_config("context7", Some("c7_")));

    assert!(manager.get_client("github").is_some());
    assert!(manager.get_client("filesystem").is_some());
    assert!(manager.get_client("context7").is_some());
    assert_eq!(manager.clients().count(), 3);
}

#[tokio::test]
async fn test_manager_all_tools() {
    let bus = EventBus::new();
    let mut manager = McpGatewayManager::new(bus);

    // Add two clients with different tools
    let gh_client = manager.add_client(test_config("github", Some("gh_")));
    let fs_client = manager.add_client(test_config("filesystem", Some("fs_")));

    gh_client
        .update_tools(vec![
            test_tool("search_code", Some("gh_"), "github"),
            test_tool("get_user", Some("gh_"), "github"),
        ])
        .await;

    fs_client
        .update_tools(vec![
            test_tool("read_file", Some("fs_"), "filesystem"),
            test_tool("list_directory", Some("fs_"), "filesystem"),
        ])
        .await;

    let all_tools = manager.all_tools().await;
    assert_eq!(all_tools.len(), 4);
}

#[tokio::test]
async fn test_manager_find_client_for_tool() {
    let bus = EventBus::new();
    let mut manager = McpGatewayManager::new(bus);

    let gh_client = manager.add_client(test_config("github", Some("gh_")));
    let fs_client = manager.add_client(test_config("filesystem", Some("fs_")));

    gh_client
        .update_tools(vec![test_tool("search_code", Some("gh_"), "github")])
        .await;
    fs_client
        .update_tools(vec![test_tool("read_file", Some("fs_"), "filesystem")])
        .await;

    // Find by prefixed name
    let client = manager.find_client_for_tool("gh_search_code").await;
    assert!(client.is_some());
    assert_eq!(client.unwrap().name(), "github");

    let client = manager.find_client_for_tool("fs_read_file").await;
    assert!(client.is_some());
    assert_eq!(client.unwrap().name(), "filesystem");

    // Non-existent tool
    let client = manager.find_client_for_tool("unknown_tool").await;
    assert!(client.is_none());
}

// =============================================================================
// Event Integration Tests
// =============================================================================

#[tokio::test]
async fn test_tool_discovered_event_emitted() {
    init_test_logging();

    let discovered_count = Arc::new(AtomicUsize::new(0));
    let count_clone = Arc::clone(&discovered_count);

    let mut bus = EventBus::new();
    bus.register(
        Handler::new(
            "count_discoveries",
            EventType::ToolDiscovered,
            "*",
            move |_ctx, event| {
                count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(event)
            },
        )
        .with_priority(100),
    );

    let config = test_config("github", Some("gh_"));
    let client = UpstreamMcpClient::new(config).with_event_bus(Arc::new(RwLock::new(bus)));

    let tools = vec![
        test_tool("search_code", Some("gh_"), "github"),
        test_tool("get_user", Some("gh_"), "github"),
    ];

    client.update_tools(tools).await;

    // Each tool should trigger a tool:discovered event
    assert_eq!(discovered_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn test_event_handler_receives_tool_metadata() {
    init_test_logging();

    let captured_names = Arc::new(RwLock::new(Vec::<String>::new()));
    let names_clone = Arc::clone(&captured_names);

    let mut bus = EventBus::new();
    bus.register(
        Handler::new(
            "capture_names",
            EventType::ToolDiscovered,
            "*",
            move |_ctx, event| {
                let name = event.identifier.clone();
                let names = Arc::clone(&names_clone);
                tokio::spawn(async move {
                    names.write().await.push(name);
                });
                Ok(event)
            },
        )
        .with_priority(100),
    );

    let config = test_config("context7", Some("c7_"));
    let client = UpstreamMcpClient::new(config).with_event_bus(Arc::new(RwLock::new(bus)));

    client
        .update_tools(vec![
            test_tool("resolve-library-id", Some("c7_"), "context7"),
            test_tool("get-library-docs", Some("c7_"), "context7"),
        ])
        .await;

    // Allow async handler to complete
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let names = captured_names.read().await;
    assert!(names.contains(&"c7_resolve-library-id".to_string()));
    assert!(names.contains(&"c7_get-library-docs".to_string()));
}

// =============================================================================
// Transport Configuration Tests
// =============================================================================

#[test]
fn test_stdio_transport_config() {
    let config = TransportConfig::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "@upstash/context7-mcp".to_string()],
        env: vec![("API_KEY".to_string(), "secret".to_string())],
    };

    let json = serde_json::to_value(&config).unwrap();
    assert_eq!(json["type"], "stdio");
    assert_eq!(json["command"], "npx");
    assert_eq!(json["args"][0], "-y");
}

#[test]
fn test_sse_transport_config() {
    let config = TransportConfig::Sse {
        url: "http://localhost:3000/sse".to_string(),
        auth_header: Some("Bearer token123".to_string()),
    };

    let json = serde_json::to_value(&config).unwrap();
    assert_eq!(json["type"], "sse");
    assert_eq!(json["url"], "http://localhost:3000/sse");
    assert_eq!(json["auth_header"], "Bearer token123");
}

#[test]
fn test_transport_config_roundtrip() {
    let original = TransportConfig::Stdio {
        command: "node".to_string(),
        args: vec!["server.js".to_string()],
        env: vec![("NODE_ENV".to_string(), "production".to_string())],
    };

    let json = serde_json::to_string(&original).unwrap();
    let parsed: TransportConfig = serde_json::from_str(&json).unwrap();

    match parsed {
        TransportConfig::Stdio { command, args, env } => {
            assert_eq!(command, "node");
            assert_eq!(args, vec!["server.js"]);
            assert_eq!(
                env,
                vec![("NODE_ENV".to_string(), "production".to_string())]
            );
        }
        _ => panic!("Expected Stdio transport"),
    }
}

// =============================================================================
// Content Block Tests
// =============================================================================

#[test]
fn test_content_block_text() {
    let block = ContentBlock::Text {
        text: "Hello, world!".to_string(),
    };

    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "text");
    assert_eq!(json["text"], "Hello, world!");
}

#[test]
fn test_content_block_image() {
    let block = ContentBlock::Image {
        data: "base64data...".to_string(),
        mime_type: "image/png".to_string(),
    };

    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "image");
    assert_eq!(json["mime_type"], "image/png");
}

#[test]
fn test_tool_call_result_success() {
    let result = ToolCallResult {
        content: vec![ContentBlock::Text {
            text: "Result data".to_string(),
        }],
        is_error: false,
    };

    assert!(!result.is_error);
    assert_eq!(result.content.len(), 1);
}

#[test]
fn test_tool_call_result_error() {
    let result = ToolCallResult {
        content: vec![ContentBlock::Text {
            text: "Error: Something went wrong".to_string(),
        }],
        is_error: true,
    };

    assert!(result.is_error);
}
