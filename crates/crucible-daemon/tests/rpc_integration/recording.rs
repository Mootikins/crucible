//! Session recording and replay RPC tests.

use crucible_daemon::DaemonClient;
use std::time::Duration;

use super::server::TestServer;

/// Test session_create with granular recording_mode
#[tokio::test]
async fn test_session_create_with_granular_recording_mode() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: Some("granular".to_string()),
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create with recording_mode failed");

    // Verify response contains session_id
    assert!(
        result["session_id"].is_string(),
        "Response should contain session_id as string"
    );

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string");
    assert!(!session_id.is_empty(), "session_id should not be empty");

    server.shutdown().await;
}

/// Test session_create with None recording_mode (normal operation)
#[tokio::test]
async fn test_session_create_with_no_recording_mode() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create without recording_mode failed");

    // Verify response contains session_id
    assert!(
        result["session_id"].is_string(),
        "Response should contain session_id as string"
    );

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string");
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

/// Regression: DaemonAgentHandle::drop() must call session.end so RecordingWriter writes footer.
/// Without the Drop impl, `:q` never ended the session and the recording footer was missing.
#[tokio::test]
async fn test_recording_footer_regression_drop_ends_session() {
    use crucible_daemon::DaemonAgentHandle;
    use std::sync::Arc;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = Arc::new(client);

    // Given: an active session with an agent handle
    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: Some(kiln_dir.path().to_path_buf()),
            workspace: None,
            connect_kilns: vec![],
            recording_mode: None,
            recording_path: None,
            agent_type: None,
        })
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let session_before = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");
    assert_eq!(
        session_before["state"], "active",
        "Session should be active before handle drop"
    );

    let handle = DaemonAgentHandle::new_and_subscribe(client.clone(), session_id.clone(), event_rx)
        .await
        .expect("Failed to create agent handle");

    // When: the handle is dropped (simulates :q in TUI)
    drop(handle);

    // Drop spawns a fire-and-forget task; wait for the RPC round-trip
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Then: session was ended and cleaned up (session_get returns not-found or state "ended")
    let session_result = client.session_get(&session_id).await;
    let session_was_ended = match &session_result {
        Err(e) => e.to_string().contains("not found") || e.to_string().contains("Not found"),
        Ok(val) => val.get("state").and_then(|s| s.as_str()) == Some("ended"),
    };
    assert!(
        session_was_ended,
        "Session should be ended/removed after DaemonAgentHandle drop, got: {:?}. \
         Recording footer regression — :q must trigger session.end.",
        session_result,
    );

    server.shutdown().await;
}
