//! Integration tests for OpenCode agent
//!
//! These tests verify that the CrucibleAcpClient can successfully complete
//! the full handshake with an OpenCode-compatible mock agent and exchange messages.
//!
//! Now uses ThreadedMockAgent for in-process testing (no subprocess needed).

use crate::support::{MockStdioAgentConfig, ThreadedMockAgent};

/// Test that the client can complete the full handshake with an OpenCode mock agent
///
/// Uses ThreadedMockAgent for in-process testing without needing a binary.
#[tokio::test]
async fn test_opencode_complete_handshake() {
    // Spawn threaded mock agent with OpenCode behavior
    let config = MockStdioAgentConfig::opencode();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    // Attempt to connect with full handshake
    let result = client.connect_with_handshake().await;

    // This should succeed
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

    // Verify client is connected
    assert!(
        client.is_connected(),
        "Client should be connected after handshake"
    );
}

/// Test that initialization request gets proper response
#[tokio::test]
async fn test_opencode_initialization() {
    let config = MockStdioAgentConfig::opencode();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    // connect_with_handshake performs initialization internally
    let result = client.connect_with_handshake().await;
    assert!(result.is_ok(), "Initialization should succeed");
}

/// Test that session creation returns a session ID
#[tokio::test]
async fn test_opencode_session_creation() {
    let config = MockStdioAgentConfig::opencode();
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_handshake().await;
    assert!(result.is_ok(), "Should create session");

    let session = result.unwrap();
    assert!(
        session.id().starts_with("mock-session-"),
        "Session ID should have mock prefix"
    );
}

/// Test error handling when agent returns errors
#[tokio::test]
async fn test_opencode_error_handling() {
    let mut config = MockStdioAgentConfig::opencode();
    config.inject_errors = true;
    let (mut client, _handle) = ThreadedMockAgent::spawn_with_client(config);

    let result = client.connect_with_handshake().await;
    assert!(result.is_err(), "Should fail when errors are injected");
}
