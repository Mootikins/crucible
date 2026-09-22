//! The in-process MCP host (Streamable HTTP) that an ACP agent reaches
//! Crucible's tools through: URL format, reachability, shutdown and the tool
//! list.
//!
//! What the client puts in `session/new`'s `mcpServers` is asserted on the
//! wire in `mcp_server_frame.rs`. A tool result that crosses from this host
//! through an ACP turn is asserted in `tool_roundtrip.rs`.

use crate::support::mcp_http::{mcp_http_open_session, mcp_http_request};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::traits::KnowledgeRepository;
use crucible_daemon::test_support::{MockEmbeddingProvider, MockKnowledgeRepository};
use crucible_daemon::InProcessMcpHost;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Start an MCP host over `kiln`. The host binds a localhost port, so a
/// sandbox that denies the bind fails the test with the bind error.
async fn start_mcp_host(kiln: &Path) -> InProcessMcpHost {
    let knowledge_repo = Arc::new(MockKnowledgeRepository::new()) as Arc<dyn KnowledgeRepository>;
    let embedding_provider = Arc::new(MockEmbeddingProvider::new()) as Arc<dyn EmbeddingProvider>;
    InProcessMcpHost::start(
        kiln.to_path_buf(),
        kiln.to_path_buf(),
        knowledge_repo,
        embedding_provider,
        None,
        crucible_daemon::tools::containment::RootSet::Ambient,
    )
    .await
    .expect("the in-process MCP host binds to localhost")
}

/// The MCP `initialize` request body.
fn initialize_body() -> String {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test", "version": "0.1.0"}
        }
    })
    .to_string()
}

/// Test that the in-process MCP host starts and provides a valid URL
#[tokio::test]
async fn test_in_process_mcp_host_provides_valid_sse_url() {
    let temp = TempDir::new().unwrap();
    let host = start_mcp_host(temp.path()).await;

    let url = host.mcp_url();

    assert!(
        url.starts_with("http://127.0.0.1:"),
        "URL should be localhost"
    );
    assert!(url.ends_with("/mcp"), "URL should end with /mcp path");

    let port = host.address().port();
    assert!(
        port > 1024,
        "Port should be assigned and unprivileged (>1024)"
    );

    host.shutdown().await;
}

/// Test that the SSE endpoint is actually reachable
#[tokio::test]
async fn test_in_process_mcp_sse_endpoint_is_reachable() {
    let temp = TempDir::new().unwrap();
    let host = start_mcp_host(temp.path()).await;

    let resp = reqwest::Client::new()
        .post(host.mcp_url())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(initialize_body())
        .send()
        .await
        .expect("MCP endpoint should be reachable");

    assert!(
        resp.status().is_success(),
        "MCP endpoint should return success status, got: {}",
        resp.status()
    );

    host.shutdown().await;
}

/// Test graceful shutdown of MCP host
#[tokio::test]
async fn test_in_process_mcp_host_graceful_shutdown() {
    let temp = TempDir::new().unwrap();
    let host = start_mcp_host(temp.path()).await;

    let url = host.mcp_url();
    let client = reqwest::Client::new();
    let probe = || {
        client
            .get(&url)
            .header("Accept", "text/event-stream")
            .timeout(Duration::from_millis(500))
            .send()
    };

    probe().await.expect("Endpoint should work before shutdown");

    host.shutdown().await;

    // The listener may close a moment after `shutdown` returns. Wait for
    // the endpoint to refuse, with a deadline, rather than for a fixed time.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if probe().await.is_err() {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Endpoint should not be reachable after shutdown"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn test_streamable_http_accept_header_without_sse_still_succeeds() {
    let temp = TempDir::new().unwrap();
    let host = start_mcp_host(temp.path()).await;

    let init_resp = reqwest::Client::new()
        .post(host.mcp_url())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(initialize_body())
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

/// Test that tools/list over HTTP returns all 15 tools including delegate_session.
/// Note: The MCP HTTP endpoint uses the rmcp tool_router directly (all tools),
/// not the filtered list_tools() helper. delegate_session is always present in
/// the HTTP endpoint; filtering only applies to the Rust API (list_tools() method).
#[tokio::test]
async fn test_tools_list_over_http_returns_delegate_session() {
    let temp = TempDir::new().unwrap();
    let host = start_mcp_host(temp.path()).await;

    let url = host.mcp_url();
    let client = reqwest::Client::new();
    let session_id = mcp_http_open_session(&client, &url).await;
    let parsed = mcp_http_request(&client, &url, &session_id, 2, "tools/list", json!({})).await;

    let tools = parsed["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("should have tools array: {parsed}"));
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();

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
