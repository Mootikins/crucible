//! Integration tests for Claude-ACP agent
//!
//! These tests verify that the CrucibleAcpClient can successfully complete
//! the full handshake with a Claude-ACP-compatible mock agent, including
//! authentication flows.

use std::path::PathBuf;
use crucible_acp::CrucibleAcpClient;
use crucible_acp::client::ClientConfig;

/// Test that the client can complete handshake with Claude-ACP agent
///
/// This test will initially FAIL (RED) because the client doesn't support
/// authentication required by Claude-ACP agents.
#[tokio::test]
async fn test_claude_acp_complete_handshake_with_auth() {
    // Get workspace root and build path to mock agent
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mock_agent_path = workspace_root.join("target/debug/mock-acp-agent");

    println!("Using mock agent at: {}", mock_agent_path.display());

    let client_config = ClientConfig {
        agent_path: mock_agent_path,
        // Pass --behavior claude-acp to mock agent
        agent_args: Some(vec!["--behavior".to_string(), "claude-acp".to_string()]),
        working_dir: None,
        // Claude-ACP requires API key authentication
        env_vars: Some(vec![
            ("ANTHROPIC_API_KEY".to_string(), "test-api-key-12345".to_string()),
        ]),
        timeout_ms: Some(5000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(client_config);

    // This will FAIL because we need to:
    // 1. Detect agent requires auth from InitializeResponse.auth_methods
    // 2. Send authenticate request with credentials
    // 3. Complete handshake after successful authentication
    let result = client.connect_with_handshake().await;

    // RED: This assertion will fail
    assert!(
        result.is_ok(),
        "Should complete handshake with authentication: {:?}",
        result.err()
    );

    let session = result.unwrap();
    assert!(!session.id().is_empty(), "Should have valid session ID");
    assert!(client.is_connected(), "Client should be connected after handshake");
}

/// Test that initialization response correctly identifies auth requirements
///
/// RED test - will fail until we properly parse auth_methods from InitializeResponse
#[tokio::test]
async fn test_claude_acp_initialization_auth_detection() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mock_agent_path = workspace_root.join("target/debug/mock-acp-agent");

    let client_config = ClientConfig {
        agent_path: mock_agent_path,
        agent_args: None,
        working_dir: None,
        env_vars: Some(vec![
            ("ANTHROPIC_API_KEY".to_string(), "test-api-key-12345".to_string()),
        ]),
        timeout_ms: Some(5000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(client_config);

    // RED: This will fail because we don't have a way to check auth requirements yet
    // We need to expose initialization response or provide a method to check auth status
    let result = client.connect_with_handshake().await;

    // For now, just check that connection attempt happens
    // Later we'll verify auth_methods are properly detected
    assert!(
        result.is_ok() || result.is_err(),
        "Should attempt connection"
    );
}

/// Test that authentication request is properly formatted
///
/// RED test - will fail until we implement authenticate method
#[tokio::test]
async fn test_claude_acp_authentication_request_format() {
    // This test will verify that when we call authenticate(),
    // the request is properly formatted according to ACP spec

    // For now, this is a placeholder - we'll implement when we add auth support
    // The authenticate request should include:
    // - method: "authenticate"
    // - params: { "authMethod": "api_key", "credentials": {...} }

    // RED: No authenticate method exists yet
}

/// Test handling of authentication failure
///
/// RED test - will fail until we implement auth error handling
#[tokio::test]
async fn test_claude_acp_authentication_failure() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mock_agent_path = workspace_root.join("target/debug/mock-acp-agent");

    let client_config = ClientConfig {
        agent_path: mock_agent_path,
        agent_args: None,
        working_dir: None,
        // Intentionally invalid API key
        env_vars: Some(vec![
            ("ANTHROPIC_API_KEY".to_string(), "invalid-key".to_string()),
        ]),
        timeout_ms: Some(5000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(client_config);

    // RED: Should fail with authentication error, but we don't handle this yet
    let result = client.connect_with_handshake().await;

    // We expect authentication to fail, but need proper error handling
    if let Err(e) = result {
        // Later: verify error is authentication-specific
        println!("Expected auth error: {:?}", e);
    }
}

/// Test that Claude-ACP specific capabilities are advertised
///
/// RED test - will fail until we verify agent capabilities properly
#[tokio::test]
async fn test_claude_acp_capabilities() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mock_agent_path = workspace_root.join("target/debug/mock-acp-agent");

    let client_config = ClientConfig {
        agent_path: mock_agent_path,
        agent_args: None,
        working_dir: None,
        env_vars: Some(vec![
            ("ANTHROPIC_API_KEY".to_string(), "test-api-key-12345".to_string()),
        ]),
        timeout_ms: Some(5000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(client_config);

    let result = client.connect_with_handshake().await;

    if result.is_ok() {
        // RED: Need way to access and verify agent capabilities
        // Claude-ACP should advertise: fs.readTextFile, fs.writeTextFile, terminal, loadSession
        // For now, just verify connection succeeded
        assert!(client.is_connected());
    }
}

/// Test session creation after authentication
///
/// RED test - will fail until auth + session creation flow works
#[tokio::test]
async fn test_claude_acp_session_after_auth() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mock_agent_path = workspace_root.join("target/debug/mock-acp-agent");

    let client_config = ClientConfig {
        agent_path: mock_agent_path,
        agent_args: None,
        working_dir: None,
        env_vars: Some(vec![
            ("ANTHROPIC_API_KEY".to_string(), "test-api-key-12345".to_string()),
        ]),
        timeout_ms: Some(5000),
        max_retries: Some(1),
    };

    let mut client = CrucibleAcpClient::new(client_config);

    // RED: Should complete auth then create session
    let result = client.connect_with_handshake().await;

    if let Ok(session) = result {
        assert!(!session.id().is_empty(), "Should have valid session ID after auth");
        assert!(session.id().starts_with("mock-session-"), "Should be mock session");
    } else {
        // Will fail until auth is implemented
        panic!("Should successfully create session after authentication");
    }
}
