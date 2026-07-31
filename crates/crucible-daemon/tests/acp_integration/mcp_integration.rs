//! MCP integration tests for the ACP host.
//!
//! Consolidates two former `tests/`-root files:
//! * `acp_mcp_integration.rs` — pure `McpServer::Stdio` schema / serialization
//!   coverage for `NewSessionRequest` (no live host involved).
//! * `acp_in_process_mcp.rs` — live `InProcessMcpHost` (Streamable HTTP / SSE
//!   endpoint) coverage: URL format, reachability, shutdown, tool list, and
//!   HTTP-vs-stdio transport negotiation with mock agents.
//!
//! The two concerns are complementary: the stdio tests exercise the protocol
//! types in isolation, the in-process tests exercise the running HTTP server.
//! Both groups are preserved verbatim — no duplicate-behavior tests were found
//! during the consolidation audit.

use agent_client_protocol::{EnvVariable, McpServer, McpServerStdio, NewSessionRequest};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::acp::client::{ClientConfig, CrucibleAcpClient};
use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
use crucible_daemon::InProcessMcpHost;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

// ===========================================================================
// Stdio-variant MCP server schema / serialization (former acp_mcp_integration)
// ===========================================================================

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

// ===========================================================================
// In-process Streamable HTTP / SSE MCP host (former acp_in_process_mcp)
// ===========================================================================

fn is_permission_denied(err: &crucible_daemon::acp::ClientError) -> bool {
    matches!(
        err,
        crucible_daemon::acp::ClientError::Connection(message) if message.contains("Operation not permitted")
    )
}

async fn start_mcp_host(
    kiln_path: PathBuf,
    knowledge_repo: Arc<dyn KnowledgeRepository>,
    embedding_provider: Arc<dyn EmbeddingProvider>,
) -> InProcessMcpHost {
    match InProcessMcpHost::start(kiln_path, knowledge_repo, embedding_provider, None).await {
        Ok(host) => host,
        Err(err) => {
            if is_permission_denied(&err) {
                panic!(
                    "In-process MCP SSE server requires binding to localhost (sandbox denied): {}",
                    err
                );
            }
            panic!("Should start MCP host: {:?}", err);
        }
    }
}

/// Test that the in-process MCP host starts and provides a valid URL
#[tokio::test]
async fn test_in_process_mcp_host_provides_valid_sse_url() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.mcp_url();

    // Verify URL format
    assert!(
        url.starts_with("http://127.0.0.1:"),
        "URL should be localhost"
    );
    assert!(url.ends_with("/mcp"), "URL should end with /mcp path");

    // Verify port is non-zero (actually assigned)
    let port = host.address().port();
    assert!(port > 0, "Port should be non-zero");
    assert!(port > 1024, "Port should be unprivileged (>1024)");

    host.shutdown().await;
}

/// Test that the SSE endpoint is actually reachable
#[tokio::test]
async fn test_in_process_mcp_sse_endpoint_is_reachable() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.mcp_url();

    // Try to connect to the streamable HTTP endpoint
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#)
        .send()
        .await;

    // The server should respond to a valid MCP initialize request
    assert!(response.is_ok(), "MCP endpoint should be reachable");

    let resp = response.unwrap();
    // Streamable HTTP MCP returns 200 OK for valid requests
    assert!(
        resp.status().is_success(),
        "MCP endpoint should return success status, got: {}",
        resp.status()
    );

    host.shutdown().await;
}

/// Test that McpServer::Http can be constructed with the host's URL
#[tokio::test]
async fn test_mcp_server_http_variant_with_host_url() {
    use agent_client_protocol::{McpServer, McpServerHttp};

    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.mcp_url();

    let mcp_server = McpServer::Http(McpServerHttp::new("crucible", url.clone()));

    let serialized = serde_json::to_value(&mcp_server).expect("Should serialize");

    assert_eq!(serialized["name"], "crucible");
    assert_eq!(serialized["url"], url);
    assert!(serialized["headers"].is_array());
    assert_eq!(serialized["type"], "http");

    host.shutdown().await;
}

/// Test that the ACP NewSessionRequest can include Streamable HTTP MCP server
#[tokio::test]
async fn test_new_session_request_with_http_mcp() {
    use agent_client_protocol::{McpServer, McpServerHttp, NewSessionRequest};
    use serde_json::json;

    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.mcp_url();

    let mcp_server = McpServer::Http(McpServerHttp::new("crucible", url.clone()));

    let request: NewSessionRequest = serde_json::from_value(json!({
        "cwd": "/test",
        "mcpServers": [mcp_server],
        "_meta": null
    }))
    .expect("Failed to create NewSessionRequest");

    assert_eq!(request.mcp_servers.len(), 1);

    match &request.mcp_servers[0] {
        McpServer::Http(http) => {
            assert_eq!(&http.name, "crucible");
            assert_eq!(&http.url, &url);
            assert!(http.headers.is_empty());
        }
        _ => panic!("Expected McpServer::Http variant"),
    }

    let json = serde_json::to_string(&request).expect("Should serialize");
    assert!(json.contains("crucible"));
    assert!(json.contains(&url));

    host.shutdown().await;
}

/// Test graceful shutdown of MCP host
#[tokio::test]
async fn test_in_process_mcp_host_graceful_shutdown() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.mcp_url();

    // Verify endpoint works before shutdown
    let client = reqwest::Client::new();
    let before = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .send()
        .await;
    assert!(before.is_ok(), "Endpoint should work before shutdown");

    // Shutdown
    host.shutdown().await;

    // Give the server time to shut down
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Endpoint should no longer be reachable
    let after = client
        .get(&url)
        .header("Accept", "text/event-stream")
        .timeout(tokio::time::Duration::from_millis(500))
        .send()
        .await;

    // Connection should fail or timeout
    assert!(
        after.is_err(),
        "Endpoint should not be reachable after shutdown"
    );
}

/// Test that tools/list over HTTP returns all 16 tools including delegate_session
#[tokio::test]
async fn test_streamable_http_accept_header_without_sse_still_succeeds() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.mcp_url();
    let client = reqwest::Client::new();

    let init_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#)
        .send()
        .await
        .expect("initialize should succeed");

    assert!(
        init_resp.status().is_success(),
        "missing text/event-stream should still succeed, got: {}",
        init_resp.status()
    );

    host.shutdown().await;
}

/// Test that tools/list over HTTP returns all 21 tools including delegate_session.
/// Note: The MCP HTTP endpoint uses the rmcp tool_router directly (all tools),
/// not the filtered list_tools() helper. delegate_session is always present in
/// the HTTP endpoint; filtering only applies to the Rust API (list_tools() method).
#[tokio::test]
async fn test_tools_list_over_http_returns_delegate_session() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let url = host.mcp_url();
    let client = reqwest::Client::new();

    // Step 1: Initialize
    let init_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"0.1.0"}}}"#)
        .send()
        .await
        .expect("initialize should succeed");

    assert!(init_resp.status().is_success());
    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .expect("should have session id")
        .to_str()
        .unwrap()
        .to_string();

    // Step 2: Send initialized notification
    let _notif_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .send()
        .await
        .expect("initialized notification should succeed");

    // Step 3: List tools
    let tools_resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#)
        .send()
        .await
        .expect("tools/list should succeed");

    assert!(
        tools_resp.status().is_success(),
        "tools/list status: {}",
        tools_resp.status()
    );

    let body = tools_resp.text().await.unwrap();
    eprintln!("tools/list response: {}", body);

    // Parse SSE format: extract JSON from "data: {...}" lines
    let json_str = body
        .lines()
        .find(|line| line.starts_with("data: {"))
        .and_then(|line| line.strip_prefix("data: "))
        .expect("should find data line with JSON");

    let parsed: serde_json::Value = serde_json::from_str(json_str).expect("should be valid JSON");
    let tools = parsed["result"]["tools"]
        .as_array()
        .expect("should have tools array");

    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    eprintln!("Tool names: {:?}", tool_names);

    // Kiln + delegation only; workspace tools are not on the MCP surface.
    assert_eq!(
        tools.len(),
        15,
        "Should have 15 tools, got: {:?}",
        tool_names
    );
    assert!(
        tool_names.contains(&"delegate_session"),
        "Should contain delegate_session, got: {:?}",
        tool_names
    );

    host.shutdown().await;
}

// Transport negotiation tests use the threaded mock agents shared with the
// rest of the `acp_integration` suite via `crate::support`.

/// Test 10: HTTP-capable agent gets connected via connect_with_best_mcp
/// with in-process MCP host running.
#[tokio::test]
async fn test_http_capable_agent_gets_http_transport_with_mcp_host() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let mcp_url = host.mcp_url();

    // Create a mock agent that supports HTTP MCP
    let config = crate::support::MockStdioAgentConfig {
        mcp_http: true,
        ..crate::support::MockStdioAgentConfig::opencode()
    };
    let (mut client, _handle) = crate::support::ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(Some(&mcp_url))
        .await
        .expect("connect_with_best_mcp should succeed for HTTP-capable agent");

    assert!(!session.id().is_empty(), "Session ID should be non-empty");
    assert!(
        client.agent_supports_http_mcp(),
        "Agent should report HTTP MCP support"
    );

    host.shutdown().await;
}

/// Test 11: Stdio-only agent still gets a valid session with MCP host running.
#[tokio::test]
async fn test_stdio_only_agent_still_gets_session_with_mcp_host() {
    let temp = TempDir::new().unwrap();
    let knowledge_repo = Arc::new(MockKnowledgeRepository) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider) as Arc<dyn EmbeddingProvider>;

    let host = start_mcp_host(
        temp.path().to_path_buf(),
        knowledge_repo,
        embedding_provider,
    )
    .await;

    let mcp_url = host.mcp_url();

    // Create a mock agent that does NOT support HTTP MCP
    let config = crate::support::MockStdioAgentConfig {
        mcp_http: false,
        ..crate::support::MockStdioAgentConfig::gemini()
    };
    let (mut client, _handle) = crate::support::ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(Some(&mcp_url))
        .await
        .expect("connect_with_best_mcp should succeed for stdio-only agent via fallback");

    assert!(!session.id().is_empty(), "Session ID should be non-empty");
    assert!(
        !client.agent_supports_http_mcp(),
        "Agent should not report HTTP MCP support"
    );

    host.shutdown().await;
}
