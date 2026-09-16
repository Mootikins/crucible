//! End-to-end tests for kiln + note RPC methods.
//!
//! Covers the full round-trip: client → daemon → storage → response
//! for kiln.open, kiln.list, kiln.close, list_notes, and get_note_by_name.

use anyhow::Result;
use crucible_core::parser::BlockHash;
use crucible_core::storage::NoteRecord;
use crucible_daemon::storage::sqlite::{create_sqlite_client, SqliteConfig};
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// Install the rustls CryptoProvider before any TLS usage. rustls 0.23 refuses
/// to auto-pick a provider when the dependency graph offers more than one, so
/// the in-process daemon panics on its first TLS handshake unless we install
/// one explicitly (idempotent; ignored if already set).
fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// In-process test server (mirrors TestServer from rpc_integration.rs).
struct TestServer {
    _temp_dir: TempDir,
    socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

impl TestServer {
    async fn start() -> Result<Self> {
        ensure_crypto_provider();
        let temp_dir = tempfile::tempdir()?;
        let socket_path = temp_dir.path().join("daemon.sock");

        // One registered kiln named `kiln`: sessions address kilns by name, so
        // a fixture that registers none has a daemon that refuses every scoped
        // request.
        let kiln = temp_dir.path().join("kiln");
        std::fs::create_dir_all(&kiln)?;
        let server = Server::bind_with_data_home_and_kilns(
            &socket_path,
            temp_dir.path().to_path_buf(),
            &[("kiln", &kiln)],
        )
        .await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Poll for readiness rather than sleeping a fixed interval. Under a
        // loaded box the socket may not be accepting when a fixed timer
        // elapses, which is this suite's intermittent-failure source.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if DaemonClient::connect_to(&socket_path).await.is_ok() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "daemon did not start accepting connections within 5s"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

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

/// Wait until the fixture's own registered kiln is open, then answer the paths
/// `kiln.list` reports.
///
/// A registered kiln is OPEN once the daemon has started — boot opens the
/// registry's eager entries, so that a restart does not leave every
/// kiln-addressed route answering 404. That open runs in its own task, so a
/// test that counted the list immediately would race it. Polling for the
/// steady state is the only honest way to assert about the list's size.
async fn open_kiln_paths(client: &DaemonClient, expected: usize) -> Vec<String> {
    for _ in 0..100 {
        let list = client.kiln_list().await.expect("kiln_list failed");
        // Only the rows that are OPEN. `kiln.list` also reports registered
        // kilns that nothing has opened — the bundled help corpus among them,
        // on a machine where it has been extracted — and these tests are about
        // what `kiln.open` and `kiln.close` do to the open set.
        let open: Vec<String> = list
            .iter()
            .filter(|row| row["open"].as_bool().unwrap_or(true))
            .filter_map(|row| row["path"].as_str().map(str::to_string))
            .collect();
        if open.len() >= expected {
            return open;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "kiln.list never reached {expected} open kilns: {:?}",
        client.kiln_list().await
    );
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

    // The fixture registers one kiln, and a registered kiln is open once the
    // daemon has started. This used to assert an empty list: `kiln.list`
    // reports what the manager holds open, nothing opened the registered
    // entries, and a restart therefore made every kiln-addressed route 404.
    let before = open_kiln_paths(&client, 1).await;
    assert_eq!(before.len(), 1, "the fixture's own kiln, and only it");

    // Open a second kiln by path.
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    let after = open_kiln_paths(&client, 2).await;
    assert_eq!(after.len(), 2, "the fixture's kiln plus the opened one");
    assert!(
        after
            .iter()
            .any(|path| path.contains(kiln_dir.path().to_str().unwrap())),
        "the opened path must be listed: {after:?}"
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

    // The fixture's registered kiln is open from the start; this one is not.
    let before = open_kiln_paths(&client, 1).await;

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    let after = open_kiln_paths(&client, 2).await;
    assert_eq!(after.len(), before.len() + 1, "the open added exactly one");

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

    // Closed, and only the closed one: the fixture's registered kiln stays
    // open, because closing one kiln says nothing about another.
    let list = client.kiln_list().await.expect("kiln_list failed");
    let paths: Vec<&str> = list
        .iter()
        .filter(|row| row["open"].as_bool().unwrap_or(true))
        .filter_map(|row| row["path"].as_str())
        .collect();
    assert!(
        !paths
            .iter()
            .any(|path| path.contains(kiln_dir.path().to_str().unwrap())),
        "the closed kiln must be gone: {paths:?}"
    );
    assert_eq!(paths.len(), before.len(), "nothing else changed: {paths:?}");

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
        .list_notes(kiln_dir.path(), None, None)
        .await
        .expect("list_notes RPC failed");

    assert_eq!(notes.len(), 3, "Should return all 3 seeded notes");

    let names: Vec<&str> = notes.iter().map(|row| row.name.as_str()).collect();
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
        .get_note_by_name(kiln_dir.path(), "daily", None)
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
        .get_note_by_name(kiln_dir.path(), "nonexistent-xyz-abc", None)
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
        .list_notes(kiln_dir.path(), Some("projects/"), None)
        .await
        .expect("list_notes with filter failed");
    assert_eq!(notes.len(), 1, "Filter should match one note");
    assert_eq!(
        notes[0].name, "rust-project",
        "Filtered note should be rust-project"
    );

    // get_note_by_name for the found note
    let note = client
        .get_note_by_name(kiln_dir.path(), "rust", None)
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

    // Closed. The fixture's own registered kiln is still open — boot opened
    // it, and closing this one says nothing about it.
    let list = client.kiln_list().await.expect("kiln_list failed");
    let paths: Vec<&str> = list
        .iter()
        .filter(|row| row["open"].as_bool().unwrap_or(true))
        .filter_map(|row| row["path"].as_str())
        .collect();
    assert!(
        !paths
            .iter()
            .any(|path| path.contains(kiln_dir.path().to_str().unwrap())),
        "the closed kiln must be gone: {paths:?}"
    );

    server.shutdown().await;
}

// =============================================================================
// kiln.register
// =============================================================================

/// A name registered through the socket resolves in the SAME daemon process.
///
/// This is the no-restart guarantee, and it is the reason the registry gained
/// interior mutability. The old path wrote the user's config file and the
/// running daemon kept its startup snapshot, so `cru kiln register work …`
/// followed by a session on `work` was refused until the daemon restarted.
///
/// The test crosses the wire on purpose. A handler-level test can assert the
/// registry was updated, but only a request over the socket proves the
/// registry the handler wrote is the registry `session.create` reads. Those
/// are the same `Arc` today; nothing but this test says they must stay so.
#[tokio::test]
async fn a_registered_name_is_usable_without_restarting_the_daemon() {
    let server = TestServer::start().await.expect("Failed to start server");
    let late = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    // Precondition: the daemon started with one kiln, `kiln`, and it was not
    // this one. A session on `late` must be refused before the registration,
    // or the assertion after it proves nothing.
    let before = client
        .call(
            "session.create",
            serde_json::json!({ "type": "chat", "kilns": ["late"] }),
        )
        .await;
    assert!(
        before.is_err(),
        "precondition: `late` must be unknown before it is registered"
    );

    let reply = client
        .kiln_register("late", late.path(), false, false)
        .await
        .expect("registering an existing directory succeeds");
    assert_eq!(reply["outcome"], serde_json::json!("added"));

    // The registration went to the state file, not to the user's config. The
    // reply names the file, and the version gate's number is in it.
    let state_file = PathBuf::from(
        reply["state_file"]
            .as_str()
            .expect("the reply must name the file it wrote"),
    );
    let state: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&state_file).expect("kilns.json was written"),
    )
    .expect("kilns.json is JSON");
    assert_eq!(state["version"], 1);
    assert!(state["kilns"]["late"]["path"].is_string(), "{state}");

    // No restart, no reconnect, no second client: the same connection.
    let after = client
        .call(
            "session.create",
            serde_json::json!({ "type": "chat", "kilns": ["late"] }),
        )
        .await
        .expect("the name the daemon just registered must resolve now");
    assert!(
        after["session_id"].as_str().is_some(),
        "session.create must accept the freshly registered name: {after}"
    );

    server.shutdown().await;
}

/// The registration survives a restart, because it went to a file.
///
/// The live-registry assertion above would also pass if the handler only
/// mutated memory. This one fails in that case: a second server over the same
/// data home reads `kilns.json` at startup and must already know the name.
#[tokio::test]
async fn a_registered_name_survives_a_daemon_restart() {
    let data_home = tempfile::tempdir().expect("Failed to create data home");
    let late = tempfile::tempdir().expect("Failed to create kiln dir");
    ensure_crypto_provider();

    for round in 0..2 {
        let socket_path = data_home.path().join(format!("daemon-{round}.sock"));
        let server = Server::bind_with_data_home_and_kilns(
            &socket_path,
            data_home.path().to_path_buf(),
            &[],
        )
        .await
        .expect("bind");
        let shutdown = server.shutdown_handle();
        let handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if DaemonClient::connect_to(&socket_path).await.is_ok() {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "daemon never accepted"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let client = DaemonClient::connect_to(&socket_path)
            .await
            .expect("connect");

        if round == 0 {
            client
                .kiln_register("late", late.path(), false, false)
                .await
                .expect("register");
        } else {
            // A fresh process, a fresh registry, no registration call.
            let after = client
                .call(
                    "session.create",
                    serde_json::json!({ "type": "chat", "kilns": ["late"] }),
                )
                .await
                .expect("the registration must have been persisted, not only cached");
            assert!(after["session_id"].as_str().is_some(), "{after}");
        }

        drop(client);
        let _ = shutdown.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.abort();
    }
}

/// The never-re-point rule, across the socket: a name this daemon already holds
/// for one directory is refused for another. A session that persisted `late`
/// must not open a different corpus after a second registration.
#[tokio::test]
async fn registering_a_held_name_over_another_directory_is_refused() {
    let server = TestServer::start().await.expect("Failed to start server");
    let first = tempfile::tempdir().expect("Failed to create kiln dir");
    let second = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_register("late", first.path(), false, false)
        .await
        .expect("the first registration lands");
    let err = client
        .kiln_register("late", second.path(), false, false)
        .await
        .expect_err("the second must be refused");
    assert!(
        err.to_string().contains("already registered"),
        "the refusal must say why: {err}"
    );

    // Re-registering the identical pair stays a no-op, so a setup script that
    // runs twice is not an error.
    let again = client
        .kiln_register("late", first.path(), false, false)
        .await
        .expect("the same name and path again is a no-op");
    assert_eq!(again["outcome"].as_str(), Some("already_present"));

    server.shutdown().await;
}
