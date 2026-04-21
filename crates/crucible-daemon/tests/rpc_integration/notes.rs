//! Notes RPC tests: list_notes, get_note_by_name, search_vectors.

use crucible_daemon::DaemonClient;
use tempfile::TempDir;

use super::server::TestServer;

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
    use crucible_daemon::DaemonStorageClient;
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
    use crucible_daemon::DaemonStorageClient;
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
    use crucible_core::storage::NoteRecord;
    use crucible_daemon::storage::sqlite::{create_sqlite_client, SqliteConfig};

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
