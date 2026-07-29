//! Parameterized agent handshake integration tests
//!
//! Verifies that `CrucibleAcpClient` completes the full handshake, session
//! creation, and error-handling flow for every mock agent kind (Claude-ACP and
//! OpenCode). Each test runs once per agent via `test-case`, preserving the
//! coverage of the former `claude_acp_integration` and `opencode_integration`
//! modules while sharing one assertion body per behavior.

use crate::support::{MockStdioAgentConfig, ThreadedMockAgent};
use test_case::test_case;

/// Build a fresh config for the given agent kind. Used as a `test-case`
/// argument so each test enumerates over both supported agent kinds.
#[test_case(MockStdioAgentConfig::claude_acp as fn() -> MockStdioAgentConfig; "claude_acp")]
#[test_case(MockStdioAgentConfig::opencode as fn() -> MockStdioAgentConfig; "opencode")]
#[tokio::test]
async fn handshake_completes(make_config: fn() -> MockStdioAgentConfig) {
    let config = make_config();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_best_mcp(None).await;

    // Mock agents advertise auth/methods but don't enforce them, so the
    // handshake should always succeed for both agent kinds.
    if let Err(ref e) = result {
        eprintln!("Handshake failed with error: {:?}", e);
    }
    assert!(
        result.is_ok(),
        "Should complete handshake successfully: {:?}",
        result.err()
    );

    let session = result.unwrap();
    assert!(!session.id().is_empty(), "Should have valid session ID");
    assert!(
        client.is_connected(),
        "Client should be connected after handshake"
    );
}

/// Initialization (the `initialize` request inside `connect_with_best_mcp`)
/// succeeds for every agent kind.
#[test_case(MockStdioAgentConfig::claude_acp as fn() -> MockStdioAgentConfig; "claude_acp")]
#[test_case(MockStdioAgentConfig::opencode as fn() -> MockStdioAgentConfig; "opencode")]
#[tokio::test]
async fn initialization_succeeds(make_config: fn() -> MockStdioAgentConfig) {
    let config = make_config();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_best_mcp(None).await;
    assert!(
        result.is_ok(),
        "Initialization should succeed: {:?}",
        result.err()
    );
}

/// After a successful handshake, the session id carries the mock prefix.
#[test_case(MockStdioAgentConfig::claude_acp as fn() -> MockStdioAgentConfig; "claude_acp")]
#[test_case(MockStdioAgentConfig::opencode as fn() -> MockStdioAgentConfig; "opencode")]
#[tokio::test]
async fn session_id_has_mock_prefix(make_config: fn() -> MockStdioAgentConfig) {
    let config = make_config();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_best_mcp(None).await;
    assert!(
        result.is_ok(),
        "Should complete handshake: {:?}",
        result.err()
    );

    let session = result.unwrap();
    assert!(
        session.id().starts_with("mock-session-"),
        "Session ID should have mock prefix, got: {}",
        session.id()
    );
}

/// When the mock agent is configured to inject errors, the handshake fails
/// for every agent kind.
#[test_case(MockStdioAgentConfig::claude_acp as fn() -> MockStdioAgentConfig; "claude_acp")]
#[test_case(MockStdioAgentConfig::opencode as fn() -> MockStdioAgentConfig; "opencode")]
#[tokio::test]
async fn error_injection_fails_handshake(make_config: fn() -> MockStdioAgentConfig) {
    let mut config = make_config();
    config.inject_errors = true;
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_best_mcp(None).await;
    assert!(result.is_err(), "Should fail when errors are injected");
}
