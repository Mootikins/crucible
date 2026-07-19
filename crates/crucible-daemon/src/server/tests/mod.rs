use super::*;
use crate::session_storage::FileSessionStorage;
use chrono::{Duration as ChronoDuration, Utc};
use observe::*;
use serde_json::json;
use serde_json::Value;
use session::*;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

mod events;
mod lifecycle;
mod models_settings;
mod persist_event;
mod persisted_session;
mod rpc_basic;
mod subscription;
mod truncation;
mod trust;

pub(super) fn build_llm_config(
    default_key: &str,
    provider_type: crucible_core::config::BackendType,
) -> LlmConfig {
    build_llm_config_with_trust(default_key, provider_type, None)
}

pub(super) fn build_llm_config_with_trust(
    default_key: &str,
    provider_type: crucible_core::config::BackendType,
    trust_level: Option<crucible_core::config::TrustLevel>,
) -> LlmConfig {
    let mut providers = HashMap::new();
    providers.insert(
        default_key.to_string(),
        crucible_core::config::LlmProviderConfig {
            provider_type,
            endpoint: None,
            default_model: None,
            temperature: None,
            max_tokens: None,
            timeout_secs: None,
            api_key: None,
            available_models: None,
            trust_level,
            name: None,
        },
    );
    LlmConfig {
        default: Some(default_key.to_string()),
        providers,
    }
}

/// Build an `AgentManager` suitable for tests that don't actually drive an
/// agent — they just need a value to pass to `handle_session_create` so the
/// setup task has a handle for `list_providers`. The returned manager has no
/// MCP gateway, no ACP config, no plugin loader.
pub(super) fn test_agent_manager(
    kiln_manager: Arc<KilnManager>,
    session_manager: Arc<SessionManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
    llm_config: Option<LlmConfig>,
) -> Arc<AgentManager> {
    let background_manager = Arc::new(crate::background_manager::BackgroundJobManager::new(
        event_tx,
    ));
    // These tests never drive workspace tools — WorkspaceTools just needs a
    // path value. Use a per-process temp path rather than hardcoding /tmp.
    let workspace_tools = Arc::new(crate::tools::workspace::WorkspaceTools::new(
        std::env::temp_dir().join(format!("crucible-server-test-{}", std::process::id())),
    ));
    Arc::new(AgentManager::new(
        crate::agent_manager::AgentManagerParams {
            kiln_manager,
            session_manager,
            background_manager,
            mcp_gateway: None,
            llm_config,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools,
        },
    ))
}

pub(super) fn create_session_request(kiln: &Path, workspace: &Path, provider_key: &str) -> Request {
    serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "session.create",
        "params": {
            "type": "chat",
            "kiln": kiln,
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
                "kiln": kiln,
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
