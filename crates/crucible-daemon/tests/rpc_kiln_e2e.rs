//! End-to-end tests for kiln + note RPC methods.
//!
//! Covers the full round-trip: client → daemon → storage → response
//! for kiln.open, kiln.list, kiln.close, list_notes, and get_note_by_name.

use anyhow::Result;
use crucible_core::parser::BlockHash;
use crucible_core::storage::NoteRecord;
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use crucible_sqlite::{create_sqlite_client, SqliteConfig};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// In-process test server (mirrors TestServer from rpc_integration.rs).
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

/// Create a kiln directory with pre-seeded notes in the SQLite database.
async fn create_seeded_kiln() -> TempDir {
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");
    let db_dir = kiln_dir.path().join(".crucible");
    std::fs::create_dir_all(&db_dir).expect("Failed to create .crucible dir");
    let db_path = db_dir.join("crucible-sqlite.db");

    let config = SqliteConfig::new(&db_path);
    let client = create_sqlite_client(config)
        .await
        .expect("Failed to create SQLite client");
    let store = client.as_note_store();

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

    drop(client);
    kiln_dir
}

// =============================================================================
// kiln.open
// =============================================================================

#[tokio::test]
async fn test_kiln_open_with_temp_dir() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Opening an empty temp dir should succeed
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open should succeed for valid directory");

    server.shutdown().await;
}

// =============================================================================
// kiln.list
// =============================================================================

#[tokio::test]
async fn test_kiln_list_shows_opened_kiln() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // List should be empty before opening
    let list = client.kiln_list().await.expect("kiln_list failed");
    assert!(list.is_empty(), "No kilns should be open initially");

    // Open kiln
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    // List should now contain the opened kiln
    let list = client.kiln_list().await.expect("kiln_list failed");
    assert_eq!(list.len(), 1, "Should have exactly one kiln");

    let kiln_path = list[0]["path"].as_str().expect("path should be string");
    assert!(
        kiln_path.contains(kiln_dir.path().to_str().unwrap()),
        "Listed kiln path should match the opened path"
    );

    server.shutdown().await;
}

// =============================================================================
// kiln.close
// =============================================================================

#[tokio::test]
async fn test_kiln_close_removes_from_list() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Open kiln
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    // Verify it appears in list
    let list = client.kiln_list().await.expect("kiln_list failed");
    assert_eq!(list.len(), 1, "Should have one kiln open");

    // Close kiln via raw RPC call (no typed method on DaemonClient)
    let result = client
        .call(
            "kiln.close",
            serde_json::json!({ "path": kiln_dir.path().to_string_lossy() }),
        )
        .await
        .expect("kiln.close RPC failed");
    assert_eq!(
        result["status"].as_str(),
        Some("ok"),
        "Close should return status ok"
    );

    // Verify kiln is gone from list
    let list = client.kiln_list().await.expect("kiln_list failed");
    assert!(list.is_empty(), "Kiln should be removed after close");

    server.shutdown().await;
}

// =============================================================================
// list_notes
// =============================================================================

#[tokio::test]
async fn test_list_notes_returns_seeded_notes() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = create_seeded_kiln().await;

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    let notes = client
        .list_notes(kiln_dir.path(), None)
        .await
        .expect("list_notes RPC failed");

    assert_eq!(notes.len(), 3, "Should return all 3 seeded notes");

    let names: Vec<&str> = notes
        .iter()
        .map(|(name, _, _, _, _)| name.as_str())
        .collect();
    assert!(names.contains(&"daily"), "Should contain 'daily' note");
    assert!(
        names.contains(&"rust-project"),
        "Should contain 'rust-project' note"
    );
    assert!(
        names.contains(&"api-docs"),
        "Should contain 'api-docs' note"
    );

    server.shutdown().await;
}

// =============================================================================
// get_note_by_name
// =============================================================================

#[tokio::test]
async fn test_get_note_by_name_returns_matching_note() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = create_seeded_kiln().await;

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    // Search by name fragment
    let result = client
        .get_note_by_name(kiln_dir.path(), "daily")
        .await
        .expect("get_note_by_name RPC failed");

    assert!(result.is_some(), "Should find note matching 'daily'");
    let note = result.unwrap();
    assert!(
        note.get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("daily"),
        "Found note path should contain 'daily'"
    );

    // Non-existent note should return None
    let result = client
        .get_note_by_name(kiln_dir.path(), "nonexistent-xyz-abc")
        .await
        .expect("get_note_by_name RPC failed");
    assert!(result.is_none(), "Non-existent note should return None");

    server.shutdown().await;
}

// =============================================================================
// Combined: open → list_notes with filter → close → verify empty
// =============================================================================

#[tokio::test]
async fn test_kiln_lifecycle_open_query_close() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = create_seeded_kiln().await;

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Open
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    // Query with path filter
    let notes = client
        .list_notes(kiln_dir.path(), Some("projects/"))
        .await
        .expect("list_notes with filter failed");
    assert_eq!(notes.len(), 1, "Filter should match one note");
    assert_eq!(
        notes[0].0, "rust-project",
        "Filtered note should be rust-project"
    );

    // get_note_by_name for the found note
    let note = client
        .get_note_by_name(kiln_dir.path(), "rust")
        .await
        .expect("get_note_by_name failed")
        .expect("Should find rust project note");
    assert!(
        note["path"].as_str().unwrap_or("").contains("rust-project"),
        "Note path should contain rust-project"
    );

    // Close
    let close_result = client
        .call(
            "kiln.close",
            serde_json::json!({ "path": kiln_dir.path().to_string_lossy() }),
        )
        .await
        .expect("kiln.close failed");
    assert_eq!(close_result["status"].as_str(), Some("ok"));

    // Verify kiln list is empty
    let list = client.kiln_list().await.expect("kiln_list failed");
    assert!(list.is_empty(), "Kiln should be removed after close");

    server.shutdown().await;
}
