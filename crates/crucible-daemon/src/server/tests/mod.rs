use super::*;
use crate::session_storage::FileSessionStorage;
use chrono::{Duration as ChronoDuration, Utc};
use observe::*;
use serde_json::json;
use serde_json::Value;
use session::*;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

mod child_sessions;
mod delegation_e2e;
mod event_seq;
mod events;
mod graph;
mod isolation_param;
mod kiln_scope_validation;
mod lifecycle;
mod models_settings;
mod persist_event;
mod persisted_session;
mod review_watch;
mod rpc_basic;
mod session_id_boundary;
mod session_log_capture;
mod subscription;
mod truncation;
mod trust;

/// Poll until the session log has at least `n` lines. The persist task is
/// a separate tokio task, so there is no synchronous point to await; a
/// fixed sleep would be a timing guess.
pub(super) async fn wait_for_lines(path: &Path, n: usize) -> String {
    for _ in 0..100 {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            if content.lines().filter(|l| !l.trim().is_empty()).count() >= n {
                return content;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("session log at {} never reached {n} lines", path.display());
}

// These three moved to `crate::test_fixtures` once `session_bridge::tests` and
// `agent_manager::tests` wanted them too; re-exported so the ~20 call sites in
// this subtree keep their bare names.
pub(super) use crate::test_fixtures::{
    build_llm_config, build_llm_config_with_trust, test_agent_manager,
};

pub(super) fn create_session_request(kiln: &Path, workspace: &Path, provider_key: &str) -> Request {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session.create",
        "params": {
            "type": "chat",
            "kilns": [kiln],
            "workspace": workspace,
            "provider_key": provider_key
        }
    }))
    .unwrap()
}

pub(super) fn write_workspace_config(
    workspace: &Path,
    kiln_relative_path: &str,
    classification: Option<&str>,
) {
    let crucible_dir = workspace.join(".crucible");
    std::fs::create_dir_all(&crucible_dir).unwrap();
    let mut config = format!("[[kilns]]\npath = \"{}\"\n", kiln_relative_path);
    if let Some(classification) = classification {
        config.push_str(&format!("data_classification = \"{}\"\n", classification));
    }
    std::fs::write(crucible_dir.join("project.toml"), config).unwrap();
}

pub(super) async fn rpc_call(client: &mut UnixStream, request: Value) -> Value {
    let request = serde_json::to_string(&request).unwrap();
    client
        .write_all(format!("{}\n", request).as_bytes())
        .await
        .unwrap();

    let mut buf = Vec::with_capacity(8192);
    loop {
        let mut chunk = [0u8; 1024];
        let n = client.read(&mut chunk).await.unwrap();
        if n == 0 {
            break;
        }

        buf.extend_from_slice(&chunk[..n]);
        if buf.contains(&b'\n') {
            break;
        }
    }

    let end = buf.iter().position(|b| *b == b'\n').unwrap_or(buf.len());
    serde_json::from_slice(&buf[..end]).unwrap()
}

pub(super) fn extract_session_id(response: &Value) -> String {
    response["result"]["session_id"]
        .as_str()
        .expect("session.create should return session_id")
        .to_string()
}

pub(super) async fn create_chat_session(client: &mut UnixStream, kiln: &Path, id: u64) -> String {
    let response = rpc_call(
        client,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session.create",
            "params": {
                "type": "chat",
                "kilns": [kiln],
            }
        }),
    )
    .await;

    assert!(
        response["error"].is_null(),
        "session.create failed: {response:?}"
    );
    extract_session_id(&response)
}

/// Shared fixture for in-process daemon RPC integration tests.
///
/// Centralizes the ~12-line dance every test used to hand-roll: TempDir,
/// sock/kiln paths, `Server::bind_with_data_home` against the isolated
/// tempdir data home (never the real `~/.crucible`), spawning `run()`,
/// waiting for the listener to come up, and (via `shutdown()`) the teardown
/// send + task await.
pub(super) struct TestServer {
    pub tmp: TempDir,
    pub sock_path: PathBuf,
    pub kiln_path: PathBuf,
    pub event_tx: broadcast::Sender<SessionEventMessage>,
    pub kiln_manager: Arc<KilnManager>,
    shutdown_tx: broadcast::Sender<()>,
    task: tokio::task::JoinHandle<Result<()>>,
}

impl TestServer {
    /// Binds and spawns a server against a fresh tempdir data home (with a
    /// `kiln` subdirectory pre-created), then waits for it to start
    /// accepting connections.
    pub(super) async fn start() -> Self {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let kiln_path = tmp.path().join("kiln");
        std::fs::create_dir_all(&kiln_path).unwrap();

        let server = Server::bind_with_data_home(&sock_path, tmp.path().to_path_buf())
            .await
            .unwrap();
        let event_tx = server.event_sender();
        let kiln_manager = server.kiln_manager.clone();
        let shutdown_tx = server.shutdown_handle();
        let task = tokio::spawn(server.run());

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        Self {
            tmp,
            sock_path,
            kiln_path,
            event_tx,
            kiln_manager,
            shutdown_tx,
            task,
        }
    }

    /// Where this server writes sessions. Sessions live under the daemon's
    /// own data root now, not in a kiln, so tests that read a transcript off
    /// disk have to ask the server rather than compose a kiln path.
    pub(super) fn sessions_root(&self) -> PathBuf {
        FileSessionStorage::root_for(self.tmp.path())
    }

    /// Connects a new client to the running server.
    pub(super) async fn connect(&self) -> UnixStream {
        UnixStream::connect(&self.sock_path).await.unwrap()
    }

    /// Standard teardown: sends shutdown and awaits the server task.
    pub(super) async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.task.await;
    }

    /// For tests that trigger shutdown via the `shutdown` RPC method itself
    /// rather than the out-of-band handle: awaits the server task directly
    /// with a timeout, without sending on `shutdown_tx`. Returns whether the
    /// task completed within `timeout`.
    pub(super) async fn await_shutdown_within(self, timeout: std::time::Duration) -> bool {
        tokio::time::timeout(timeout, self.task).await.is_ok()
    }
}

pub(super) async fn configure_internal_mock_agent(
    client: &mut UnixStream,
    session_id: &str,
    id: u64,
    model: &str,
) -> Value {
    rpc_call(
        client,
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session.configure_agent",
            "params": {
                "session_id": session_id,
                "agent": {
                    "agent_type": "internal",
                    "provider": "mock",
                    "model": model,
                    "system_prompt": "test",
                    "provider_key": "mock"
                }
            }
        }),
    )
    .await
}
