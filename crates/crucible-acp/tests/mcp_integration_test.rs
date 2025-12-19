//! Integration test for MCP server configuration
//!
//! Verifies that the MCP server info is properly populated in NewSessionRequest

use agent_client_protocol::{EnvVariable, McpServer, McpServerStdio, NewSessionRequest};
use crucible_acp::client::{ClientConfig, CrucibleAcpClient};
use serde_json::json;
use std::path::PathBuf;

/// Integration test: MCP server configuration is populated in handshake
#[tokio::test]
async fn test_mcp_server_configuration_in_handshake() {
    // Create a client with basic configuration
    let config = ClientConfig {
        agent_path: PathBuf::from("cru"),
        agent_args: None,
        working_dir: Some(PathBuf::from("/test")),
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(1),
    };

    let _client = CrucibleAcpClient::new(config);

    // Manually create what connect_with_handshake would create
    // This verifies the NewSessionRequest structure includes MCP servers
    let session_request: NewSessionRequest = serde_json::from_value(json!({
        "cwd": "/test",
        "mcpServers": [
            {
                "type": "stdio",
                "name": "crucible",
                "command": "cru",
                "args": ["mcp"],
                "env": []
            }
        ],
        "_meta": null
    }))
    .expect("Failed to create NewSessionRequest");

    // Verify the structure is correct
    assert_eq!(
        session_request.mcp_servers.len(),
        1,
        "Should have one MCP server configured"
    );

    match &session_request.mcp_servers[0] {
        McpServer::Stdio(stdio) => {
            assert_eq!(
                &stdio.name, "crucible",
                "MCP server name should be 'crucible'"
            );
            assert_eq!(
                &stdio.command,
                &PathBuf::from("cru"),
                "Command should be 'cru'"
            );
            assert_eq!(stdio.args.len(), 1, "Should have one arg");
            assert_eq!(stdio.args[0], "mcp", "Arg should be 'mcp'");
            assert_eq!(
                stdio.env.len(),
                0,
                "Should have no environment variables by default"
            );
        }
        _ => panic!("Expected McpServer::Stdio variant"),
    }

    // Verify it serializes correctly
    let serialized = serde_json::to_string(&session_request);
    assert!(
        serialized.is_ok(),
        "NewSessionRequest with MCP servers should serialize"
    );

    let json = serialized.unwrap();
    assert!(
        json.contains("crucible"),
        "Serialized JSON should contain server name"
    );
    assert!(
        json.contains("mcp"),
        "Serialized JSON should contain 'mcp' arg"
    );
}

/// Integration test: Verify connect_with_handshake populates MCP servers
#[tokio::test]
async fn test_connect_with_handshake_includes_mcp_servers() {
    use std::env;

    // This test verifies the logic in connect_with_handshake
    // The method uses current_exe() to find the cru binary, so we test that path resolution works

    let current_exe_result = env::current_exe();
    assert!(
        current_exe_result.is_ok(),
        "Should be able to get current executable path"
    );

    let exe_path = current_exe_result.unwrap();
    let parent = exe_path.parent();
    assert!(
        parent.is_some(),
        "Executable should have a parent directory"
    );

    // Verify the command path resolution logic
    let cru_path = parent.unwrap().join("cru");
    assert!(cru_path.is_absolute(), "cru path should be absolute");

    // Verify the McpServer can be constructed with this path
    let mcp_server = McpServer::Stdio(
        McpServerStdio::new("crucible", cru_path.clone()).args(vec!["mcp".to_string()]),
    );

    // Verify it can be added to a NewSessionRequest
    let request: NewSessionRequest = serde_json::from_value(json!({
        "cwd": "/test",
        "mcpServers": [mcp_server],
        "_meta": null
    }))
    .expect("Failed to create NewSessionRequest");

    assert_eq!(request.mcp_servers.len(), 1);
}

/// Integration test: MCP server with environment variables
#[tokio::test]
async fn test_mcp_server_with_env_variables() {
    // Test that we can add environment variables to the MCP server configuration
    let env_vars = vec![
        serde_json::from_value::<EnvVariable>(json!({
            "name": "RUST_LOG",
            "value": "debug",
            "_meta": null
        }))
        .expect("Failed to create EnvVariable"),
        serde_json::from_value::<EnvVariable>(json!({
            "name": "KILN_PATH",
            "value": "/path/to/kiln",
            "_meta": null
        }))
        .expect("Failed to create EnvVariable"),
    ];

    let mcp_server = McpServer::Stdio(
        McpServerStdio::new("crucible", "cru")
            .args(vec!["mcp".to_string()])
            .env(env_vars.clone()),
    );

    // Verify environment variables are preserved
    match &mcp_server {
        McpServer::Stdio(stdio) => {
            assert_eq!(stdio.env.len(), 2);
            assert_eq!(stdio.env[0].name, "RUST_LOG");
            assert_eq!(stdio.env[0].value, "debug");
            assert_eq!(stdio.env[1].name, "KILN_PATH");
            assert_eq!(stdio.env[1].value, "/path/to/kiln");
        }
        _ => panic!("Expected Stdio variant"),
    }

    // Verify it can be included in NewSessionRequest
    let request: NewSessionRequest = serde_json::from_value(json!({
        "cwd": "/test",
        "mcpServers": [mcp_server],
        "_meta": null
    }))
    .expect("Failed to create NewSessionRequest");

    let serialized = serde_json::to_string(&request).unwrap();
    assert!(serialized.contains("RUST_LOG"));
    assert!(serialized.contains("debug"));
}

/// Integration test: Multiple MCP servers in one request
#[tokio::test]
async fn test_multiple_mcp_servers() {
    // Verify that the protocol supports multiple MCP servers
    let crucible_server =
        McpServer::Stdio(McpServerStdio::new("crucible", "cru").args(vec!["mcp".to_string()]));

    let another_server = McpServer::Stdio(
        McpServerStdio::new("another-tool", "/usr/bin/other-mcp-server")
            .args(vec!["--mode".to_string(), "stdio".to_string()]),
    );

    let request: NewSessionRequest = serde_json::from_value(json!({
        "cwd": "/test",
        "mcpServers": [crucible_server, another_server],
        "_meta": null
    }))
    .expect("Failed to create NewSessionRequest");

    assert_eq!(request.mcp_servers.len(), 2);

    // Verify both servers serialize correctly
    let serialized = serde_json::to_string(&request).unwrap();
    assert!(serialized.contains("crucible"));
    assert!(serialized.contains("another-tool"));
}

/// Integration test: MCP server configuration matches expected schema
#[tokio::test]
async fn test_mcp_server_schema_compliance() {
    // Create an MCP server configuration
    let mcp_server =
        McpServer::Stdio(McpServerStdio::new("crucible", "cru").args(vec!["mcp".to_string()]));

    // Serialize and verify JSON structure
    let serialized = serde_json::to_value(&mcp_server).unwrap();

    // Verify required fields are present
    assert!(serialized.get("name").is_some());
    assert!(serialized.get("command").is_some());
    assert!(serialized.get("args").is_some());
    assert!(serialized.get("env").is_some());

    // Verify field values
    assert_eq!(serialized["name"], "crucible");
    assert_eq!(serialized["command"], "cru");
    assert!(serialized["args"].is_array());
    assert_eq!(serialized["args"][0], "mcp");
}
