//! `cru search` finding a word that appears only inside a note.
//!
//! Asserted against the surface the command uses — the `search_text` RPC on a
//! real server with a real index — rather than against `tools/search.rs`, the
//! ripgrep walk the agent tools use. That path already worked the whole time
//! `cru search` was broken, so a test there passes and proves nothing.

mod common;

use crucible_daemon::DaemonClient;
use std::path::Path;
use std::time::Duration;

async fn start_server() -> common::InProcessDaemon {
    common::InProcessDaemonBuilder::new()
        .expect("a test daemon builder")
        .with_kiln("kiln")
        .start()
        .await
        .expect("Failed to start server")
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
    let server = start_server().await;
    let kiln = tempfile::tempdir().unwrap();
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .unwrap();
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
