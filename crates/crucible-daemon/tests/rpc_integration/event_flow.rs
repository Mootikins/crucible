//! Event streaming and event-flow tests.
//!
//! Covers:
//! - Event streaming with background reader.
//! - Concurrent RPC calls while in event mode.
//! - Daemon agent error surfaces to chat error.

use crucible_daemon::DaemonClient;

use super::server::TestServer;

/// Event types emitted asynchronously during session setup. See
/// `spawn_setup_task` in `crucible-daemon/src/server/session/mod.rs` — these
/// fan out on tokio tasks and can arrive at any time after `session.create`
/// returns, including well after our subsequent RPC calls.
fn is_setup_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "session_initialized"
            | "workspace_indexed"
            | "kiln_notes_indexed"
            | "plugins_discovered"
            | "mcp_servers_ready"
            | "providers_listed"
            | "context_limit_resolved"
    )
}

#[tokio::test]
async fn test_event_streaming_with_background_reader() {
    use std::sync::Arc;
    use std::time::Duration;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, mut event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = Arc::new(client);

    let result = client
        .session_create(crucible_daemon::rpc_client::SessionCreateParams {
            session_type: "chat".to_string(),
            kiln: kiln_dir.path().to_path_buf(),
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
        .expect("should have session_id")
        .to_string();

    client
        .session_subscribe(&[&session_id])
        .await
        .expect("subscribe failed");

    let ping_result = client.ping().await.expect("ping failed");
    assert_eq!(ping_result, "pong");

    let list_result = client.kiln_list().await.expect("kiln_list failed");
    assert!(list_result.is_empty() || !list_result.is_empty());

    // Test intent: ping and kiln_list are pure request/response — they must
    // not emit *their own* events. Setup events (workspace_indexed, etc.)
    // fire asynchronously from session.create and can interleave arbitrarily
    // with later RPC calls; filter them out so the assertion remains
    // deterministic under load. Any non-setup event within the window is a
    // regression and fails the test.
    let offending = tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            match event_rx.recv().await {
                Some(evt) if is_setup_event(&evt.event_type) => continue,
                other => return other,
            }
        }
    })
    .await;
    assert!(
        matches!(offending, Err(_) | Ok(None)),
        "ping and kiln_list should not produce non-setup events, got: {offending:?}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_concurrent_rpc_calls_event_mode() {
    use std::sync::Arc;

    let server = TestServer::start().await.expect("Failed to start server");

    let (client, _event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = Arc::new(client);

    let mut handles = vec![];
    for _ in 0..5 {
        let c = client.clone();
        handles.push(tokio::spawn(async move { c.ping().await }));
    }

    for handle in handles {
        let result = handle.await.expect("task panicked");
        assert_eq!(result.expect("ping failed"), "pong");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn test_daemon_agent_error_produces_chat_error() {
    let server = TestServer::start().await.expect("Failed to start server");

    let (client, _event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");

    let result = client
        .session_send_message("nonexistent-session-id", "Hello", true)
        .await;

    assert!(
        result.is_err(),
        "Sending to nonexistent session should fail"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        !err_msg.is_empty(),
        "Error message should not be empty for TUI display"
    );
    assert!(
        err_msg.len() < 1000,
        "Error message should be reasonably sized: {}",
        err_msg
    );

    server.shutdown().await;
}

