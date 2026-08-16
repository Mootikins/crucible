//! End-to-end tests for kiln-less session creation — the branch's headline
//! feature: `session.create` with `kilns` omitted yields a session with NO
//! kiln, a tools-only agent that still composes with the mid-session scope
//! RPCs.
//!
//! Zero kilns is a legitimate state (§4.1), not a request for a default. The
//! daemon used to substitute its own data root, which is the PARENT of the
//! sessions root — so every kiln-less session carried an allowed root
//! enclosing every transcript the daemon had ever written, and `grep` walked
//! straight into it. An empty kiln set degrades capabilities (no note or kiln
//! tools, no precognition, no semantic search); it must never degrade
//! containment.
//!
//! HERMETICITY: `bind_with_data_home` injects the daemon's data root as a
//! *value* threaded through `Server`/`RpcContext` (no `CRUCIBLE_HOME` env
//! mutation), and every data-root-aware handler reads that value —
//! `handle_session_list`, `handle_kiln_list`, the archive sweep, and
//! `handle_session_create`. Nothing here touches the developer's real
//! `~/.crucible`.
//!
//! No test in this file is `#[ignore]`d: they run in `just test ci`. The
//! 22-line header that used to explain why they all were described the
//! `crucible_home()` fallback bug, which was fixed and the attributes removed
//! without anyone updating the prose.

use anyhow::Result;
use crucible_daemon::rpc_client::SessionCreateParams;
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

struct TestServer {
    _temp_dir: TempDir,
    /// The isolated data root injected into the daemon (the tempdir path).
    /// A kiln-less `session.create` must NOT resolve it as a kiln — it is the
    /// parent of the sessions root — but it is still where the per-session
    /// scratch workspace lands.
    data_home: PathBuf,
    socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

fn ensure_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

impl TestServer {
    async fn start() -> Result<Self> {
        ensure_crypto_provider();
        let temp_dir = tempfile::tempdir()?;
        let data_home = temp_dir.path().to_path_buf();
        let socket_path = temp_dir.path().join("daemon.sock");

        let server = Server::bind_with_data_home(&socket_path, data_home.clone()).await?;
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
            data_home,
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

/// Create a session with an empty kiln set — the tools-only path.
async fn create_kilnless_session(client: &DaemonClient) -> serde_json::Value {
    client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await
        .expect("kiln-less session_create failed")
}

#[tokio::test]
async fn kilnless_create_succeeds_and_returns_active_session() {
    let server = TestServer::start().await.expect("Failed to start server");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let created = create_kilnless_session(&client).await;

    let session_id = created["session_id"]
        .as_str()
        .expect("session_id should be a string");
    assert!(!session_id.is_empty(), "session_id must be non-empty");
    assert_eq!(
        created["type"].as_str(),
        Some("chat"),
        "kiln-less session should keep its requested type"
    );

    // The response echoes the resolved kiln set, and for a kiln-less create
    // it is empty — the data root is emphatically not substituted in.
    assert_eq!(
        created["kilns"].as_array().map(Vec::as_slice),
        Some(&[][..]),
        "a kiln-less create must attach no kiln at all"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn kilnless_session_persists_an_empty_kiln_set() {
    let server = TestServer::start().await.expect("Failed to start server");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let created = create_kilnless_session(&client).await;
    let session_id = created["session_id"].as_str().unwrap().to_string();

    // Re-read via session.get: empty on the wire must be empty on disk too.
    // The data root in particular must not have crept back in — it encloses
    // the sessions root, and an allowed root there is the transcript leak.
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session["kilns"].as_array().map(Vec::as_slice),
        Some(&[][..]),
        "a kiln-less session must persist an empty kiln set, not {}",
        server.data_home.display()
    );

    server.shutdown().await;
}

#[tokio::test]
async fn kilnless_no_workspace_gets_session_scratch_dir() {
    let server = TestServer::start().await.expect("Failed to start server");
    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let created = create_kilnless_session(&client).await;
    let session_id = created["session_id"].as_str().unwrap().to_string();

    // With no workspace provided, the session gets its own session-unique
    // scratch workspace under `<data_home>/workspaces/<session_id>` — NOT the
    // kiln path. This is the session's filesystem containment boundary.
    let session = client.session_get(&session_id).await.unwrap();
    assert!(session["kilns"].as_array().unwrap().is_empty());
    let workspace = session["workspace"].as_str().expect("workspace present");

    let expected = server.data_home.join("workspaces").join(&session_id);
    assert_eq!(
        workspace,
        expected.to_str().unwrap(),
        "kiln-less workspace should be a session-unique scratch dir under <data_home>/workspaces"
    );
    assert!(
        expected.is_dir(),
        "the scratch workspace directory should have been created"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn kilnless_session_composes_with_scope_mutations() {
    let server = TestServer::start().await.expect("Failed to start server");
    let extra_kiln = tempfile::tempdir().unwrap();

    let client = DaemonClient::connect_to(&server.socket_path)
        .await
        .expect("Failed to connect");

    let created = create_kilnless_session(&client).await;
    let session_id = created["session_id"].as_str().unwrap().to_string();

    // A kiln-less session must still accept the mid-session scope RPCs: connect
    // an extra kiln, then disconnect it, round-tripping back to empty.
    let scope = client
        .session_connect_kiln(&session_id, extra_kiln.path())
        .await
        .expect("connect_kiln on a kiln-less session failed");
    let attached = scope["kilns"].as_array().unwrap();
    assert!(
        attached
            .iter()
            .any(|k| k.as_str() == Some(extra_kiln.path().to_string_lossy().as_ref())),
        "extra kiln should be attached: {attached:?}"
    );

    let scope = client
        .session_disconnect_kiln(&session_id, extra_kiln.path())
        .await
        .expect("disconnect_kiln on a kiln-less session failed");
    assert!(
        !scope["kilns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k.as_str() == Some(extra_kiln.path().to_string_lossy().as_ref())),
        "the extra kiln should be gone after disconnect: {:?}",
        scope["kilns"]
    );

    // Persisted: attach then detach round-trips back to the empty set the
    // session was created with, rather than leaving a substituted default
    // behind.
    let session = client.session_get(&session_id).await.unwrap();
    assert_eq!(
        session["kilns"].as_array().map(Vec::as_slice),
        Some(&[][..]),
        "scope mutations must round-trip back to the empty kiln set"
    );

    server.shutdown().await;
}
