//! `cru search` finding a word that appears only inside a note.
//!
//! Asserted against the surface the command uses — the `search_text` RPC on a
//! real server with a real index — rather than against `tools/search.rs`, the
//! ripgrep walk the agent tools use. That path already worked the whole time
//! `cru search` was broken, so a test there passes and proves nothing.

use anyhow::Result;
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

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

/// Wait until the note is in the metadata index, so a later empty text search
/// means "the body was not indexed" and not "the file has not landed yet".
async fn wait_until_indexed(client: &DaemonClient, kiln: &Path, name: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let notes = client
            .list_notes(kiln, None, None)
            .await
            .expect("list_notes RPC failed");
        if notes.iter().any(|(n, _, _, _, _)| n == name) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "note '{name}' never reached the index"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn text_search_matches_words_in_note_bodies() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    // The sentinel appears ONLY in the body — not in the title, not in the
    // filename. That is the whole point: the old implementation matched names
    // and titles, so a test whose sentinel appeared in either would pass
    // against it.
    std::fs::write(
        kiln_dir.path().join("meeting.md"),
        "# Meeting\n\nzqxjvbn is the distinctive body word.\n",
    )
    .expect("failed to write note");
    wait_until_indexed(&client, kiln_dir.path(), "meeting").await;

    let hits = client
        .search_text(kiln_dir.path(), "zqxjvbn", 20)
        .await
        .expect("search_text RPC failed");

    assert!(
        hits.iter().any(|h| h.path.contains("meeting")),
        "a word in a note's body must be findable; got {:?}",
        hits.iter().map(|h| &h.path).collect::<Vec<_>>()
    );

    server.shutdown().await;
}

/// Title matches must keep working — they are the half that was never broken,
/// and FTS5 indexes the title column too.
#[tokio::test]
async fn text_search_still_matches_titles() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    std::fs::write(
        kiln_dir.path().join("architecture.md"),
        "---\ntitle: Wikilink Resolution\n---\n\nbody text\n",
    )
    .expect("failed to write note");
    wait_until_indexed(&client, kiln_dir.path(), "architecture").await;

    let hits = client
        .search_text(kiln_dir.path(), "Wikilink", 20)
        .await
        .expect("search_text RPC failed");

    assert!(
        hits.iter().any(|h| h.path.contains("architecture")),
        "a word in the title must still be findable; got {:?}",
        hits.iter().map(|h| &h.path).collect::<Vec<_>>()
    );

    server.shutdown().await;
}

/// A user's query is words, not FTS5 syntax. `foo-bar` and a stray quote are
/// both valid input and both are MATCH syntax errors unquoted — which would
/// surface as "search failed" for ordinary typing.
#[tokio::test]
async fn punctuation_in_a_query_does_not_error() {
    let server = TestServer::start().await.expect("Failed to start server");
    let kiln_dir = tempfile::tempdir().expect("Failed to create kiln dir");

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");
    client
        .kiln_open(kiln_dir.path())
        .await
        .expect("kiln_open failed");

    for query in ["foo-bar", "what\"s this", "AND", "*", "a OR b"] {
        client
            .search_text(kiln_dir.path(), query, 20)
            .await
            .unwrap_or_else(|e| panic!("query {query:?} should not error: {e:#}"));
    }

    server.shutdown().await;
}
