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

            // Each client queries the kiln
            // Note: This will return an empty result since we haven't indexed anything,
            // but it verifies the query RPC works across multiple sessions
            let result = client
                .query(&kiln_path, "SELECT * FROM notes LIMIT 1")
                .await;

            // Query should succeed (even if empty results)
            assert!(
                result.is_ok(),
                "Client {} query failed: {:?}",
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

/// Test StorageClient via DaemonStorageClient with multiple sessions
///
/// This tests the full StorageClient trait implementation through the daemon,
/// simulating how the CLI's get_storage(daemon mode) works.
#[tokio::test]
async fn test_daemon_storage_client_multi_session() {
    use crucible_core::traits::StorageClient;
    use crucible_daemon_client::DaemonStorageClient;
    use std::sync::Arc;

    let server = TestServer::start().await.expect("Failed to start server");
    let socket_path = server.socket_path.clone();

    // Create a temp kiln directory
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    // Create two DaemonStorageClient instances pointing to the same kiln
    let client1 = Arc::new(
        DaemonClient::connect_to(&socket_path)
            .await
            .expect("client1"),
    );
    let client2 = Arc::new(
        DaemonClient::connect_to(&socket_path)
            .await
            .expect("client2"),
    );

    let storage1 = DaemonStorageClient::new(client1, kiln_dir.path().to_path_buf());
    let storage2 = DaemonStorageClient::new(client2, kiln_dir.path().to_path_buf());

    // Both should be able to query
    let result1 = storage1.query_raw("SELECT count() FROM notes").await;
    let result2 = storage2.query_raw("SELECT count() FROM notes").await;

    assert!(
        result1.is_ok(),
        "Storage1 query failed: {:?}",
        result1.err()
    );
    assert!(
        result2.is_ok(),
        "Storage2 query failed: {:?}",
        result2.err()
    );

    server.shutdown().await;
}

/// Test search_vectors RPC method - backend-agnostic vector search
///
/// This tests that the daemon's search_vectors RPC works with the SQLite backend
/// (the daemon default), ensuring CLI clients can perform semantic search
/// regardless of which SQL dialect they were designed for.
#[tokio::test]
async fn test_search_vectors_rpc() {
    let server = TestServer::start().await.expect("Failed to start server");
    let socket_path = server.socket_path.clone();

    // Create a temp kiln directory
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&socket_path)
        .await
        .expect("Failed to connect");

    // Open the kiln first
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // Search with a test vector - should return empty since no data yet
    let test_vector: Vec<f32> = vec![1.0, 0.0, 0.0];
    let results = client
        .search_vectors(kiln_dir.path(), &test_vector, 10)
        .await
        .expect("search_vectors RPC failed");

    // Should succeed but return empty (no embeddings in fresh db)
    assert!(
        results.is_empty(),
        "Fresh kiln should have no search results"
    );

    server.shutdown().await;
}

/// Test search_vectors via KnowledgeRepository trait
///
/// This is the full integration test that mimics how the CLI's semantic_search
/// actually works: KilnContext -> StorageHandle::Daemon -> DaemonStorageClient -> RPC
#[tokio::test]
async fn test_search_vectors_via_knowledge_repository() {
    use crucible_core::traits::KnowledgeRepository;
    use crucible_daemon_client::DaemonStorageClient;
    use std::sync::Arc;

    let server = TestServer::start().await.expect("Failed to start server");
    let socket_path = server.socket_path.clone();

    // Create a temp kiln directory
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let daemon_client = Arc::new(
        DaemonClient::connect_to(&socket_path)
            .await
            .expect("Failed to connect"),
    );

    // Open kiln via daemon client first
    daemon_client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // Create DaemonStorageClient which implements KnowledgeRepository
    let storage = DaemonStorageClient::new(daemon_client, kiln_dir.path().to_path_buf());

    // Use the KnowledgeRepository trait method - this is what semantic_search calls
    let test_vector: Vec<f32> = vec![0.5, 0.5, 0.0];
    let results = storage
        .search_vectors(test_vector)
        .await
        .expect("KnowledgeRepository::search_vectors failed");

    // Should succeed with empty results (no embeddings)
    assert!(
        results.is_empty(),
        "Fresh kiln should have no search results via KnowledgeRepository"
    );

    server.shutdown().await;
}
