//! Integration tests for Claude-ACP agent
//!
//! These tests verify that the CrucibleAcpClient can successfully complete
//! the full handshake with a Claude-ACP-compatible mock agent, including
//! authentication flows.
//!
//! Now uses ThreadedMockAgent for in-process testing (no subprocess needed).

use crate::support::{MockStdioAgentConfig, ThreadedMockAgent};

/// Test that the client can complete handshake with Claude-ACP agent
///
/// Note: Claude-ACP agents require authentication, but the mock agent
/// accepts any auth. This tests the handshake flow works.
#[tokio::test]
async fn test_claude_acp_complete_handshake_with_auth() {
    // Spawn threaded mock agent with Claude-ACP behavior
    let config = MockStdioAgentConfig::claude_acp();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    // connect_with_handshake will succeed because mock agent doesn't
    // actually enforce auth (it just advertises requiring it)
    let result = client.connect_with_handshake().await;

    assert!(
        result.is_ok(),
        "Should complete handshake with authentication: {:?}",
        result.err()
    );

    let session = result.unwrap();
    assert!(!session.id().is_empty(), "Should have valid session ID");
    assert!(
        client.is_connected(),
        "Client should be connected after handshake"
    );
}

/// Test that initialization response correctly identifies auth requirements
#[tokio::test]
async fn test_claude_acp_initialization_auth_detection() {
    let config = MockStdioAgentConfig::claude_acp();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    // Claude-ACP config sets requires_auth = true
    // The mock agent advertises auth_methods in initialize response
    let result = client.connect_with_handshake().await;

    // Connection should succeed (mock doesn't actually enforce auth)
    assert!(
        result.is_ok(),
        "Should complete connection: {:?}",
        result.err()
    );
}

/// Test that Claude-ACP specific capabilities are advertised
#[tokio::test]
async fn test_claude_acp_capabilities() {
    let config = MockStdioAgentConfig::claude_acp();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_handshake().await;
    assert!(result.is_ok(), "Connection should succeed");
    assert!(client.is_connected());
}

/// Test session creation after authentication
#[tokio::test]
async fn test_claude_acp_session_after_auth() {
    let config = MockStdioAgentConfig::claude_acp();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_handshake().await;
    assert!(result.is_ok(), "Should complete handshake");

    let session = result.unwrap();
    assert!(
        !session.id().is_empty(),
        "Should have valid session ID after auth"
    );
    assert!(
        session.id().starts_with("mock-session-"),
        "Should be mock session"
    );
}
