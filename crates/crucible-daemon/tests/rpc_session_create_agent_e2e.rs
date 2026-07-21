//! End-to-end tests for daemon-owned default-agent resolution in
//! `session.create`.
//!
//! The daemon (not each client) resolves what a new session's agent should be:
//! callers pass an optional agent spec and the daemon resolves the ACP profile
//! or builds config-derived internal defaults, then configures the session's
//! agent as part of create. These tests pin that contract:
//!   * an internal spec configures the agent and the response carries the model;
//!   * caller-supplied provider/model overrides win over config defaults;
//!   * an unknown ACP profile fails without creating a session;
//!   * no spec (back-compat) leaves the session agent-less.
//!
//! Hermetic per the project rules: each server binds an isolated tempdir data
//! root via `Server::bind_with_data_home` (a value, no `CRUCIBLE_HOME` env
//! mutation) and installs the rustls crypto provider.

use anyhow::Result;
use crucible_daemon::rpc_client::{SessionAgentSpec, SessionCreateParams};
use crucible_daemon::DaemonClient;
use crucible_daemon::Server;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::task::JoinHandle;

struct TestServer {
    _temp_dir: TempDir,
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

        let server = Server::bind_with_data_home(&socket_path, data_home).await?;
        let shutdown_handle = server.shutdown_handle();

        let server_handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(Self {
            _temp_dir: temp_dir,
            socket_path,
            _server_handle: server_handle,
            shutdown_handle,
        })
    }

    async fn connect(&self) -> DaemonClient {
        DaemonClient::connect_to(&self.socket_path)
            .await
            .expect("Failed to connect")
    }

    async fn shutdown(self) {
        let _ = self.shutdown_handle.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Base params for a kiln-less internal session — the daemon resolves the kiln
/// to its (injected) data root.
fn base_params(agent_type: &str) -> SessionCreateParams {
    SessionCreateParams {
        session_type: "chat".to_string(),
        kiln: None,
        workspace: None,
        connect_kilns: vec![],
        recording_mode: None,
        recording_path: None,
        agent_type: Some(agent_type.to_string()),
    }
}

async fn session_count(client: &DaemonClient) -> usize {
    let result = client
        .session_list(None, None, None, None, Some(true))
        .await
        .expect("session.list failed");
    result["sessions"].as_array().map(|a| a.len()).unwrap_or(0)
}

#[tokio::test]
async fn internal_spec_configures_agent_with_config_defaults() {
    let server = TestServer::start().await.expect("start server");
    let client = server.connect().await;

    // An internal spec with no overrides ⇒ config-derived defaults. With no
    // provider configured in the isolated data root, that is the built-in
    // Ollama / default-model fallback.
    let created = client
        .session_create_with_agent(base_params("internal"), SessionAgentSpec::default())
        .await
        .expect("create with internal spec failed");

    let model = created["agent_model"]
        .as_str()
        .expect("create response must carry agent_model");
    assert!(!model.is_empty(), "resolved model must be non-empty");

    // session.get reflects the daemon-configured agent.
    let session_id = created["session_id"].as_str().unwrap();
    let session = client.session_get(session_id).await.unwrap();
    let agent = &session["agent"];
    assert!(
        agent.is_object(),
        "agent should be configured as part of create, got: {agent}"
    );
    assert_eq!(agent["agent_type"], "internal");
    assert_eq!(
        agent["model"].as_str(),
        Some(model),
        "session.get model must match the create response"
    );
    assert!(
        agent["provider_key"].is_string(),
        "internal default must set a provider_key"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn internal_spec_applies_provider_and_model_overrides() {
    let server = TestServer::start().await.expect("start server");
    let client = server.connect().await;

    let spec = SessionAgentSpec {
        provider: Some("anthropic".to_string()),
        model: Some("claude-sonnet-5".to_string()),
        endpoint: Some("https://api.anthropic.com".to_string()),
        ..Default::default()
    };
    let created = client
        .session_create_with_agent(base_params("internal"), spec)
        .await
        .expect("create with overrides failed");

    assert_eq!(created["agent_model"].as_str(), Some("claude-sonnet-5"));

    let session_id = created["session_id"].as_str().unwrap();
    let session = client.session_get(session_id).await.unwrap();
    let agent = &session["agent"];
    assert_eq!(agent["provider"], "anthropic");
    assert_eq!(agent["model"], "claude-sonnet-5");
    assert_eq!(agent["endpoint"], "https://api.anthropic.com");

    server.shutdown().await;
}

#[tokio::test]
async fn unknown_acp_profile_errors_without_creating_a_session() {
    let server = TestServer::start().await.expect("start server");
    let client = server.connect().await;

    let before = session_count(&client).await;

    let spec = SessionAgentSpec {
        agent_name: Some("no-such-agent-xyz".to_string()),
        ..Default::default()
    };
    let err = client
        .session_create_with_agent(base_params("acp"), spec)
        .await
        .expect_err("unknown ACP profile must fail the create");
    assert!(
        err.to_string().contains("Unknown ACP agent profile"),
        "error should name the unknown profile, got: {err}"
    );

    let after = session_count(&client).await;
    assert_eq!(
        before, after,
        "a rejected ACP create must not leave an orphaned session"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn create_without_spec_leaves_agent_unconfigured() {
    let server = TestServer::start().await.expect("start server");
    let client = server.connect().await;

    // Back-compat: the plain `session_create` (no agent spec) must behave
    // exactly as before — a session is created with no agent, to be configured
    // by a later `session.configure_agent`.
    let created = client
        .session_create(base_params("internal"))
        .await
        .expect("plain create failed");
    assert!(
        created["agent_model"].is_null(),
        "no spec ⇒ no resolved model in the response, got: {}",
        created["agent_model"]
    );

    let session_id = created["session_id"].as_str().unwrap();
    let session = client.session_get(session_id).await.unwrap();
    assert!(
        session["agent"].is_null(),
        "no spec ⇒ agent must remain unconfigured, got: {}",
        session["agent"]
    );

    server.shutdown().await;
}
