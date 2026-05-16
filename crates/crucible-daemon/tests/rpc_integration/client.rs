//! Client basic tests: ping, shutdown, sequential, concurrent, errors.

use crucible_daemon::DaemonClient;
use std::path::PathBuf;
use std::time::Duration;

use super::server::TestServer;

/// Test DaemonClient.ping() with real daemon
#[tokio::test]
async fn test_client_ping_with_real_daemon() {
    let server = TestServer::start().await.expect("Failed to start server");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client.ping().await.expect("Ping failed");
    assert_eq!(result, "pong");

    server.shutdown().await;
}

#[tokio::test]
async fn test_interaction_event_flows_to_receiver() {
    use crucible_core::interaction::InteractionRequest;
    use crucible_core::traits::chat::AgentHandle;
    use crucible_daemon::DaemonAgentHandle;
    use std::time::Duration;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

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
        .expect("session_id should be string")
        .to_string();

    let mut handle =
        DaemonAgentHandle::new_and_subscribe(client.clone(), session_id.clone(), event_rx)
            .await
            .expect("Failed to create agent handle");

    let interaction_rx = handle.take_interaction_receiver();
    assert!(
        interaction_rx.is_some(),
        "DaemonAgentHandle should return Some(interaction_rx)"
    );
    let mut interaction_rx = interaction_rx.unwrap();

    assert!(
        handle.take_interaction_receiver().is_none(),
        "Second call to take_interaction_receiver should return None"
    );

    let interact_result = client
        .call(
            "session.test_interaction",
            serde_json::json!({
                "session_id": session_id,
                "type": "ask"
            }),
        )
        .await
        .expect("test_interaction RPC failed");

    let request_id = interact_result["request_id"]
        .as_str()
        .expect("request_id should be in response");

    let event = tokio::time::timeout(Duration::from_secs(2), interaction_rx.recv())
        .await
        .expect("Timed out waiting for interaction event")
        .expect("Interaction channel closed unexpectedly");

    assert_eq!(event.request_id, request_id, "Request ID should match");
    assert!(
        matches!(event.request, InteractionRequest::Ask(_)),
        "Should be an Ask request, got {:?}",
        event.request
    );

    server.shutdown().await;
}

/// Test DaemonClient.shutdown() actually stops the daemon
#[tokio::test]
async fn test_client_shutdown_with_real_daemon() {
    let server = TestServer::start().await.expect("Failed to start server");
    let socket_path = server.socket_path.clone();

    let client = DaemonClient::connect_to(&socket_path)
        .await
        .expect("Failed to connect");

    // Send shutdown
    client.shutdown().await.expect("Shutdown failed");

    // Wait for shutdown to complete
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Try to connect a new client - should fail because daemon is down
    let result = DaemonClient::connect_to(&socket_path).await;
    assert!(
        result.is_err(),
        "Expected new connection to fail after shutdown"
    );
}

/// Test multiple sequential RPC calls
#[tokio::test]
async fn test_client_multiple_sequential_calls() {
    let server = TestServer::start().await.expect("Failed to start server");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Make multiple calls
    for i in 0..10 {
        let result = client
            .ping()
            .await
            .unwrap_or_else(|_| panic!("Ping {} failed", i));
        assert_eq!(result, "pong", "Ping {} should return pong", i);
    }

    server.shutdown().await;
}

/// Test that kiln.open with nonexistent path returns error
#[tokio::test]
async fn test_client_kiln_open_nonexistent_path() {
    let server = TestServer::start().await.expect("Failed to start server");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let fake_path = PathBuf::from("/nonexistent/path/to/kiln");
    let result = client.kiln_open(&fake_path).await;

    assert!(result.is_err(), "Expected error for nonexistent path");

    server.shutdown().await;
}

/// Test concurrent client instances using the same daemon
#[tokio::test]
async fn test_multiple_clients_concurrent() {
    let server = TestServer::start().await.expect("Failed to start server");
    let socket_path = server.socket_path.clone();

    // Spawn 5 concurrent clients
    let mut handles = vec![];
    for i in 0..5 {
        let socket = socket_path.clone();
        let handle = tokio::spawn(async move {
            let client = DaemonClient::connect_to(&socket)
                .await
                .unwrap_or_else(|_| panic!("Client {} failed to connect", i));

            // Each client makes 3 requests
            for j in 0..3 {
                let result = client
                    .ping()
                    .await
                    .unwrap_or_else(|_| panic!("Client {} request {} failed", i, j));
                assert_eq!(result, "pong");
            }
        });
        handles.push(handle);
    }

    // Wait for all clients
    for (i, handle) in handles.into_iter().enumerate() {
        handle
            .await
            .unwrap_or_else(|_| panic!("Client {} task panicked", i));
    }

    server.shutdown().await;
}

/// Test that client connection fails when no daemon is running
#[tokio::test]
async fn test_client_connect_fails_without_daemon() {
    let temp_dir = tempfile::tempdir().unwrap();
    let socket_path = temp_dir.path().join("nonexistent.sock");

    let result = DaemonClient::connect_to(&socket_path).await;
    assert!(result.is_err(), "Expected connection to fail");
}

/// Test RPC error handling (invalid params)
#[tokio::test]
async fn test_client_handles_rpc_errors() {
    let server = TestServer::start().await.expect("Failed to start server");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Make a raw call that will trigger an error (missing required param)
    let result = client.call("kiln.open", serde_json::json!({})).await;

    assert!(result.is_err(), "Expected error for missing param");
    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("error") || err_str.contains("path"),
        "Error should mention the problem"
    );

    server.shutdown().await;
}

/// Test new client connection fails when daemon stops
#[tokio::test]
async fn test_new_connection_fails_after_shutdown() {
    let server = TestServer::start().await.expect("Failed to start server");
    let socket_path = server.socket_path.clone();

    let client = DaemonClient::connect_to(&socket_path)
        .await
        .expect("Failed to connect");

    // First ping should work
    let result = client.ping().await;
    assert!(result.is_ok(), "First ping should succeed");

    // Shutdown server
    server.shutdown().await;

    // New connection should fail
    let result = DaemonClient::connect_to(&socket_path).await;
    assert!(
        result.is_err(),
        "New connection should fail after server shutdown"
    );
}

/// Test multiple clients querying the same kiln concurrently
///
/// This tests the multi-session capability where multiple CLI instances
/// can share the same daemon and query the same kiln.
#[tokio::test]
async fn test_multiple_clients_query_same_kiln() {
    let server = TestServer::start().await.expect("Failed to start server");
    let socket_path = server.socket_path.clone();

    // Create a temp kiln directory with a valid structure
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    // Open the kiln first via one client
    let setup_client = DaemonClient::connect_to(&socket_path)
        .await
        .expect("Failed to connect for setup");

    setup_client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // Spawn 3 concurrent clients that all query the same kiln
    let mut handles = vec![];
    for i in 0..3 {
        let socket = socket_path.clone();
        let kiln_path = kiln_dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let client = DaemonClient::connect_to(&socket)
                .await
                .unwrap_or_else(|_| panic!("Client {} failed to connect", i));

            // Each client lists notes in the kiln
            // Note: This will return an empty result since we haven't indexed anything,
            // but it verifies the RPC works across multiple sessions
            let result = client.list_notes(&kiln_path, None, None).await;

            // Query should succeed (even if empty results)
            assert!(
                result.is_ok(),
                "Client {} list_notes failed: {:?}",
                i,
                result.err()
            );
        });
        handles.push(handle);
    }

    // Wait for all clients to complete
    for (i, handle) in handles.into_iter().enumerate() {
        handle
            .await
            .unwrap_or_else(|e| panic!("Client {} task panicked: {:?}", i, e));
    }

    // Verify kiln appears in list
    let list = setup_client.kiln_list().await.expect("kiln_list failed");
    assert!(!list.is_empty(), "Kiln should be in list after opening");

    server.shutdown().await;
}
