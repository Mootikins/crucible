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
        tokio::time::timeout(Duration::from_secs(5), self._server_handle)
            .await
            .unwrap()
            .unwrap();
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
        if notes.iter().any(|row| row.name == name) {
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
async fn text_search_preserves_body_title_file_kind_and_query_semantics() {
    let server = TestServer::start().await.unwrap();
    let kiln = tempfile::tempdir().unwrap();
    let client = DaemonClient::connect_to(&server.socket_path).await.unwrap();
    client.kiln_open(kiln.path()).await.unwrap();
    for (name, content) in [
        (
            "meeting.md",
            "# Meeting\n\nzqxjvbn is the distinctive body word.\n",
        ),
        (
            "scratch.txt",
            "no headings. qfmzlrt is the distinctive body word.\n",
        ),
        ("diagram.png", "assetonlytoken should never be indexed.\n"),
        (
            "architecture.md",
            "---\ntitle: Wikilink Resolution\n---\n\nbody text\n",
        ),
        (
            "spread.md",
            "# Spread\n\nzqxjvbn appears here, and much later wbtqkdh does too.\n",
        ),
    ] {
        std::fs::write(kiln.path().join(name), content).unwrap();
    }
    for name in ["meeting", "scratch", "architecture", "spread"] {
        wait_until_indexed(&client, kiln.path(), name).await;
    }
    for (query, expected) in [
        ("zqxjvbn", Some("meeting")),
        ("qfmzlrt", Some("scratch")),
        ("Wikilink", Some("architecture")),
        ("zqxjvbn wbtqkdh", Some("spread")),
        ("\"zqxjvbn wbtqkdh\"", None),
        ("assetonlytoken", None),
    ] {
        let hits = client.search_text(kiln.path(), query, 20).await.unwrap();
        assert!(
            match expected {
                Some(name) => hits.iter().any(|hit| hit.path.contains(name)),
                None => hits.is_empty(),
            },
            "query {query:?}: expected {expected:?}, got {hits:?}"
        );
    }
    for query in ["foo-bar", "what\"s this", "AND", "*", "a OR b"] {
        client
            .search_text(kiln.path(), query, 20)
            .await
            .unwrap_or_else(|e| panic!("query {query:?} should not error: {e:#}"));
    }
    drop(client);
    server.shutdown().await;
}
