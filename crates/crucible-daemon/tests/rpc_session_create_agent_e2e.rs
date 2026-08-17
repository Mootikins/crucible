//! End-to-end tests for daemon-owned default-agent resolution in
//! `session.create`.
//!
//! The daemon (not each client) resolves what a new session's agent should be:
//! callers pass an optional agent spec and the daemon resolves the ACP profile
//! or builds config-derived internal defaults, then configures the session's
//! agent as part of create. These tests pin that contract:
//!   * an internal spec configures the agent and the response carries the model;
//!   * caller-supplied provider/model overrides win over config defaults;
//!   * `agent_card` layers a kiln agent card over those defaults, and
//!     `agent_name` still does the same on an internal session (the deprecated
//!     alias `crucible-web` sends), but both at once is refused;
//!   * on an ACP session `agent_name` still means an ACP profile — the alias is
//!     internal-branch-only, so a card cannot shadow a profile and a
//!     `session.configure_agent` round trip keeps the name `acp_launch` needs;
//!   * an unknown ACP profile or agent card fails without creating a session;
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

        let kiln = data_home.join("kiln");
        std::fs::create_dir_all(&kiln)?;
        let server =
            Server::bind_with_data_home_and_kilns(&socket_path, data_home, &[("kiln", &kiln)])
                .await?;
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
            socket_path,
            _server_handle: server_handle,
            shutdown_handle,
        })
    }

    /// A kiln inside the injected data root: `<kiln>/.crucible/agents/` is
    /// where a card fixture goes, and a session sees it only by ATTACHING the
    /// kiln. A kiln-less session has no kiln card directory to read at all —
    /// that is §4.1's "an empty kiln set degrades capabilities", and it is why
    /// these tests name the kiln instead of leaning on a data-root fallback.
    /// Discovery runs per create, so seeding after start is fine.
    fn card_kiln(&self) -> PathBuf {
        self._temp_dir.path().join("kiln")
    }

    /// The registry name `card_kiln` is registered under — what a request has
    /// to say to attach it.
    fn card_kiln_name(&self) -> crucible_core::config::KilnName {
        crucible_core::config::KilnName::parse("kiln").expect("a valid kiln name")
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
        kilns: vec![],
        workspace: None,
        recording_mode: None,
        recording_path: None,
        agent_type: Some(agent_type.to_string()),
        isolation: None,
    }
}

/// [`base_params`] with the card kiln attached — for the tests whose fixture
/// card must actually be discoverable.
fn base_params_in(agent_type: &str, kiln: crucible_core::config::KilnName) -> SessionCreateParams {
    SessionCreateParams {
        kilns: vec![kiln],
        ..base_params(agent_type)
    }
}

async fn session_count(client: &DaemonClient) -> usize {
    let result = client
        .session_list(None, None, None, None, Some(true))
        .await
        .expect("session.list failed");
    result["sessions"].as_array().map(|a| a.len()).unwrap_or(0)
}

/// Write an agent card into the kiln's `.crucible/agents/`.
fn write_card(kiln: &std::path::Path, file: &str, body: &str) {
    let dir = kiln.join(".crucible").join("agents");
    std::fs::create_dir_all(&dir).expect("create card dir");
    std::fs::write(dir.join(file), body).expect("write card");
}

const RESEARCHER_CARD: &str =
    "---\nname: researcher\ndescription: Explores and synthesizes\nmodel: llama3.2\n---\n\nYou are a researcher.\n";

/// A card deliberately named after a built-in ACP profile, to prove the two
/// namespaces do not bleed into each other.
const CLAUDE_CARD: &str =
    "---\nname: claude\ndescription: A card, not a profile\nmodel: llama3.2\n---\n\nI am the card, not the subprocess.\n";

#[tokio::test]
async fn agent_card_resolves_a_kiln_card_onto_the_internal_defaults() {
    let server = TestServer::start().await.expect("start server");
    write_card(&server.card_kiln(), "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let created = client
        .call(
            "session.create",
            serde_json::json!({
                "type": "chat",
                "kilns": [server.card_kiln_name()],
                "configure_agent": true,
                "agent_card": "researcher",
            }),
        )
        .await
        .expect("create with agent_card failed");

    assert_eq!(created["agent_model"].as_str(), Some("llama3.2"));

    let session_id = created["session_id"].as_str().unwrap();
    let session = client.session_get(session_id).await.unwrap();
    let agent = &session["agent"];
    assert_eq!(agent["agent_type"], "internal");
    assert_eq!(agent["agent_card_name"], "researcher");
    // A card is an internal agent, never an ACP profile: `agent_name` must stay
    // clear, because a set `agent_name` is what forces `TrustLevel::Cloud` at
    // runtime (`trust_resolution.rs`).
    assert!(
        agent["agent_name"].is_null(),
        "a card must not set agent_name, got: {}",
        agent["agent_name"]
    );
    assert_eq!(agent["system_prompt"], "You are a researcher.");

    server.shutdown().await;
}

/// `crucible-web` sends `agent_name` with no `agent_type` for a card, so the
/// deprecated alias has to keep resolving cards.
#[tokio::test]
async fn agent_name_without_agent_type_still_resolves_a_card() {
    let server = TestServer::start().await.expect("start server");
    write_card(&server.card_kiln(), "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let created = client
        .call(
            "session.create",
            serde_json::json!({
                "type": "chat",
                "kilns": [server.card_kiln_name()],
                "configure_agent": true,
                "agent_name": "researcher",
            }),
        )
        .await
        .expect("create with legacy agent_name failed");

    let session_id = created["session_id"].as_str().unwrap();
    let session = client.session_get(session_id).await.unwrap();
    assert_eq!(session["agent"]["agent_card_name"], "researcher");

    server.shutdown().await;
}

#[tokio::test]
async fn agent_card_and_agent_name_together_are_rejected() {
    let server = TestServer::start().await.expect("start server");
    write_card(&server.card_kiln(), "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let before = session_count(&client).await;

    let err = client
        .call(
            "session.create",
            serde_json::json!({
                "type": "chat",
                "kilns": [server.card_kiln_name()],
                "configure_agent": true,
                "agent_card": "researcher",
                "agent_name": "researcher",
            }),
        )
        .await
        .expect_err("both agent fields set must fail the create");
    assert!(
        err.to_string().contains("mutually exclusive"),
        "error should say the two fields conflict, got: {err}"
    );

    assert_eq!(
        before,
        session_count(&client).await,
        "a rejected create must not leave an orphaned session"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn unknown_agent_card_errors_without_creating_a_session() {
    let server = TestServer::start().await.expect("start server");
    write_card(&server.card_kiln(), "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let before = session_count(&client).await;

    let err = client
        .call(
            "session.create",
            serde_json::json!({
                "type": "chat",
                "kilns": [server.card_kiln_name()],
                "configure_agent": true,
                "agent_card": "no-such-card",
            }),
        )
        .await
        .expect_err("unknown card must fail the create");
    let message = err.to_string();
    assert!(
        message.contains("Unknown agent card: no-such-card"),
        "error should name the unknown card, got: {message}"
    );
    // Exactly the fixture's card, nothing else. The global card directory is
    // injected (`BindWithPluginConfigParams::config_home`) rather than read
    // from the environment, so a developer's own `~/.config/crucible/agents/`
    // — which is FIRST in discovery precedence — cannot appear here.
    // The trailing quote is the end of the JSON-RPC message string, so this
    // pins the list to exactly one card.
    assert!(
        message.contains("Available cards: researcher\""),
        "only the fixture's card should be discoverable, got: {message}"
    );

    assert_eq!(
        before,
        session_count(&client).await,
        "a rejected create must not leave an orphaned session"
    );

    server.shutdown().await;
}

/// `agent_name` means "agent card" only on the internal branch. On an ACP
/// session it still means an ACP profile, so a card that happens to share a
/// built-in profile's name must not shadow it — the profile launches.
#[tokio::test]
async fn acp_agent_name_selects_a_profile_not_a_card_of_the_same_name() {
    let server = TestServer::start().await.expect("start server");
    write_card(&server.card_kiln(), "claude.md", CLAUDE_CARD);
    let client = server.connect().await;

    let spec = SessionAgentSpec {
        agent_name: Some("claude".to_string()),
        ..Default::default()
    };
    let created = client
        .session_create_with_agent(base_params_in("acp", server.card_kiln_name()), spec)
        .await
        .expect("create with an ACP profile failed");

    let session_id = created["session_id"].as_str().unwrap();
    let session = client.session_get(session_id).await.unwrap();
    let agent = &session["agent"];
    assert_eq!(agent["agent_type"], "acp");
    // `acp_launch::build_client_config` reads exactly this field to pick the
    // command; without it the launch falls back to exec'ing the literal `acp`.
    assert_eq!(agent["agent_name"], "claude");
    assert!(
        agent["agent_card_name"].is_null(),
        "the same-named card must not be consulted, got: {}",
        agent["agent_card_name"]
    );
    // `from_profile` leaves the prompt empty and `apply_session_defaults` then
    // fills in the daemon's default, so the assertion is about provenance, not
    // emptiness: whatever it is, it is not the card's.
    let prompt = agent["system_prompt"].as_str().unwrap_or_default();
    assert!(
        !prompt.contains("I am the card"),
        "the card's prompt must not leak onto an ACP agent, got: {prompt}"
    );

    server.shutdown().await;
}

/// The other door onto the same field: Discord configures its ACP agents after
/// create rather than at create, so `session.configure_agent` has to store an
/// ACP profile name verbatim.
#[tokio::test]
async fn configure_agent_keeps_an_acp_profile_name() {
    let server = TestServer::start().await.expect("start server");
    write_card(&server.card_kiln(), "claude.md", CLAUDE_CARD);
    let client = server.connect().await;

    let created = client
        .session_create(base_params_in("internal", server.card_kiln_name()))
        .await
        .expect("plain create failed");
    let session_id = created["session_id"].as_str().unwrap().to_string();

    // The minimal ACP agent: everything else on `SessionAgent` has a serde
    // default, and spelling only the load-bearing fields keeps the test
    // readable when the struct grows.
    client
        .call(
            "session.configure_agent",
            serde_json::json!({
                "session_id": session_id,
                "agent": {
                    "agent_type": "acp",
                    "agent_name": "claude",
                    "provider": "custom",
                    "model": "claude",
                    "system_prompt": "",
                },
            }),
        )
        .await
        .expect("configure_agent with an ACP profile failed");

    let session = client.session_get(&session_id).await.unwrap();
    let agent = &session["agent"];
    assert_eq!(agent["agent_type"], "acp");
    assert_eq!(agent["agent_name"], "claude");
    assert!(
        agent["agent_card_name"].is_null(),
        "configure_agent must not reinterpret an ACP name as a card, got: {}",
        agent["agent_card_name"]
    );

    server.shutdown().await;
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
        .session_create(base_params_in("internal", server.card_kiln_name()))
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
