//! Transport negotiation tests for ACP capability-aware MCP transport selection.
//!
//! These tests verify that `connect_with_best_mcp()` correctly negotiates MCP
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

#[path = "acp_support/mod.rs"]
mod support;

use std::sync::{Arc, Mutex};

use serde_json::Value;
use support::{MockStdioAgentConfig, ThreadedMockAgent};

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

/// Connect a client to a mock agent with `config` and return the one MCP
/// server the client put in the `session/new` it sent. The value comes from
/// the frame that crossed the transport, not from any client accessor.
async fn offered_transport(config: MockStdioAgentConfig, mcp_url: Option<&str>) -> Offered {
    let log = Arc::new(Mutex::new(Vec::<Value>::new()));
    let config = MockStdioAgentConfig {
        request_log: Some(log.clone()),
        ..config
    };
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);
    let session = client
        .connect_with_best_mcp(mcp_url)
        .await
        .expect("connect_with_best_mcp should succeed");
    assert!(!session.id().is_empty(), "Session ID should be non-empty");

    let frames = log.lock().unwrap().clone();
    let session_new = frames
        .iter()
        .find(|frame| frame["method"] == "session/new")
        .unwrap_or_else(|| panic!("the client sent no session/new; frames: {frames:?}"));
    let servers = session_new["params"]["mcpServers"]
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

/// Test 4: Capabilities are stored after initialize()
#[tokio::test]
async fn capabilities_stored_after_initialize() {
    let config = MockStdioAgentConfig {
        mcp_http: true,
        mcp_sse: true,
        ..MockStdioAgentConfig::opencode()
    };
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    // Before initialize, capabilities should default to false
    assert!(
        !client.agent_supports_http_mcp(),
        "HTTP MCP should be false before initialize"
    );
    assert!(
        !client.agent_supports_sse_mcp(),
        "SSE MCP should be false before initialize"
    );

    // Perform initialize (but not full connect — just the init step)
    use agent_client_protocol::schema::v1::InitializeRequest;
    let init_request = InitializeRequest::new(1u16.into());
    let init_response = client
        .initialize(init_request)
        .await
        .expect("initialize should succeed");

    // Verify capabilities were stored
    assert!(
        client.agent_supports_http_mcp(),
        "HTTP MCP should be true after initialize with mcp_http=true"
    );
    assert!(
        client.agent_supports_sse_mcp(),
        "SSE MCP should be true after initialize with mcp_sse=true"
    );

    // Verify the response itself has correct capabilities
    assert!(init_response.agent_capabilities.mcp_capabilities.http);
    assert!(init_response.agent_capabilities.mcp_capabilities.sse);
}

/// Test 5: Capabilities default to false when not initialized
#[tokio::test]
async fn capabilities_default_false_when_not_initialized() {
    let config = MockStdioAgentConfig::opencode();
    let (client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    assert!(
        !client.agent_supports_http_mcp(),
        "HTTP MCP should default to false"
    );
    assert!(
        !client.agent_supports_sse_mcp(),
        "SSE MCP should default to false"
    );
}

// ---------------------------------------------------------------------------
// Transport selection, asserted on the `session/new` the agent received
// ---------------------------------------------------------------------------

/// An agent that reports HTTP support and a URL: the agent gets that URL
/// over Streamable HTTP.
#[tokio::test]
async fn agent_reporting_http_support_gets_http_transport() {
    let config = MockStdioAgentConfig {
        mcp_http: true,
        ..MockStdioAgentConfig::opencode()
    };
    assert_eq!(
        offered_transport(config, Some(MCP_URL)).await,
        Offered::Http(MCP_URL.to_string())
    );
}

/// An agent without HTTP support gets stdio even when a URL exists.
#[tokio::test]
async fn agent_without_http_support_falls_back_to_stdio() {
    let config = MockStdioAgentConfig {
        mcp_http: false,
        ..MockStdioAgentConfig::gemini()
    };
    assert_stdio(&offered_transport(config, Some(MCP_URL)).await, "gemini");
}

/// No MCP URL: stdio, even for an agent that supports HTTP.
#[tokio::test]
async fn agent_with_no_mcp_url_always_gets_stdio() {
    let config = MockStdioAgentConfig {
        mcp_http: true,
        ..MockStdioAgentConfig::opencode()
    };
    assert_stdio(&offered_transport(config, None).await, "opencode");
}

/// Each built-in profile gets the transport its advertisement earns when a
/// URL is available: HTTP where the profile reports HTTP, stdio otherwise.
/// No profile ever gets legacy SSE, because the daemon does not serve it.
#[tokio::test]
async fn each_builtin_profile_gets_valid_mcp_transport() {
    let cases: [(&str, MockStdioAgentConfig, bool); 4] = [
        ("opencode", MockStdioAgentConfig::opencode(), true),
        ("claude_acp", MockStdioAgentConfig::claude_acp(), true),
        ("gemini", MockStdioAgentConfig::gemini(), false),
        ("codex", MockStdioAgentConfig::codex(), true),
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
    let config = MockStdioAgentConfig {
        mcp_http: false,
        mcp_sse: true,
        ..MockStdioAgentConfig::opencode()
    };
    assert_stdio(&offered_transport(config, Some(MCP_URL)).await, "sse-only");
}

/// An agent with both gets Streamable HTTP, not legacy SSE.
#[tokio::test]
async fn agent_with_both_http_and_sse_gets_http_not_sse() {
    let config = MockStdioAgentConfig {
        mcp_http: true,
        mcp_sse: true,
        ..MockStdioAgentConfig::opencode()
    };
    assert_eq!(
        offered_transport(config, Some(MCP_URL)).await,
        Offered::Http(MCP_URL.to_string())
    );
}

/// Without a URL every profile completes the handshake on stdio.
#[tokio::test]
async fn stdio_fallback_always_creates_valid_session() {
    let profiles: [(&str, MockStdioAgentConfig); 4] = [
        ("opencode", MockStdioAgentConfig::opencode()),
        ("claude_acp", MockStdioAgentConfig::claude_acp()),
        ("gemini", MockStdioAgentConfig::gemini()),
        ("codex", MockStdioAgentConfig::codex()),
    ];

    for (name, config) in profiles {
        assert_stdio(&offered_transport(config, None).await, name);
    }
}
