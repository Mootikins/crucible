//! Transport negotiation tests for ACP capability-aware MCP transport selection.
//!
//! These tests verify that `handshake()` correctly negotiates MCP
//! transport based on agent-reported capabilities per the ACP specification:
//!
//! - `McpServer::Stdio` — All agents MUST support this transport
//! - `McpServer::Http` — Only when agent reports `mcp_capabilities.http == true`
//!   and the daemon has a URL to give
//! - `McpServer::Sse` — Never: the daemon serves Streamable HTTP, not legacy SSE
//!
//! The selection tests read the `session/new` frame the mock agent received,
//! so they fail when the client sends the wrong server, not only when the
//! mock advertises the wrong capability.

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;

use mock_agent::{connect, logged, MockScript};
use serde_json::Value;

const MCP_URL: &str = "http://127.0.0.1:9999/mcp";

/// The MCP transport the client offered the agent in `session/new`.
#[derive(Debug, PartialEq, Eq)]
enum Offered {
    /// `McpServer::Http` with this URL.
    Http(String),
    /// `McpServer::Sse` with this URL.
    Sse(String),
    /// `McpServer::Stdio` (no `type` tag on the wire) with this command.
    Stdio(String),
}

/// A script that advertises these MCP transports.
fn script(mcp_http: bool, mcp_sse: bool) -> MockScript {
    MockScript {
        mcp_http,
        mcp_sse,
        ..MockScript::default()
    }
}

/// The MCP transports that the built-in profiles advertise.
fn opencode() -> MockScript {
    script(true, false)
}
fn claude_acp() -> MockScript {
    script(true, true)
}
fn gemini() -> MockScript {
    script(false, false)
}
fn codex() -> MockScript {
    script(true, false)
}

/// Connect a client to a mock agent with `script` and return the one MCP
/// server the client put in the `session/new` it sent. The value comes from
/// the frame log of the agent, not from any client accessor.
async fn offered_transport(script: MockScript, mcp_url: Option<&str>) -> Offered {
    let dir = tempfile::tempdir().expect("a temp dir");
    let log = dir.path().join("frames.jsonl");
    let script = MockScript {
        log: Some(log.clone()),
        ..script
    };
    let (mut client, _agent) = connect(script, None, None).await;
    let session = client
        .handshake(mcp_url, None)
        .await
        .expect("the handshake should succeed");
    assert!(!session.id().is_empty(), "Session ID should be non-empty");

    let frames = logged(&log, "session/new");
    let session_new = frames
        .first()
        .unwrap_or_else(|| panic!("the client sent no session/new; frames: {frames:?}"));
    let servers = session_new["mcpServers"]
        .as_array()
        .unwrap_or_else(|| panic!("session/new carried no mcpServers: {session_new}"));
    assert_eq!(servers.len(), 1, "one crucible MCP server: {servers:?}");
    let server = &servers[0];
    assert_eq!(server["name"], "crucible", "server name: {server}");
    let text = |key: &str| server[key].as_str().unwrap_or_default().to_string();
    match server.get("type").and_then(Value::as_str) {
        Some("http") => Offered::Http(text("url")),
        Some("sse") => Offered::Sse(text("url")),
        None => Offered::Stdio(text("command")),
        Some(other) => panic!("unknown MCP transport {other:?}: {server}"),
    }
}

/// Assert `offered` is the stdio server that runs `cru mcp --stdio`.
fn assert_stdio(offered: &Offered, name: &str) {
    match offered {
        Offered::Stdio(command) => assert!(
            command.ends_with("cru"),
            "{name}: the stdio server must run cru, got {command:?}"
        ),
        other => panic!("{name}: expected the stdio transport, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Phase A: Capability storage tests
// ---------------------------------------------------------------------------

/// Test 4: Capabilities are stored after the handshake
#[tokio::test]
async fn capabilities_stored_after_initialize() {
    let (mut client, _agent) = connect(script(true, true), None, None).await;

    // Before the handshake, capabilities should default to false
    assert!(
        !client.agent_supports_http_mcp(),
        "HTTP MCP should be false before the handshake"
    );

    client
        .handshake(None, None)
        .await
        .expect("the handshake should succeed");

    assert!(
        client.agent_supports_http_mcp(),
        "HTTP MCP should be true after the handshake with mcp_http=true"
    );
}

/// Test 5: Capabilities default to false when not initialized
#[tokio::test]
async fn capabilities_default_false_when_not_initialized() {
    let (client, _agent) = connect(opencode(), None, None).await;

    assert!(
        !client.agent_supports_http_mcp(),
        "HTTP MCP should default to false"
    );
}

// ---------------------------------------------------------------------------
// Transport selection, asserted on the `session/new` the agent received
// ---------------------------------------------------------------------------

/// An agent that reports HTTP support and a URL: the agent gets that URL
/// over Streamable HTTP.
#[tokio::test]
async fn agent_reporting_http_support_gets_http_transport() {
    assert_eq!(
        offered_transport(script(true, false), Some(MCP_URL)).await,
        Offered::Http(MCP_URL.to_string())
    );
}

/// An agent without HTTP support gets stdio even when a URL exists.
#[tokio::test]
async fn agent_without_http_support_falls_back_to_stdio() {
    assert_stdio(&offered_transport(gemini(), Some(MCP_URL)).await, "gemini");
}

/// No MCP URL: stdio, even for an agent that supports HTTP.
#[tokio::test]
async fn agent_with_no_mcp_url_always_gets_stdio() {
    assert_stdio(&offered_transport(opencode(), None).await, "opencode");
}

/// Each built-in profile gets the transport its advertisement earns when a
/// URL is available: HTTP where the profile reports HTTP, stdio otherwise.
/// No profile ever gets legacy SSE, because the daemon does not serve it.
#[tokio::test]
async fn each_builtin_profile_gets_valid_mcp_transport() {
    let cases: [(&str, MockScript, bool); 4] = [
        ("opencode", opencode(), true),
        ("claude_acp", claude_acp(), true),
        ("gemini", gemini(), false),
        ("codex", codex(), true),
    ];

    for (name, config, expects_http) in cases {
        let offered = offered_transport(config, Some(MCP_URL)).await;
        if expects_http {
            assert_eq!(offered, Offered::Http(MCP_URL.to_string()), "{name}");
        } else {
            assert_stdio(&offered, name);
        }
    }
}

/// An agent with SSE only gets stdio: the daemon does not serve legacy SSE.
#[tokio::test]
async fn agent_with_sse_only_gets_stdio_fallback() {
    assert_stdio(
        &offered_transport(script(false, true), Some(MCP_URL)).await,
        "sse-only",
    );
}

/// An agent with both gets Streamable HTTP, not legacy SSE.
#[tokio::test]
async fn agent_with_both_http_and_sse_gets_http_not_sse() {
    assert_eq!(
        offered_transport(claude_acp(), Some(MCP_URL)).await,
        Offered::Http(MCP_URL.to_string())
    );
}

/// Without a URL every profile completes the handshake on stdio.
#[tokio::test]
async fn stdio_fallback_always_creates_valid_session() {
    let profiles: [(&str, MockScript); 4] = [
        ("opencode", opencode()),
        ("claude_acp", claude_acp()),
        ("gemini", gemini()),
        ("codex", codex()),
    ];

    for (name, config) in profiles {
        assert_stdio(&offered_transport(config, None).await, name);
    }
}
