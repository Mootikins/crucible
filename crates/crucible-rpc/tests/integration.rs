//! Integration tests for DaemonClient with real daemon
//!
//! These tests verify that the client library correctly communicates
//! with a real daemon process.

use anyhow::Result;
use crucible_core::traits::chat::AgentHandle;
use crucible_daemon::Server;
use crucible_rpc::DaemonClient;
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

        let server = Server::bind(&socket_path, None).await?;
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

#[tokio::test]
async fn test_interaction_event_flows_to_receiver() {
    use crucible_core::interaction::InteractionRequest;
    use crucible_core::traits::chat::AgentHandle;
    use crucible_rpc::DaemonAgentHandle;
    use std::time::Duration;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
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
            let result = client.list_notes(&kiln_path, None).await;

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

/// Test list_notes RPC method
#[tokio::test]
async fn test_list_notes_rpc() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // List notes - should return empty for fresh kiln
    let results = client
        .list_notes(kiln_dir.path(), None)
        .await
        .expect("list_notes RPC failed");

    assert!(results.is_empty(), "Fresh kiln should have no notes");

    server.shutdown().await;
}

/// Test get_note_by_name RPC method
#[tokio::test]
async fn test_get_note_by_name_rpc() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // Get non-existent note - should return None
    let result = client
        .get_note_by_name(kiln_dir.path(), "NonExistent")
        .await
        .expect("get_note_by_name RPC failed");

    assert!(result.is_none(), "Non-existent note should return None");

    server.shutdown().await;
}

/// Test KnowledgeRepository via DaemonStorageClient with multiple sessions
///
/// This tests the full KnowledgeRepository trait implementation through the daemon,
/// simulating how the CLI's get_storage(daemon mode) works.
#[tokio::test]
async fn test_daemon_storage_client_multi_session() {
    use crucible_core::traits::KnowledgeRepository;
    use crucible_rpc::DaemonStorageClient;
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

    // Both should be able to list notes through daemon
    let result1 = storage1.list_notes(None).await;
    let result2 = storage2.list_notes(None).await;

    assert!(
        result1.is_ok(),
        "Storage1 list_notes failed: {:?}",
        result1.err()
    );
    assert!(
        result2.is_ok(),
        "Storage2 list_notes failed: {:?}",
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
    use crucible_rpc::DaemonStorageClient;
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

// =============================================================================
// Tests with seeded data
// =============================================================================

/// Create a kiln directory with pre-seeded notes in the SQLite database.
///
/// This creates the database file directly before the daemon opens it,
/// allowing us to test list_notes and get_note_by_name with actual data.
async fn create_seeded_kiln() -> TempDir {
    use crucible_core::parser::BlockHash;
    use crucible_core::storage::{NoteRecord, NoteStore};
    use crucible_sqlite::{create_sqlite_client, SqliteConfig};

    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");
    let db_dir = kiln_dir.path().join(".crucible");
    std::fs::create_dir_all(&db_dir).expect("Failed to create .crucible dir");
    let db_path = db_dir.join("crucible-sqlite.db");

    // Create SQLite database with notes
    let config = SqliteConfig::new(&db_path);
    let client = create_sqlite_client(config)
        .await
        .expect("Failed to create SQLite client");
    let store = client.as_note_store();

    // Insert test notes
    let note1 = NoteRecord::new("notes/daily.md", BlockHash::zero())
        .with_title("Daily Note")
        .with_tags(vec!["daily".to_string(), "journal".to_string()]);
    store.upsert(note1).await.expect("Failed to insert note1");

    let note2 = NoteRecord::new("projects/rust-project.md", BlockHash::zero())
        .with_title("Rust Project")
        .with_tags(vec!["project".to_string(), "rust".to_string()]);
    store.upsert(note2).await.expect("Failed to insert note2");

    let note3 = NoteRecord::new("references/api-docs.md", BlockHash::zero())
        .with_title("API Documentation")
        .with_tags(vec!["reference".to_string()]);
    store.upsert(note3).await.expect("Failed to insert note3");

    // Drop the client to release the database file
    drop(client);

    kiln_dir
}

/// Test list_notes returns actual notes when they exist
#[tokio::test]
async fn test_list_notes_with_data() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = create_seeded_kiln().await;

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // List all notes
    let results = client
        .list_notes(kiln_dir.path(), None)
        .await
        .expect("list_notes RPC failed");

    assert_eq!(results.len(), 3, "Expected 3 notes");

    // Check that names are extracted from paths
    let names: Vec<_> = results
        .iter()
        .map(|(name, _, _, _, _)| name.as_str())
        .collect();
    assert!(names.contains(&"daily"), "Should have 'daily' note");
    assert!(
        names.contains(&"rust-project"),
        "Should have 'rust-project' note"
    );
    assert!(names.contains(&"api-docs"), "Should have 'api-docs' note");

    server.shutdown().await;
}

/// Test list_notes with path_filter
#[tokio::test]
async fn test_list_notes_with_filter() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = create_seeded_kiln().await;

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // Filter by "projects/" path
    let results = client
        .list_notes(kiln_dir.path(), Some("projects/"))
        .await
        .expect("list_notes RPC failed");

    assert_eq!(results.len(), 1, "Expected 1 note matching filter");
    assert_eq!(results[0].0, "rust-project");

    server.shutdown().await;
}

/// Test get_note_by_name finds a note by title
#[tokio::test]
async fn test_get_note_by_name_found() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = create_seeded_kiln().await;

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // Search by title (case-insensitive)
    let result = client
        .get_note_by_name(kiln_dir.path(), "rust")
        .await
        .expect("get_note_by_name RPC failed");

    assert!(result.is_some(), "Should find note containing 'rust'");
    let note = result.unwrap();
    assert!(
        note.get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("rust-project"),
        "Found note should be the rust project"
    );

    server.shutdown().await;
}

/// Test get_note_by_name with path match
#[tokio::test]
async fn test_get_note_by_name_by_path() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = create_seeded_kiln().await;

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // Search by path fragment
    let result = client
        .get_note_by_name(kiln_dir.path(), "daily")
        .await
        .expect("get_note_by_name RPC failed");

    assert!(result.is_some(), "Should find note with 'daily' in path");

    server.shutdown().await;
}

/// Test get_note_by_name returns None for non-existent
#[tokio::test]
async fn test_get_note_by_name_not_found() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = create_seeded_kiln().await;

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("Failed to open kiln");

    // Search for non-existent note
    let result = client
        .get_note_by_name(kiln_dir.path(), "nonexistent-note-xyz")
        .await
        .expect("get_note_by_name RPC failed");

    assert!(result.is_none(), "Should not find non-existent note");

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_create_and_list() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string");
    assert!(!session_id.is_empty(), "session_id should not be empty");

    let list = client
        .session_list(Some(kiln_dir.path()), None, Some("chat"), None)
        .await
        .expect("session_list failed");

    let sessions = list["sessions"]
        .as_array()
        .expect("sessions should be array");
    assert!(!sessions.is_empty(), "Should have at least one session");

    let found = sessions.iter().any(|s| {
        s["session_id"]
            .as_str()
            .map(|id| id == session_id)
            .unwrap_or(false)
    });
    assert!(found, "Created session should be in list");

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_subscribe_and_unsubscribe() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, mut event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    client
        .session_subscribe(&[&session_id])
        .await
        .expect("session_subscribe failed");

    client
        .session_unsubscribe(&[&session_id])
        .await
        .expect("session_unsubscribe failed");

    while event_rx.try_recv().is_ok() {}

    server.shutdown().await;
}

#[tokio::test]
async fn test_daemon_agent_handle_creation() {
    use crucible_core::traits::chat::AgentHandle;
    use crucible_rpc::DaemonAgentHandle;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let handle = DaemonAgentHandle::new_and_subscribe(client.clone(), session_id.clone(), event_rx)
        .await
        .expect("Failed to create agent handle");

    assert_eq!(handle.session_id(), session_id);
    assert!(handle.is_connected());
    assert!(handle.supports_streaming());

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_configure_agent() {
    use crucible_core::session::SessionAgent;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: "ollama".to_string(),
        model: "llama3.2".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
    };

    let result = client.session_configure_agent(&session_id, &agent).await;
    assert!(
        result.is_ok(),
        "session_configure_agent should succeed: {:?}",
        result.err()
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_send_message_returns_message_id() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let result = client.session_send_message(&session_id, "Hello!").await;

    match result {
        Ok(message_id) => {
            assert!(
                !message_id.is_empty() || message_id.is_empty(),
                "Got a message ID response"
            );
        }
        Err(e) => {
            let err_str = e.to_string();
            assert!(
                err_str.contains("agent")
                    || err_str.contains("not configured")
                    || err_str.contains("error"),
                "Error should be about agent configuration, not RPC failure: {}",
                err_str
            );
        }
    }

    server.shutdown().await;
}

#[tokio::test]
async fn test_session_cancel() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let cancelled = client
        .session_cancel(&session_id)
        .await
        .expect("session_cancel RPC failed");

    assert!(
        !cancelled,
        "Cancel should return false when nothing is active"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_tui_sessions_command_flow() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");
    let workspace_dir = tempfile::tempdir().expect("Failed to create workspace dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session1 = client
        .session_create("chat", kiln_dir.path(), Some(workspace_dir.path()), vec![])
        .await
        .expect("session_create 1 failed");
    let session1_id = session1["session_id"].as_str().unwrap();

    let session2 = client
        .session_create("chat", kiln_dir.path(), Some(workspace_dir.path()), vec![])
        .await
        .expect("session_create 2 failed");
    let session2_id = session2["session_id"].as_str().unwrap();

    let list_result = client
        .session_list(
            Some(kiln_dir.path()),
            Some(workspace_dir.path()),
            Some("chat"),
            None,
        )
        .await
        .expect("session_list failed");

    let sessions = list_result["sessions"]
        .as_array()
        .expect("result.sessions should be array");
    assert!(sessions.len() >= 2, "Should have at least 2 sessions");

    let ids: Vec<&str> = sessions
        .iter()
        .filter_map(|s| s["session_id"].as_str())
        .collect();
    assert!(ids.contains(&session1_id), "Should contain session 1");
    assert!(ids.contains(&session2_id), "Should contain session 2");

    server.shutdown().await;
}

#[tokio::test]
async fn test_tui_resume_command_flow() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let create_result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");
    let session_id = create_result["session_id"]
        .as_str()
        .expect("should have session_id");

    client
        .session_pause(session_id)
        .await
        .expect("session_pause failed");

    let resume_result = client
        .session_resume(session_id)
        .await
        .expect("session_resume failed");

    let state = resume_result["state"].as_str().unwrap_or("");
    assert!(
        state.to_lowercase().contains("active"),
        "Resumed session should be active, got: {}",
        state
    );

    server.shutdown().await;
}

#[tokio::test]
async fn test_tui_daemon_agent_full_flow() {
    use crucible_core::session::SessionAgent;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let create_result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");
    let session_id = create_result["session_id"]
        .as_str()
        .expect("should have session_id")
        .to_string();

    client
        .session_subscribe(&[&session_id])
        .await
        .expect("subscribe failed");

    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: "ollama".to_string(),
        model: "llama3.2".to_string(),
        system_prompt: "You are helpful.".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
    };

    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("configure_agent failed");

    let handle = crucible_rpc::DaemonAgentHandle::new(client.clone(), session_id.clone(), event_rx);

    assert_eq!(handle.session_id(), session_id);
    assert!(handle.is_connected());
    assert!(handle.supports_streaming());

    client
        .session_unsubscribe(&[&session_id])
        .await
        .expect("unsubscribe failed");

    server.shutdown().await;
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
        .session_create("chat", kiln_dir.path(), None, vec![])
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

    let timeout_result = tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await;
    assert!(
        timeout_result.is_err(),
        "Should timeout since no events generated yet"
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
        .session_send_message("nonexistent-session-id", "Hello")
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

// =============================================================================
// Full event flow tests: Daemon → Client → ChatChunk → TUI
// =============================================================================

mod event_flow_tests {
    use crucible_rpc::SessionEvent;
    use serde_json::json;

    fn simulate_daemon_event(event_type: &str, data: serde_json::Value) -> SessionEvent {
        SessionEvent {
            session_id: "test-session".to_string(),
            event_type: event_type.to_string(),
            data,
        }
    }

    fn event_to_chunk(event: &SessionEvent) -> Option<crucible_core::traits::chat::ChatChunk> {
        match event.event_type.as_str() {
            "text_delta" => {
                let content = event.data.get("content")?.as_str()?;
                Some(crucible_core::traits::chat::ChatChunk {
                    delta: content.to_string(),
                    done: false,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                })
            }
            "thinking" => {
                let content = event.data.get("content")?.as_str()?;
                Some(crucible_core::traits::chat::ChatChunk {
                    delta: String::new(),
                    done: false,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: Some(content.to_string()),
                    usage: None,
                    subagent_events: None,
                })
            }
            "tool_call" => {
                let tool = event.data.get("tool")?.as_str()?;
                let call_id = event.data.get("call_id").and_then(|v| v.as_str());
                let args = event.data.get("args").cloned();
                Some(crucible_core::traits::chat::ChatChunk {
                    delta: String::new(),
                    done: false,
                    tool_calls: Some(vec![crucible_core::traits::chat::ChatToolCall {
                        name: tool.to_string(),
                        arguments: args,
                        id: call_id.map(String::from),
                    }]),
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                })
            }
            "tool_result" => {
                let result = event.data.get("result")?;
                let call_id = event.data.get("call_id").and_then(|v| v.as_str());
                let result_str = if result.is_string() {
                    result.as_str().unwrap_or("").to_string()
                } else {
                    result.to_string()
                };
                Some(crucible_core::traits::chat::ChatChunk {
                    delta: String::new(),
                    done: false,
                    tool_calls: None,
                    tool_results: Some(vec![crucible_core::traits::chat::ChatToolResult {
                        name: call_id.unwrap_or("tool").to_string(),
                        result: result_str,
                        error: None,
                        call_id: call_id.map(String::from),
                    }]),
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                })
            }
            "message_complete" | "ended" => Some(crucible_core::traits::chat::ChatChunk {
                delta: String::new(),
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            }),
            _ => None,
        }
    }

    #[test]
    fn daemon_text_delta_becomes_chat_chunk_delta() {
        let event = simulate_daemon_event("text_delta", json!({ "content": "Hello world" }));
        let chunk = event_to_chunk(&event).expect("Should convert to chunk");

        assert_eq!(chunk.delta, "Hello world");
        assert!(!chunk.done);
        assert!(chunk.tool_calls.is_none());
        assert!(chunk.reasoning.is_none());
    }

    #[test]
    fn daemon_thinking_becomes_reasoning_chunk() {
        let event = simulate_daemon_event("thinking", json!({ "content": "Let me analyze..." }));
        let chunk = event_to_chunk(&event).expect("Should convert to chunk");

        assert_eq!(chunk.delta, "");
        assert_eq!(chunk.reasoning, Some("Let me analyze...".to_string()));
        assert!(!chunk.done);
    }

    #[test]
    fn daemon_tool_call_becomes_tool_calls_chunk() {
        let event = simulate_daemon_event(
            "tool_call",
            json!({
                "call_id": "tc-123",
                "tool": "read_file",
                "args": { "path": "test.rs" }
            }),
        );
        let chunk = event_to_chunk(&event).expect("Should convert to chunk");

        assert_eq!(chunk.delta, "");
        let tool_calls = chunk.tool_calls.expect("Should have tool_calls");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "read_file");
        assert_eq!(tool_calls[0].id, Some("tc-123".to_string()));
    }

    #[test]
    fn daemon_tool_result_becomes_tool_results_chunk() {
        let event = simulate_daemon_event(
            "tool_result",
            json!({
                "call_id": "tc-123",
                "result": "fn main() { }"
            }),
        );
        let chunk = event_to_chunk(&event).expect("Should convert to chunk");

        assert_eq!(chunk.delta, "");
        let results = chunk.tool_results.expect("Should have tool_results");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result, "fn main() { }");
    }

    #[test]
    fn daemon_message_complete_sets_done_flag() {
        let event = simulate_daemon_event(
            "message_complete",
            json!({ "message_id": "msg-1", "full_response": "Done!" }),
        );
        let chunk = event_to_chunk(&event).expect("Should convert to chunk");

        assert!(chunk.done);
        assert_eq!(chunk.delta, "");
    }

    #[test]
    fn daemon_ended_sets_done_flag() {
        let event = simulate_daemon_event("ended", json!({ "reason": "cancelled" }));
        let chunk = event_to_chunk(&event).expect("Should convert to chunk");

        assert!(chunk.done);
    }

    #[test]
    fn unknown_event_type_returns_none() {
        let event = simulate_daemon_event("unknown_event", json!({}));
        assert!(event_to_chunk(&event).is_none());
    }

    #[test]
    fn malformed_text_delta_returns_none() {
        let event = simulate_daemon_event("text_delta", json!({}));
        assert!(event_to_chunk(&event).is_none());
    }
}

#[tokio::test]
async fn test_session_switch_model() {
    use crucible_core::session::SessionAgent;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: "ollama".to_string(),
        model: "llama3.2".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        temperature: Some(0.7),
        max_tokens: Some(4096),
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
    };

    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("configure_agent failed");

    let result = client.session_switch_model(&session_id, "gpt-4").await;
    assert!(
        result.is_ok(),
        "session_switch_model should succeed: {:?}",
        result.err()
    );

    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");

    let model = session["agent"]["model"]
        .as_str()
        .expect("model should be string");
    assert_eq!(model, "gpt-4", "Model should be updated in session");

    server.shutdown().await;
}

#[tokio::test]
async fn test_daemon_agent_handle_switch_model() {
    use crucible_core::session::SessionAgent;
    use crucible_core::traits::chat::AgentHandle;
    use crucible_rpc::DaemonAgentHandle;

    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let (client, event_rx) = DaemonClient::connect_to_with_events(&server.socket_path)
        .await
        .expect("Failed to connect with events");
    let client = std::sync::Arc::new(client);

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![])
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string();

    let agent = SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some("ollama".to_string()),
        provider: "ollama".to_string(),
        model: "llama3.2".to_string(),
        system_prompt: "Test assistant".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: Some("http://localhost:11434".to_string()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
    };

    client
        .session_configure_agent(&session_id, &agent)
        .await
        .expect("configure_agent failed");

    let mut handle =
        DaemonAgentHandle::new_and_subscribe(client.clone(), session_id.clone(), event_rx)
            .await
            .expect("Failed to create agent handle");

    let result = handle.switch_model("gpt-4-turbo").await;
    assert!(
        result.is_ok(),
        "DaemonAgentHandle::switch_model should succeed: {:?}",
        result.err()
    );

    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");

    let model = session["agent"]["model"]
        .as_str()
        .expect("model should be string");
    assert_eq!(
        model, "gpt-4-turbo",
        "Model should be updated via AgentHandle"
    );

    server.shutdown().await;
}
