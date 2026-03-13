//! End-to-end tests for session lifecycle RPC methods.
//!
//! Covers: session.create, session.list, session.get, session.pause,
//! session.resume, session.end, session.delete, session.archive,
//! session.unarchive — the full session lifecycle.

use anyhow::Result;
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// In-process test server (same pattern as rpc_integration.rs).
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

/// Helper: create a session and return its ID.
async fn create_session(client: &DaemonClient, kiln: &std::path::Path) -> String {
    let result = client
        .session_create("chat", kiln, None, vec![], None, None)
        .await
        .expect("session_create failed");

    result["session_id"]
        .as_str()
        .expect("session_id should be string")
        .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// session.create returns a non-empty session ID.
#[tokio::test]
async fn test_session_create_returns_id() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_create("chat", kiln_dir.path(), None, vec![], None, None)
        .await
        .expect("session_create failed");

    let session_id = result["session_id"]
        .as_str()
        .expect("session_id should be a string");
    assert!(!session_id.is_empty(), "session_id must not be empty");

    server.shutdown().await;
}

/// session.list includes a previously created session.
#[tokio::test]
async fn test_session_list_includes_created() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session_id = create_session(&client, kiln_dir.path()).await;

    let list = client
        .session_list(Some(kiln_dir.path()), None, Some("chat"), None, None)
        .await
        .expect("session_list failed");

    let sessions = list["sessions"]
        .as_array()
        .expect("sessions should be array");

    let found = sessions
        .iter()
        .any(|s| s["session_id"].as_str() == Some(session_id.as_str()));
    assert!(found, "Created session {session_id} must appear in list");

    server.shutdown().await;
}

/// session.get returns details matching the created session.
#[tokio::test]
async fn test_session_get_returns_details() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session_id = create_session(&client, kiln_dir.path()).await;

    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");

    // Verify the returned session has the correct ID and an active state
    assert_eq!(
        session["session_id"].as_str(),
        Some(session_id.as_str()),
        "Returned session_id must match"
    );
    assert_eq!(
        session["state"].as_str(),
        Some("active"),
        "Newly created session should be active"
    );

    server.shutdown().await;
}

/// session.pause transitions the session state to paused.
#[tokio::test]
async fn test_session_pause_changes_state() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session_id = create_session(&client, kiln_dir.path()).await;

    // Pause the session
    client
        .session_pause(&session_id)
        .await
        .expect("session_pause failed");

    // Verify state changed
    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get after pause failed");

    let state = session["state"]
        .as_str()
        .expect("state should be string")
        .to_lowercase();
    assert!(
        state.contains("pause"),
        "Session state should indicate paused, got: {state}"
    );

    server.shutdown().await;
}

/// session.resume transitions a paused session back to active.
#[tokio::test]
async fn test_session_resume_changes_state() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session_id = create_session(&client, kiln_dir.path()).await;

    // Pause first
    client
        .session_pause(&session_id)
        .await
        .expect("session_pause failed");

    // Resume
    let resume_result = client
        .session_resume(&session_id)
        .await
        .expect("session_resume failed");

    let state = resume_result["state"].as_str().unwrap_or("").to_lowercase();
    assert!(
        state.contains("active"),
        "Resumed session should be active, got: {state}"
    );

    // Double-check via session_get
    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get after resume failed");

    let get_state = session["state"]
        .as_str()
        .expect("state should be string")
        .to_lowercase();
    assert!(
        get_state.contains("active"),
        "session_get should confirm active state, got: {get_state}"
    );

    server.shutdown().await;
}

/// session.end removes the session from the active list.
#[tokio::test]
async fn test_session_end_removes_from_list() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session_id = create_session(&client, kiln_dir.path()).await;

    // End the session
    client
        .session_end(&session_id)
        .await
        .expect("session_end failed");

    // Allow time for cleanup
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify: session_get should either fail or return ended state
    let get_result = client.session_get(&session_id).await;
    let is_gone = match &get_result {
        Err(e) => {
            let msg = e.to_string();
            msg.contains("not found") || msg.contains("Not found")
        }
        Ok(val) => val.get("state").and_then(|s| s.as_str()) == Some("ended"),
    };
    assert!(
        is_gone,
        "Session should be ended or removed after session.end, got: {get_result:?}"
    );

    // Verify: session should not appear in active list
    let list = client
        .session_list(
            Some(kiln_dir.path()),
            None,
            Some("chat"),
            Some("active"),
            None,
        )
        .await
        .expect("session_list failed");

    let empty = vec![];
    let sessions = list["sessions"].as_array().unwrap_or(&empty);
    let still_active = sessions
        .iter()
        .any(|s| s["session_id"].as_str() == Some(session_id.as_str()));
    assert!(
        !still_active,
        "Ended session must not appear in active session list"
    );

    server.shutdown().await;
}

/// Full lifecycle: create → get → pause → resume → end.
#[tokio::test]
async fn test_session_full_lifecycle() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // 1. Create
    let session_id = create_session(&client, kiln_dir.path()).await;

    // 2. Get — should be active
    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get failed");
    assert_eq!(session["state"].as_str(), Some("active"));

    // 3. Pause
    client
        .session_pause(&session_id)
        .await
        .expect("session_pause failed");
    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get after pause failed");
    let state = session["state"].as_str().unwrap_or("").to_lowercase();
    assert!(state.contains("pause"), "Expected paused, got: {state}");

    // 4. Resume
    client
        .session_resume(&session_id)
        .await
        .expect("session_resume failed");
    let session = client
        .session_get(&session_id)
        .await
        .expect("session_get after resume failed");
    assert_eq!(
        session["state"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .as_str(),
        "active"
    );

    // 5. End
    client
        .session_end(&session_id)
        .await
        .expect("session_end failed");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let get_result = client.session_get(&session_id).await;
    let ended = match &get_result {
        Err(e) => {
            let msg = e.to_string();
            msg.contains("not found") || msg.contains("Not found")
        }
        Ok(val) => val.get("state").and_then(|s| s.as_str()) == Some("ended"),
    };
    assert!(ended, "Session should be ended, got: {get_result:?}");

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// Delete Tests
// ---------------------------------------------------------------------------

/// session.delete removes the session and confirms deletion.
#[tokio::test]
async fn test_session_delete_removes_session() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session_id = create_session(&client, kiln_dir.path()).await;

    // Delete the session
    let delete_result = client
        .session_delete(&session_id, kiln_dir.path())
        .await
        .expect("session_delete failed");

    // Verify the response indicates deletion
    let deleted = delete_result
        .get("deleted")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(deleted, "session_delete should return deleted: true");

    // Allow time for cleanup
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify: session should not appear in list
    let list = client
        .session_list(Some(kiln_dir.path()), None, Some("chat"), None, None)
        .await
        .expect("session_list failed");

    let empty = vec![];
    let sessions = list["sessions"].as_array().unwrap_or(&empty);
    let still_listed = sessions
        .iter()
        .any(|s| s["session_id"].as_str() == Some(session_id.as_str()));
    assert!(
        !still_listed,
        "Deleted session must not appear in session list"
    );

    server.shutdown().await;
}

/// session.delete with a nonexistent session ID returns an error.
#[tokio::test]
async fn test_session_delete_nonexistent_returns_error() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let result = client
        .session_delete("nonexistent-session-id", kiln_dir.path())
        .await;

    assert!(
        result.is_err(),
        "Deleting a nonexistent session should return an error"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found")
            || err_msg.contains("Not found")
            || err_msg.contains("not allowed"),
        "Error should indicate session not found or not allowed, got: {err_msg}"
    );

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// Archive/Unarchive Tests
// ---------------------------------------------------------------------------

/// session.archive marks a session as archived.
#[tokio::test]
async fn test_session_archive_marks_archived() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session_id = create_session(&client, kiln_dir.path()).await;

    // Archive the session
    let archive_result = client
        .session_archive(&session_id, kiln_dir.path())
        .await
        .expect("session_archive failed");

    let archived = archive_result
        .get("archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(archived, "session_archive should return archived: true");

    server.shutdown().await;
}

/// session.unarchive restores an archived session.
#[tokio::test]
async fn test_session_unarchive_restores_session() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let session_id = create_session(&client, kiln_dir.path()).await;

    // Archive first
    client
        .session_archive(&session_id, kiln_dir.path())
        .await
        .expect("session_archive failed");

    // Then unarchive
    let unarchive_result = client
        .session_unarchive(&session_id, kiln_dir.path())
        .await
        .expect("session_unarchive failed");

    let archived = unarchive_result.get("archived").and_then(|v| v.as_bool());
    assert_eq!(
        archived,
        Some(false),
        "session_unarchive should return archived: false"
    );

    server.shutdown().await;
}

// ---------------------------------------------------------------------------
// include_archived Filter Tests
// ---------------------------------------------------------------------------

/// session.list excludes archived sessions by default.
#[tokio::test]
async fn test_session_list_excludes_archived_by_default() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Create two sessions
    let session_a = create_session(&client, kiln_dir.path()).await;
    let _session_b = create_session(&client, kiln_dir.path()).await;

    // Archive session A
    client
        .session_archive(&session_a, kiln_dir.path())
        .await
        .expect("session_archive failed");

    // List WITHOUT include_archived (default = exclude archived)
    let list = client
        .session_list(Some(kiln_dir.path()), None, Some("chat"), None, Some(false))
        .await
        .expect("session_list failed");

    let empty = vec![];
    let sessions = list["sessions"].as_array().unwrap_or(&empty);

    let found_archived = sessions
        .iter()
        .any(|s| s["session_id"].as_str() == Some(session_a.as_str()));
    assert!(
        !found_archived,
        "Archived session should NOT appear in default listing"
    );

    server.shutdown().await;
}

/// session.list includes archived sessions when include_archived=true.
#[tokio::test]
async fn test_session_list_includes_archived_when_requested() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Create two sessions
    let session_a = create_session(&client, kiln_dir.path()).await;
    let session_b = create_session(&client, kiln_dir.path()).await;

    // Archive session A
    client
        .session_archive(&session_a, kiln_dir.path())
        .await
        .expect("session_archive failed");

    // List WITH include_archived=true
    let list = client
        .session_list(Some(kiln_dir.path()), None, Some("chat"), None, Some(true))
        .await
        .expect("session_list failed");

    let empty = vec![];
    let sessions = list["sessions"].as_array().unwrap_or(&empty);

    let found_a = sessions
        .iter()
        .any(|s| s["session_id"].as_str() == Some(session_a.as_str()));
    let found_b = sessions
        .iter()
        .any(|s| s["session_id"].as_str() == Some(session_b.as_str()));
    assert!(
        found_a,
        "Archived session should appear when include_archived=true"
    );
    assert!(
        found_b,
        "Non-archived session should also appear when include_archived=true"
    );

    server.shutdown().await;
}
