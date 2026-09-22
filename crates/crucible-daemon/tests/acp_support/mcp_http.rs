//! An MCP client over Streamable HTTP, played the way an ACP agent plays it,
//! for tests that call the daemon's in-process MCP host directly.

use serde_json::{json, Value};

/// Open an MCP session on a Streamable HTTP endpoint the way an agent does:
/// `initialize`, then `notifications/initialized`. Returns the session id the
/// server assigned.
pub async fn mcp_http_open_session(http: &reqwest::Client, url: &str) -> String {
    let init = http
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": {"name": "scripted-agent", "version": "0.1.0"}
                }
            })
            .to_string(),
        )
        .send()
        .await
        .expect("MCP initialize reaches the host");
    assert!(
        init.status().is_success(),
        "MCP initialize failed: {}",
        init.status()
    );
    let session_id = init
        .headers()
        .get("mcp-session-id")
        .expect("the MCP host assigns a session id")
        .to_str()
        .expect("the session id is ASCII")
        .to_string();

    let initialized = http
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", &session_id)
        .body(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string())
        .send()
        .await
        .expect("MCP initialized notification reaches the host");
    assert!(
        initialized.status().is_success(),
        "MCP initialized notification failed: {}",
        initialized.status()
    );

    session_id
}

/// Send one JSON-RPC request on an open MCP session and return the reply.
/// The host may answer as plain JSON or as one SSE `data:` event.
pub async fn mcp_http_request(
    http: &reqwest::Client,
    url: &str,
    session_id: &str,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    let response = http
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Mcp-Session-Id", session_id)
        .body(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string())
        .send()
        .await
        .unwrap_or_else(|e| panic!("MCP {method} reaches the host: {e}"));
    assert!(
        response.status().is_success(),
        "MCP {method} failed: {}",
        response.status()
    );
    let body = response.text().await.expect("read the MCP reply body");
    let json_text = body
        .lines()
        .find_map(|line| line.strip_prefix("data: ").filter(|d| d.starts_with('{')))
        .unwrap_or(&body);
    serde_json::from_str(json_text)
        .unwrap_or_else(|e| panic!("MCP {method} reply is not JSON ({e}): {body}"))
}
