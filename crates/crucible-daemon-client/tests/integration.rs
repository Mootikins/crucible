//! Integration tests for DaemonClient with real daemon
//!
//! These tests verify that the client library correctly communicates
//! with a real daemon process.

use anyhow::Result;
use crucible_daemon::Server;
use crucible_daemon_client::DaemonClient;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// Test fixture that starts a real daemon server for integration testing
struct TestServer {
    _temp_dir: TempDir,
    socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

impl TestServer {
    async fn start() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        let server = Server::bind(&socket_path).await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Wait for server to be ready
        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(Self {
            _temp_dir: temp_dir,
            socket_path,
            _server_handle: server_handle,
            shutdown_handle,
        })
    }

    async fn shutdown(self) {
        let _ = self.shutdown_handle.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

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

/// Test DaemonClient.kiln_list() returns empty initially
#[tokio::test]
async fn test_client_kiln_list_with_real_daemon() {
    let server = TestServer::start().await.expect("Failed to start server");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let list = client.kiln_list().await.expect("kiln_list failed");
    assert!(list.is_empty(), "Expected empty kiln list");

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
