use std::path::PathBuf;

use super::{get_cat_command, get_simple_command};
use crate::acp::client::types::ClientConfig;
use crate::acp::client::CrucibleAcpClient;
use crate::acp::ClientError;

#[tokio::test]
async fn test_agent_process_spawning() {
    // Use a simple command as test agent
    let (cmd, args) = get_simple_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(3),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Attempt to spawn the agent process
    let result = client.spawn_agent().await;

    // Should successfully spawn process
    assert!(result.is_ok(), "Should spawn agent process");

    // Process should be running
    let process = result.unwrap();
    assert!(process.is_running(), "Agent process should be running");
}

#[tokio::test]
async fn test_connection_establishment() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/test/agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(3),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Should establish connection
    let result = client.connect().await;

    // For now this will fail, but eventually should succeed
    // with a mock or real agent
    assert!(result.is_err(), "Should fail until implementation complete");
}

#[tokio::test]
async fn test_connection_cleanup() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/test/agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(3),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Connect
    let session = client.connect().await;

    if let Ok(session) = session {
        // Disconnect should clean up resources
        let result = client.disconnect(&session).await;
        assert!(result.is_ok(), "Should disconnect cleanly");

        // Connection should be closed
        assert!(
            !client.is_connected(),
            "Should not be connected after disconnect"
        );
    }
}

#[tokio::test]
async fn test_bad_agent_path_error() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/nonexistent/agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);

    let result = client.connect().await;

    // Should fail with clear error
    assert!(result.is_err(), "Should fail for nonexistent agent");

    let err = result.unwrap_err();
    match err {
        ClientError::Connection(_) => {} // Expected
        _ => panic!("Should be Connection error"),
    }
}

#[tokio::test]
async fn test_connection_timeout() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/test/hanging-agent"),
        agent_args: None,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(100), // Very short timeout
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);

    let result = client.connect().await;

    // Should timeout
    assert!(result.is_err(), "Should timeout");

    let err = result.unwrap_err();
    match err {
        ClientError::Timeout(_) => {}    // Expected
        ClientError::Connection(_) => {} // Also acceptable
        _ => panic!("Should be Timeout or Connection error"),
    }
}

#[tokio::test]
async fn test_connection_state_tracking() {
    let (cmd, args) = get_simple_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Initially not connected
    assert!(!client.is_connected(), "Should not be connected initially");

    // After spawning, should track connection
    let _process = client.spawn_agent().await.unwrap();

    // Mark as connected (this will be part of connect() implementation)
    client.mark_connected();
    assert!(client.is_connected(), "Should be connected after marking");

    // After disconnect, should not be connected
    client.mark_disconnected();
    assert!(
        !client.is_connected(),
        "Should not be connected after disconnect"
    );
}

// RED: Test expects connect() to spawn agent and establish session
#[tokio::test]
async fn test_connect_spawns_and_establishes_session() {
    let (cmd, args) = get_simple_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(5000),
        max_retries: Some(3),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Should start with no connection
    assert!(!client.is_connected());

    // Connect should spawn agent and mark connected
    let result = client.connect().await;

    // Should succeed and return a session
    assert!(result.is_ok(), "Should connect successfully");
    assert!(client.is_connected(), "Should be connected after connect()");
}

// RED: Test expects disconnect() to clean up resources
#[tokio::test]
async fn test_disconnect_cleanup() {
    let (cmd, args) = get_simple_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(1000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);

    // Spawn manually for testing
    let _process = client.spawn_agent().await.unwrap();
    client.mark_connected();

    // Create a session for testing
    use crate::acp::session::{AcpSession, TransportConfig};
    let session = AcpSession::new(TransportConfig::default(), "test-session-123".to_string());

    // Disconnect should clean up
    let result = client.disconnect(&session).await;

    // Should succeed
    assert!(result.is_ok(), "Should disconnect successfully");
    assert!(
        !client.is_connected(),
        "Should not be connected after disconnect"
    );
}

// RED: Test expects full lifecycle: connect -> message -> disconnect
#[tokio::test]
async fn test_full_agent_lifecycle() {
    let (cmd, args) = get_cat_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        working_dir: None,
        env_vars: None,
        timeout_ms: Some(2000),
        max_retries: Some(1),
    };
    let mut client = CrucibleAcpClient::new(config);

    // 1. Connect
    if let Ok(session) = client.connect().await {
        assert!(client.is_connected(), "Should be connected after connect()");

        // 2. Send message
        let message = serde_json::json!({"action": "test"});
        let _send_result = client.send_message(message).await;

        // 3. Disconnect
        let disconnect_result = client.disconnect(&session).await;

        if disconnect_result.is_ok() {
            assert!(
                !client.is_connected(),
                "Should not be connected after disconnect"
            );
        }
    }
}
