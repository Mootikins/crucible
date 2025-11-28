//! Integration tests for OpenCode agent
//!
//! These tests verify that the CrucibleAcpClient can successfully complete
//! the full handshake with an OpenCode-compatible mock agent and exchange messages.

use crate::support::{MockStdioAgent, MockStdioAgentConfig};
use crucible_acp::client::ClientConfig;
use crucible_acp::CrucibleAcpClient;
use std::path::PathBuf;

/// Test that the client can complete the full handshake with an OpenCode mock agent
///
/// This test will initially FAIL (RED) because the client doesn't properly
/// handle the complete handshake sequence.
#[tokio::test]
async fn test_opencode_complete_handshake() {
    // Create a mock OpenCode agent configuration
    let mock_config = MockStdioAgentConfig::opencode();

    // Build a mock agent binary that we can spawn
    // For now, we'll skip this and just test the concept
    // In the real implementation, we'll compile the mock agent binary
    // and use it as the agent_path

    // Create a client configuration pointing to our mock agent
    // Get the workspace root and build the path to the mock agent
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mock_agent_path = workspace_root.join("target/debug/mock-acp-agent");

    println!("Using mock agent at: {}", mock_agent_path.display());
    println!("Mock agent exists: {}", mock_agent_path.exists());
    println!("Mock agent is absolute: {}", mock_agent_path.is_absolute());

    let client_config = ClientConfig {
        agent_path: mock_agent_path,
        agent_args: None,  // OpenCode is default behavior, no args needed
        working_dir: None, // Don't set working dir for test
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(client_config);

    // Attempt to connect with full handshake
    let result = client.connect_with_handshake().await;

    // This should succeed but currently might fail or hang
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
///
/// RED test - will fail until we properly parse InitializeResponse
#[tokio::test]
async fn test_opencode_initialization() {
    // Skip for now - needs mock agent binary
    // Will implement once we can spawn the mock agent
}

/// Test that session creation returns a session ID
///
/// RED test - will fail until we properly handle NewSessionResponse
#[tokio::test]
async fn test_opencode_session_creation() {
    // Skip for now - needs mock agent binary
    // Will implement once we can spawn the mock agent
}

/// Test that we can send a chat message and receive a response
///
/// RED test - will fail until we implement prompt handling
#[tokio::test]
async fn test_opencode_chat_message_exchange() {
    // Skip for now - needs mock agent binary
    // Will implement once we can spawn the mock agent
}

/// Test error handling when agent returns errors
///
/// RED test - will fail until we properly handle error responses
#[tokio::test]
async fn test_opencode_error_handling() {
    // Skip for now - needs mock agent binary
    // Will implement once we can spawn the mock agent
}

/// Test timeout handling when agent doesn't respond
///
/// RED test - will fail until we properly handle timeouts
#[tokio::test]
async fn test_opencode_timeout_handling() {
    // Skip for now - needs mock agent binary
    // Will implement once we can spawn the mock agent
}

/// Test that agent process cleanup works properly
///
/// RED test - will fail until we properly clean up resources
#[tokio::test]
async fn test_opencode_cleanup() {
    // Skip for now - needs mock agent binary
    // Will implement once we can spawn the mock agent
}

// Helper function to build the mock agent binary
// This will be implemented once we set up the build infrastructure
#[allow(dead_code)]
fn build_mock_agent_binary() -> PathBuf {
    // TODO: Compile the mock agent binary
    // For now, return a placeholder path
    PathBuf::from("target/debug/mock-acp-agent")
}

// Helper function to spawn a mock OpenCode agent
#[allow(dead_code)]
async fn spawn_mock_opencode_agent() -> std::io::Result<std::process::Child> {
    use std::process::{Command, Stdio};

    let binary_path = build_mock_agent_binary();

    Command::new(binary_path)
        .arg("--behavior")
        .arg("opencode")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}
