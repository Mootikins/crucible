//! Memory-scoping RPC tests.
//!
//! Verifies the post-C2 invariants:
//! - The `scope` field in RPC params is **accepted** by the server (legacy
//!   clients keep working without errors) but **ignored**: read authority
//!   always derives from the `kiln` path.
//! - `note.upsert` rejects writes whose declared `properties.scope` doesn't
//!   match the kiln being written to (C1 boundary).
//!
//! Pre-C2, a client could pass `Scope::Workspace { path: "/elsewhere" }` to
//! constrain or redirect a read — the SQL filter would honor whatever the
//! caller supplied. That made the "authority" caller-controlled rather than
//! session-bound. The fix is to ignore the param entirely on reads.

use crucible_core::parser::BlockHash;
use crucible_core::storage::{NoteRecord, Scope};
use crucible_daemon::DaemonClient;

use super::server::TestServer;

#[tokio::test]
async fn search_vectors_rpc_accepts_scope_param() {
    // The RPC client method exposes a `scope` arg for backward compat.
    // Server accepts it without erroring; under C2 the server then ignores
    // it and derives authority from `kiln_path`. Vector search itself
    // returns empty (no embedding seeded).
    let server = TestServer::start().await.expect("server");
    let kiln_dir = tempfile::tempdir().expect("kiln");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("connect");
    client.kiln_open(kiln_dir.path()).await.expect("open");

    let result = client
        .search_vectors(
            kiln_dir.path(),
            &vec![0.1; 768],
            5,
            Some(Scope::workspace_unchecked(kiln_dir.path())),
        )
        .await;

    assert!(
        result.is_ok(),
        "scope param must be accepted: {:?}",
        result.err()
    );
    server.shutdown().await;
}

#[tokio::test]
async fn search_vectors_rpc_defaults_to_workspace_when_scope_missing() {
    // Legacy clients (no scope param) must still work.
    let server = TestServer::start().await.expect("server");
    let kiln_dir = tempfile::tempdir().expect("kiln");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("connect");
    client.kiln_open(kiln_dir.path()).await.expect("open");

    let result = client
        .search_vectors(kiln_dir.path(), &vec![0.1; 768], 5, None)
        .await;
    assert!(result.is_ok(), "legacy RPC must still work: {:?}", result);
    server.shutdown().await;
}

#[tokio::test]
async fn list_notes_rpc_ignores_client_supplied_scope() {
    // Post-C2 invariant: a client cannot redirect or constrain a read by
    // supplying a `scope` param. The server derives authority from
    // `kiln_path` and the param is silently ignored.
    //
    // Pre-fix, this test seeded a note with cross-workspace scope to
    // demonstrate SQL-level filtering; C1 now refuses such writes at the
    // RPC boundary (see kiln_scope_validation::* unit tests), so we set up
    // the kiln normally and verify the read-side ignores any forged
    // override.
    let server = TestServer::start().await.expect("server");
    let kiln_dir = tempfile::tempdir().expect("kiln dir");
    let kiln_path = kiln_dir.path().to_path_buf();
    std::mem::forget(kiln_dir);

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("connect");
    client.kiln_open(&kiln_path).await.expect("kiln_open");

    // Insert a kiln-scoped note normally. C1 stamps the kiln scope.
    let note = NoteRecord::new("scoped.md", BlockHash::zero()).with_title("Scoped");
    client
        .note_upsert(&kiln_path, &note)
        .await
        .expect("note_upsert");

    // Read with a stranger workspace scope — must be ignored, the kiln's
    // own notes are returned.
    let stranger = Scope::Workspace {
        path: std::path::PathBuf::from("/this/path/does/not/exist/anywhere"),
    };
    let results = client
        .list_notes(&kiln_path, None, Some(stranger))
        .await
        .expect("list_notes_scoped");

    let names: Vec<_> = results.iter().map(|(n, _, _, _, _)| n.as_str()).collect();
    assert!(
        names.contains(&"scoped"),
        "client-supplied scope must be ignored; kiln-derived authority returns own notes: {:?}",
        names
    );

    server.shutdown().await;
}
