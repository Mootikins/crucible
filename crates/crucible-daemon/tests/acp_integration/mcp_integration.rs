//! The in-process MCP host (Streamable HTTP) that an ACP agent reaches
//! Crucible's tools through: URL format, reachability, shutdown and the tool
//! list.
//!
//! What the client puts in `session/new`'s `mcpServers` is asserted on the
//! wire in `acp_transport_negotiation.rs`. A tool result that crosses from this host
//! through an ACP turn is asserted in `tool_roundtrip.rs`.

use crate::support::mcp_http::start_host;
use serde_json::json;
use std::time::Duration;
use tempfile::TempDir;

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
    let host = start_host(temp.path(), None).await;

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
    let host = start_host(temp.path(), None).await;

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
    let host = start_host(temp.path(), None).await;

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
    let host = start_host(temp.path(), None).await;

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
