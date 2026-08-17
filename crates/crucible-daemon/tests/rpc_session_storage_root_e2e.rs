//! Where a session's files land, end to end.
//!
//! Sessions used to be written into `{kiln}/.crucible/sessions/{id}`, so a
//! filing decision rode along with a knowledge decision: sharing a reference
//! kiln shipped every conversation ever held in it. They now go to
//! `{data_home}/sessions/{id}` and nothing is written inside the kiln.
//!
//! HERMETICITY: the data root is injected as a *value* through
//! `Server::bind_with_data_home` — no `CRUCIBLE_HOME`, no `std::env::set_var`,
//! which would race the rest of the suite. That injection is also what these
//! assertions are *for*: if any write path still reached the process-global
//! `crucible_home()`, the session would appear under the developer's real
//! `~/.crucible` and the exact-path assertions below would miss.

use anyhow::Result;
use crucible_daemon::rpc_client::SessionCreateParams;
use crucible_daemon::{DaemonClient, Server};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

struct TestServer {
    _temp_dir: TempDir,
    data_home: PathBuf,
    kiln_a: PathBuf,
    kiln_b: PathBuf,
    socket_path: PathBuf,
    _server_handle: JoinHandle<()>,
    shutdown_handle: tokio::sync::broadcast::Sender<()>,
}

impl TestServer {
    async fn start() -> Result<Self> {
        // Fixtures that reach provider listing need a default crypto provider
        // installed; installing twice is a no-op error we ignore.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let temp_dir = tempfile::tempdir()?;
        let data_home = temp_dir.path().to_path_buf();
        let socket_path = temp_dir.path().join("daemon.sock");

        // Two registered kilns, both outside the data root: the fixture asserts
        // that a session's files land under the data root and NOT inside its
        // kiln, so the kilns have to be real directories the daemon knows by
        // name.
        let kiln_a = temp_dir.path().join("kilns").join("kiln-a");
        let kiln_b = temp_dir.path().join("kilns").join("kiln-b");
        std::fs::create_dir_all(&kiln_a)?;
        std::fs::create_dir_all(&kiln_b)?;
        let server = Server::bind_with_data_home_and_kilns(
            &socket_path,
            data_home.clone(),
            &[("kiln-a", &kiln_a), ("kiln-b", &kiln_b)],
        )
        .await?;
        let shutdown_handle = server.shutdown_handle();
        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Poll for readiness: `bind` creates the socket file well before the
        // daemon accepts on it, so waiting for the path is not the same test.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while tokio::net::UnixStream::connect(&socket_path).await.is_err() {
            if tokio::time::Instant::now() >= deadline {
                anyhow::bail!("daemon never accepted on {}", socket_path.display());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(Self {
            _temp_dir: temp_dir,
            data_home,
            kiln_a,
            kiln_b,
            socket_path,
            _server_handle: server_handle,
            shutdown_handle,
        })
    }

    fn sessions_root(&self) -> PathBuf {
        self.data_home.join("sessions")
    }

    fn kiln_a(&self) -> &Path {
        &self.kiln_a
    }

    fn kiln_b(&self) -> &Path {
        &self.kiln_b
    }

    async fn shutdown(self) {
        let _ = self.shutdown_handle.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Every path under `root`, relative to it — for asserting on what a directory
/// does *not* contain, which is the half a "the file is where I expect" test
/// leaves out.
fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_path_buf());
            }
        }
    }
    out.sort();
    out
}

/// No transcript tree and no trace of `session_id` anywhere under the kiln.
fn assert_kiln_holds_no_sessions(kiln: &Path, session_id: &str) {
    let contents = walk(kiln);
    assert!(
        !contents
            .iter()
            .any(|p| p.components().any(|c| c.as_os_str() == "sessions")),
        "a sessions tree survives inside the kiln: {contents:?}"
    );
    assert!(
        !contents
            .iter()
            .any(|p| p.to_string_lossy().contains(session_id)),
        "session {session_id} left a file inside the kiln: {contents:?}"
    );
}

#[tokio::test]
async fn a_session_is_stored_under_the_injected_data_home_and_never_in_its_kiln() -> Result<()> {
    let server = TestServer::start().await?;
    let client = DaemonClient::connect_to(&server.socket_path).await?;

    let created = client
        .session_create(SessionCreateParams {
            session_type: "chat".to_string(),
            kilns: vec![crucible_daemon::test_support::kiln_name("kiln-a")],
            workspace: None,
            recording_mode: None,
            recording_path: None,
            agent_type: None,
            isolation: None,
        })
        .await?;
    let session_id = created["session_id"]
        .as_str()
        .expect("session_id")
        .to_string();

    let meta = server.sessions_root().join(&session_id).join("meta.json");
    assert!(
        meta.exists(),
        "meta.json must be at {} (sessions root {:?})",
        meta.display(),
        walk(&server.data_home)
    );

    // Nothing session-shaped inside the kiln. Opening a kiln still writes its
    // own knowledge index there (`.crucible/crucible-sqlite.db`) — that is kiln
    // data and belongs to the kiln. What must be gone is the transcript tree:
    // a kiln has to be shareable without shipping conversations.
    assert_kiln_holds_no_sessions(server.kiln_a(), &session_id);

    server.shutdown().await;
    Ok(())
}

/// Two sessions in *different* kilns share one storage root, which is the
/// property that lets §4.6 scope by kiln-set overlap instead of by directory.
#[tokio::test]
async fn sessions_from_different_kilns_share_one_storage_root() -> Result<()> {
    let server = TestServer::start().await?;
    let client = DaemonClient::connect_to(&server.socket_path).await?;

    let mut ids = Vec::new();
    for kiln in ["kiln-a", "kiln-b"] {
        let created = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kilns: vec![crucible_daemon::test_support::kiln_name(kiln)],
                workspace: None,
                recording_mode: None,
                recording_path: None,
                agent_type: None,
                isolation: None,
            })
            .await?;
        ids.push(
            created["session_id"]
                .as_str()
                .expect("session_id")
                .to_string(),
        );
    }

    for id in &ids {
        assert!(
            server.sessions_root().join(id).join("meta.json").exists(),
            "session {id} is missing from the shared root: {:?}",
            walk(&server.sessions_root())
        );
    }
    for (kiln, id) in [(server.kiln_a(), &ids[0]), (server.kiln_b(), &ids[1])] {
        assert_kiln_holds_no_sessions(kiln, id);
    }

    server.shutdown().await;
    Ok(())
}
