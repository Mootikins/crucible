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
        timeout_ms: Some(5000),
        ..Default::default()
    };
    let mut client = CrucibleAcpClient::new(config);

    // Attempt to spawn the agent process
    let result = client.spawn_agent().await;

    // Should successfully spawn process
    assert!(result.is_ok(), "Should spawn agent process");

    // The child must be RETAINED on the client (not returned + dropped) so the
    // daemon can kill a hung agent; disconnect/drop then terminates it.
    assert!(
        client.agent_process.is_some(),
        "Agent child should be retained on the client"
    );
}

/// `disconnect` after `connect` drops the pipes and the child, so the
/// client has no transport left and a later write fails on the spot.
#[tokio::test]
async fn disconnect_after_connect_releases_the_transport() {
    let (cmd, args) = get_cat_command();
    let mut client = CrucibleAcpClient::new(ClientConfig {
        agent_path: cmd,
        agent_args: args,
        timeout_ms: Some(1000),
        ..Default::default()
    });

    let session = client.connect().await.expect("cat spawns");
    assert!(client.is_connected());
    assert!(client.has_transport());

    client
        .disconnect(&session)
        .await
        .expect("disconnect succeeds");

    assert!(!client.is_connected());
    assert!(!client.has_transport());
    assert!(client.agent_process.is_none(), "the child is released");
    match client.write_request(&serde_json::json!({"ping": 1})).await {
        Err(ClientError::Connection(msg)) => assert!(
            msg.contains("No writer available"),
            "unexpected connection error: {msg}"
        ),
        other => panic!("expected a connection error, got {other:?}"),
    }
}

#[tokio::test]
async fn test_bad_agent_path_error() {
    let config = ClientConfig {
        agent_path: PathBuf::from("/nonexistent/agent"),
        agent_args: None,
        timeout_ms: Some(1000),
        ..Default::default()
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
async fn test_connection_state_tracking() {
    let (cmd, args) = get_simple_command();
    let config = ClientConfig {
        agent_path: cmd,
        agent_args: args,
        timeout_ms: Some(1000),
        ..Default::default()
    };
    let mut client = CrucibleAcpClient::new(config);

    // Initially not connected
    assert!(!client.is_connected(), "Should not be connected initially");

    // After spawning, should track connection
    client.spawn_agent().await.unwrap();

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
        timeout_ms: Some(5000),
        ..Default::default()
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
        timeout_ms: Some(1000),
        ..Default::default()
    };
    let mut client = CrucibleAcpClient::new(config);

    // Spawn manually for testing
    client.spawn_agent().await.unwrap();
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

/// `connect` spawns the agent without a protocol handshake, so a plain
/// line echo is enough to drive a message through it. The round trip and
/// the disconnect both have definite outcomes.
#[tokio::test]
async fn connect_send_disconnect_over_an_echo_process() {
    let (cmd, args) = get_cat_command();
    let mut client = CrucibleAcpClient::new(ClientConfig {
        agent_path: cmd,
        agent_args: args,
        timeout_ms: Some(2000),
        ..Default::default()
    });

    let session = client.connect().await.expect("cat spawns");
    assert!(client.is_connected());

    let message = serde_json::json!({"action": "test"});
    let reply = client
        .send_message(message.clone())
        .await
        .expect("cat echoes the line");
    assert_eq!(reply, message);

    client
        .disconnect(&session)
        .await
        .expect("disconnect succeeds");
    assert!(!client.is_connected());
}
