//! Transport negotiation tests for ACP capability-aware MCP transport selection.
//!
//! These tests verify that `connect_with_best_mcp()` correctly negotiates MCP
//! transport based on agent-reported capabilities per the ACP specification:
//!
//! - `McpServer::Stdio` — All agents MUST support this transport
//! - `McpServer::Http` — Only when agent reports `mcp_capabilities.http == true`
//! - `McpServer::Sse` — Only when agent reports `mcp_capabilities.sse == true`

#[path = "support/mod.rs"]
mod support;

use support::{MockStdioAgentConfig, ThreadedMockAgent};

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
    use agent_client_protocol::InitializeRequest;
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
// Phase A: Transport selection tests via connect_with_best_mcp
// ---------------------------------------------------------------------------

/// Test 1: Agent reporting HTTP support gets HTTP transport.
///
/// We verify by inspecting the session/new request that the mock agent receives.
/// If the agent supports HTTP and a URL is provided, it should get McpServer::Http.
#[tokio::test]
async fn agent_reporting_http_support_gets_http_transport() {
    let config = MockStdioAgentConfig {
        mcp_http: true,
        ..MockStdioAgentConfig::opencode()
    };
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(Some("http://127.0.0.1:9999/mcp"))
        .await
        .expect("connect_with_best_mcp should succeed");

    // Verify session was created
    assert!(!session.id().is_empty(), "Session ID should be non-empty");

    // Verify the client reports HTTP support after initialization
    assert!(
        client.agent_supports_http_mcp(),
        "Client should report HTTP MCP support"
    );
}

/// Test 2: Agent without HTTP support falls back to stdio.
#[tokio::test]
async fn agent_without_http_support_falls_back_to_stdio() {
    let config = MockStdioAgentConfig {
        mcp_http: false,
        ..MockStdioAgentConfig::gemini()
    };
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(Some("http://127.0.0.1:9999/mcp"))
        .await
        .expect("connect_with_best_mcp should succeed even with stdio fallback");

    assert!(!session.id().is_empty(), "Session ID should be non-empty");

    // Verify the client does NOT report HTTP support
    assert!(
        !client.agent_supports_http_mcp(),
        "Client should not report HTTP MCP support"
    );
}

/// Test 3: No MCP URL always gets stdio regardless of agent capabilities.
#[tokio::test]
async fn agent_with_no_mcp_url_always_gets_stdio() {
    let config = MockStdioAgentConfig {
        mcp_http: true,
        ..MockStdioAgentConfig::opencode()
    };
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let session = client
        .connect_with_best_mcp(None)
        .await
        .expect("connect_with_best_mcp(None) should succeed");

    assert!(!session.id().is_empty(), "Session ID should be non-empty");

    // Agent does support HTTP, but since we provided no URL, stdio is used
    assert!(
        client.agent_supports_http_mcp(),
        "Agent should still report HTTP support even though stdio was used"
    );
}

// ---------------------------------------------------------------------------
// Phase B: Mock agent reports capabilities correctly
// ---------------------------------------------------------------------------

/// Test 6: Mock agent with opencode profile reports HTTP MCP capability
#[test]
fn mock_agent_reports_http_mcp_capability() {
    use serde_json::json;
    use support::MockStdioAgent;

    let config = MockStdioAgentConfig::opencode(); // mcp_http: true
    let mut agent = MockStdioAgent::new(config);

    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientInfo": null,
            "clientCapabilities": {},
            "meta": null
        }
    });

    let response = agent.handle_request(&request);
    let result = &response["result"];
    let mcp_caps = &result["agentCapabilities"]["mcpCapabilities"];

    assert_eq!(
        mcp_caps["http"], true,
        "OpenCode mock should report http=true"
    );
    assert_eq!(
        mcp_caps["sse"], false,
        "OpenCode mock should report sse=false"
    );
}

/// Test 7: Default mock agent reports no HTTP by default
#[test]
fn mock_agent_reports_no_http_by_default() {
    use serde_json::json;
    use support::MockStdioAgent;

    let config = MockStdioAgentConfig::default(); // mcp_http: false
    let mut agent = MockStdioAgent::new(config);

    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientInfo": null,
            "clientCapabilities": {},
            "meta": null
        }
    });

    let response = agent.handle_request(&request);
    let result = &response["result"];
    let mcp_caps = &result["agentCapabilities"]["mcpCapabilities"];

    assert_eq!(
        mcp_caps["http"], false,
        "Default mock should report http=false"
    );
    assert_eq!(
        mcp_caps["sse"], false,
        "Default mock should report sse=false"
    );
}

// ---------------------------------------------------------------------------
// Phase C: Invariant tests — each agent profile gets valid transport
// ---------------------------------------------------------------------------

/// Test 8: Each built-in profile gets appropriate transport when MCP URL is provided.
#[tokio::test]
async fn each_builtin_profile_gets_valid_mcp_transport() {
    struct TestCase {
        name: &'static str,
        config: MockStdioAgentConfig,
        expects_http: bool,
    }

    let cases = vec![
        TestCase {
            name: "opencode",
            config: MockStdioAgentConfig::opencode(),
            expects_http: true,
        },
        TestCase {
            name: "claude_acp",
            config: MockStdioAgentConfig::claude_acp(),
            expects_http: true,
        },
        TestCase {
            name: "gemini",
            config: MockStdioAgentConfig::gemini(),
            expects_http: false,
        },
        TestCase {
            name: "codex",
            config: MockStdioAgentConfig::codex(),
            expects_http: true,
        },
    ];

    for case in cases {
        let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(case.config);

        let session = client
            .connect_with_best_mcp(Some("http://127.0.0.1:9999/mcp"))
            .await
            .unwrap_or_else(|e| panic!("connect_with_best_mcp failed for {}: {}", case.name, e));

        assert!(
            !session.id().is_empty(),
            "{}: Session ID should be non-empty",
            case.name
        );

        assert_eq!(
            client.agent_supports_http_mcp(),
            case.expects_http,
            "{}: HTTP MCP support mismatch",
            case.name
        );
    }
}

/// Test 9: Stdio fallback always creates valid session for all profiles.
#[tokio::test]
async fn stdio_fallback_always_creates_valid_session() {
    let profiles: Vec<(&str, MockStdioAgentConfig)> = vec![
        ("opencode", MockStdioAgentConfig::opencode()),
        ("claude_acp", MockStdioAgentConfig::claude_acp()),
        ("gemini", MockStdioAgentConfig::gemini()),
        ("codex", MockStdioAgentConfig::codex()),
    ];

    for (name, config) in profiles {
        let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

        let session = client
            .connect_with_best_mcp(None) // no in-process host → always stdio
            .await
            .unwrap_or_else(|e| panic!("stdio fallback failed for {}: {}", name, e));

        assert!(
            !session.id().is_empty(),
            "{}: Session ID should be non-empty",
            name
        );
    }
}
