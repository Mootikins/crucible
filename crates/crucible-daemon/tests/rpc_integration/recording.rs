//! Session recording and replay RPC tests.

use crucible_core::protocol::requests::SessionCreateRequest;
use crucible_daemon::DaemonClient;

use super::server::TestServer;

/// Test session_create with granular recording_mode
#[tokio::test]
async fn test_session_create_with_granular_recording_mode() {
    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            kilns: SessionCreateRequest::kiln_set(vec![crucible_daemon::test_support::kiln_name(
                "kiln",
            )]),
            recording_mode: Some("granular".to_string()),
            ..Default::default()
        })
        .await
        .expect("session_create with recording_mode failed");

    let session_id = result.id.as_str();
    assert!(!session_id.is_empty(), "session_id should not be empty");

    server.shutdown().await;
}

/// Test session_create with None recording_mode (normal operation)
#[tokio::test]
async fn test_session_create_with_no_recording_mode() {
    let server = TestServer::start().await.expect("Failed to start server");
    let _kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(SessionCreateRequest {
            session_type: "chat".to_string(),
            kilns: SessionCreateRequest::kiln_set(vec![crucible_daemon::test_support::kiln_name(
                "kiln",
            )]),
            ..Default::default()
        })
        .await
        .expect("session_create without recording_mode failed");

    let session_id = result.id.as_str();
    assert!(!session_id.is_empty(), "session_id should not be empty");

    server.shutdown().await;
}

/// Test session_replay with invalid path returns error (not panic)
#[tokio::test]
async fn test_session_replay_rpc_invalid_path() {
    let server = TestServer::start().await.expect("Failed to start server");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Call session.replay with a nonexistent path
    let nonexistent_path = std::path::PathBuf::from("/nonexistent/recording.jsonl");
    let result = client.session_replay(&nonexistent_path, 1.0).await;

    // Verify we get an error, not a panic
    assert!(
        result.is_err(),
        "session_replay with invalid path should return Err, not panic"
    );

    server.shutdown().await;
}
