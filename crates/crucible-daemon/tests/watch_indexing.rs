//! The file watcher, end to end: a note written into an open kiln while the
//! daemon runs is indexed without anyone asking.
//!
//! **This suite exists because four green ones proved the wrong thing.** The
//! `watch_*_emission_tests.rs` files build an `IndexingHandler` by hand, hand it
//! a `MockEventEmitter`, and assert the mock saw the event. Every one of them
//! passed for the entire period in which `create_default_handlers()` returned an
//! empty registry and the handler was registered nowhere — they demonstrate that
//! the handler works when you construct it yourself, which is not a claim anyone
//! needs. Nothing here constructs a handler: the server builds its own registry
//! through the path `kiln.open` takes in production, and the assertion is on
//! what `list_notes` returns.

use anyhow::Result;
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

/// Install the rustls CryptoProvider before any TLS usage (see rpc_kiln_e2e).
fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

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

        // Isolated data home as a value, never process env: a shared
        // `~/.crucible` would let the developer's real registry decide what
        // this test watches.
        let server =
            Server::bind_with_data_home(&socket_path, temp_dir.path().to_path_buf()).await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

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

/// Poll `list_notes` until `predicate` holds or the deadline passes.
///
/// Polling on the condition rather than sleeping a guessed interval: the chain
/// under test is a 500ms watch debounce plus a broadcast hop plus a pipeline
/// run, and any fixed sleep either flakes or wastes the difference.
async fn wait_for_notes<F>(
    client: &DaemonClient,
    kiln: &Path,
    timeout: Duration,
    predicate: F,
) -> Vec<String>
where
    F: Fn(&[String]) -> bool,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let names: Vec<String> = client
            .list_notes(kiln, None, None)
            .await
            .expect("list_notes RPC failed")
            .into_iter()
            .map(|(name, _, _, _, _)| name)
            .collect();

        if predicate(&names) {
            return names;
        }
        if tokio::time::Instant::now() >= deadline {
            return names;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn note_created_while_daemon_runs_becomes_searchable() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    let before = client
        .list_notes(kiln_dir.path(), None, None)
        .await
        .expect("list_notes RPC failed");
    assert!(before.is_empty(), "fresh kiln should hold no notes");

    // The user creates a note. Nothing else happens — no `cru process`, no RPC.
    std::fs::write(
        kiln_dir.path().join("new-note.md"),
        "# New Note\n\nzqxjvbn distinctive body word\n",
    )
    .expect("failed to write note");

    let names = wait_for_notes(&client, kiln_dir.path(), Duration::from_secs(10), |names| {
        names.iter().any(|n| n == "new-note")
    })
    .await;

    assert!(
        names.iter().any(|n| n == "new-note"),
        "a note created while the daemon watches an open kiln should be indexed \
         without `cru process`; list_notes returned {names:?}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn note_deleted_while_daemon_runs_leaves_the_index() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    let note_path = kiln_dir.path().join("doomed.md");
    std::fs::write(&note_path, "# Doomed\n\nbody\n").expect("failed to write note");

    let names = wait_for_notes(&client, kiln_dir.path(), Duration::from_secs(10), |names| {
        names.iter().any(|n| n == "doomed")
    })
    .await;
    assert!(
        names.iter().any(|n| n == "doomed"),
        "the note must be indexed before its removal can mean anything; got {names:?}"
    );

    std::fs::remove_file(&note_path).expect("failed to delete note");

    let names = wait_for_notes(&client, kiln_dir.path(), Duration::from_secs(10), |names| {
        !names.iter().any(|n| n == "doomed")
    })
    .await;
    assert!(
        !names.iter().any(|n| n == "doomed"),
        "deleting the file should drop it from the index; list_notes returned {names:?}"
    );

    server.shutdown().await;
}
