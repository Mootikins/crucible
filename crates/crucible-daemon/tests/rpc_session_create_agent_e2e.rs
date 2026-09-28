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

mod common;

use anyhow::Result;
use common::{InProcessDaemon, InProcessDaemonBuilder};
use crucible_core::protocol::requests::{SessionAgentSpec, SessionCreateParams};
use crucible_core::protocol::RpcMethod;
use crucible_daemon::DaemonClient;

/// Two registered kilns: `kiln`, the one a card fixture goes under
/// (`<kiln>/.crucible/agents/`, seeded after start — discovery runs per
/// create, so seeding after start is fine), and `second`, for the tests that
/// need a kiln with no card.
async fn start_server() -> Result<InProcessDaemon> {
    InProcessDaemonBuilder::new()?
        .with_kiln("kiln")
        .with_kiln("second")
        .start()
        .await
}

/// The registry name a card kiln is registered under — what a request has to
/// say to attach it.
fn card_kiln_name() -> crucible_core::config::KilnName {
    crucible_daemon::test_support::kiln_name("kiln")
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
    let server = start_server().await.expect("start server");
    write_card(&server.kiln_dir("kiln"), "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let created = client
        .call(
            RpcMethod::SessionCreate,
            serde_json::json!({
                "type": "chat",
                "kilns": [card_kiln_name()],
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

/// A session attaches a flat set of kilns, so each attached kiln is a card
/// source, not only the first one.
#[tokio::test]
async fn agent_card_resolves_a_card_from_the_second_attached_kiln() {
    let server = start_server().await.expect("start server");
    let second = server.kiln_dir("second");
    write_card(&second, "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let created = client
        .call(
            RpcMethod::SessionCreate,
            serde_json::json!({
                "type": "chat",
                "kilns": [card_kiln_name(), "second"],
                "configure_agent": true,
                "agent_card": "researcher",
            }),
        )
        .await
        .expect("a card in the second attached kiln must resolve");
    assert_eq!(created["agent_model"].as_str(), Some("llama3.2"));

    server.shutdown().await;
}

/// `crucible-web` sends `agent_name` with no `agent_type` for a card, so the
/// deprecated alias has to keep resolving cards.
#[tokio::test]
async fn agent_name_without_agent_type_still_resolves_a_card() {
    let server = start_server().await.expect("start server");
    write_card(&server.kiln_dir("kiln"), "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let created = client
        .call(
            RpcMethod::SessionCreate,
            serde_json::json!({
                "type": "chat",
                "kilns": [card_kiln_name()],
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
    let server = start_server().await.expect("start server");
    write_card(&server.kiln_dir("kiln"), "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let before = session_count(&client).await;

    let err = client
        .call(
            RpcMethod::SessionCreate,
            serde_json::json!({
                "type": "chat",
                "kilns": [card_kiln_name()],
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
    let server = start_server().await.expect("start server");
    write_card(&server.kiln_dir("kiln"), "researcher.md", RESEARCHER_CARD);
    let client = server.connect().await;

    let before = session_count(&client).await;

    let err = client
        .call(
            RpcMethod::SessionCreate,
            serde_json::json!({
                "type": "chat",
                "kilns": [card_kiln_name()],
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
    let server = start_server().await.expect("start server");
    write_card(&server.kiln_dir("kiln"), "claude.md", CLAUDE_CARD);
    let client = DaemonClient::connect_to(server.socket_path())
        .await
        .unwrap();

    let spec = SessionAgentSpec {
        agent_name: Some("claude".to_string()),
        env_overrides: [("OPENCODE_MODEL".into(), "chosen-model".into())].into(),
        ..Default::default()
    };
    let created = client
        .session_create_with_agent(base_params_in("acp", card_kiln_name()), spec)
        .await
        .expect("create with an ACP profile failed");

    let session_id = created["session_id"].as_str().unwrap();
    let session = client.session_get(session_id).await.unwrap();
    let agent = &session["agent"];
    assert_eq!(agent["agent_type"], "acp");
    // `acp_launch::build_client_config` reads exactly this field to pick the
    // command; without it the launch falls back to exec'ing the literal `acp`.
    assert_eq!(agent["agent_name"], "claude");
    assert_eq!(agent["env_overrides"]["OPENCODE_MODEL"], "chosen-model");
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

    assert_eq!(
        client.session_get(session_id).await.unwrap()["state"],
        "active"
    );

    server.shutdown().await;
}

/// The other door onto the same field: Discord configures its ACP agents after
/// create rather than at create, so `session.configure_agent` has to store an
/// ACP profile name verbatim.
#[tokio::test]
async fn configure_agent_keeps_an_acp_profile_name() {
    let server = start_server().await.expect("start server");
    write_card(&server.kiln_dir("kiln"), "claude.md", CLAUDE_CARD);
    let client = server.connect().await;

    let created = client
        .session_create(base_params_in("internal", card_kiln_name()))
        .await
        .expect("plain create failed");
    let session_id = created["session_id"].as_str().unwrap().to_string();

    // The minimal ACP agent: everything else on `SessionAgent` has a serde
    // default, and spelling only the load-bearing fields keeps the test
    // readable when the struct grows.
    client
        .call(
            RpcMethod::SessionConfigureAgent,
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
    let server = start_server().await.expect("start server");
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
    let server = start_server().await.expect("start server");
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
    let server = start_server().await.expect("start server");
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
    let server = start_server().await.expect("start server");
    let client = server.connect().await;

    // Back-compat: the plain `session_create` (no agent spec) must behave
    // exactly as before — a session is created with no agent, to be configured
    // by a later `session.configure_agent`.
    let created = client
        .session_create(base_params_in("internal", card_kiln_name()))
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

/// The daemon dials a session's `endpoint`. An endpoint that names an address
/// inside the machine's own networks makes the daemon an SSRF relay for any
/// client that forwards a remote user's request, such as `crucible-web`.
///
/// These endpoints each name an internal address in a different spelling: the
/// cloud metadata address, an RFC 1918 host, and the metadata address hidden
/// in an IPv4-mapped IPv6 literal.
const INTERNAL_ENDPOINTS: &[&str] = &[
    "http://169.254.169.254/latest/meta-data/",
    "http://10.0.0.1:11434",
    "http://[::ffff:169.254.169.254]/",
];

/// The daemon, not a client, refuses the internal endpoint. The refusal is
/// `INVALID_PARAMS` because the caller can fix it, and no session is left.
#[tokio::test]
async fn create_refuses_an_internal_endpoint_without_creating_a_session() {
    let server = start_server().await.expect("start server");
    let client = server.connect().await;

    for endpoint in INTERNAL_ENDPOINTS {
        let before = session_count(&client).await;
        let spec = SessionAgentSpec {
            provider: Some("openai".to_string()),
            model: Some("gpt-4o".to_string()),
            endpoint: Some((*endpoint).to_string()),
            ..Default::default()
        };
        let err = client
            .session_create_with_agent(base_params("internal"), spec)
            .await
            .expect_err("an internal endpoint must refuse the create");
        let message = err.to_string();
        assert!(
            message.contains("-32602"),
            "{endpoint}: the refusal must be INVALID_PARAMS, got: {message}"
        );
        assert!(
            message.contains("internal address"),
            "{endpoint}: the refusal must say why, got: {message}"
        );
        assert_eq!(
            before,
            session_count(&client).await,
            "{endpoint}: a refused create must not leave a session"
        );
    }

    server.shutdown().await;
}

/// `session.configure_agent` is the other door an endpoint comes through. The
/// TUI's `cru session configure --endpoint` and a Lua plugin both use it.
#[tokio::test]
async fn configure_agent_refuses_an_internal_endpoint() {
    let server = start_server().await.expect("start server");
    let client = server.connect().await;

    let created = client
        .session_create(base_params("internal"))
        .await
        .expect("plain create failed");
    let session_id = created["session_id"].as_str().unwrap().to_string();

    for endpoint in INTERNAL_ENDPOINTS {
        let err = client
            .call(
                RpcMethod::SessionConfigureAgent,
                serde_json::json!({
                    "session_id": session_id,
                    "agent": {
                        "agent_type": "internal",
                        "provider": "openai",
                        "model": "gpt-4o",
                        "system_prompt": "",
                        "endpoint": endpoint,
                    },
                }),
            )
            .await
            .expect_err("an internal endpoint must refuse the configure");
        let message = err.to_string();
        assert!(
            message.contains("-32602") && message.contains("internal address"),
            "{endpoint}: got: {message}"
        );
    }
    let session = client.session_get(&session_id).await.unwrap();
    assert!(
        session["agent"].is_null(),
        "a refused configure must not store the agent, got: {}",
        session["agent"]
    );

    server.shutdown().await;
}

/// A local Ollama is the main local-model use. Its default endpoint is on
/// loopback, but the daemon dials it with no request at all, so a request
/// that names it gets no new reach. A loopback port that nothing configures
/// is refused.
#[tokio::test]
async fn the_default_ollama_endpoint_is_accepted_but_an_unconfigured_loopback_port_is_not() {
    let server = start_server().await.expect("start server");
    let client = server.connect().await;

    let spec = SessionAgentSpec {
        provider: Some("ollama".to_string()),
        model: Some("llama3.2".to_string()),
        endpoint: Some("http://localhost:11434".to_string()),
        ..Default::default()
    };
    let created = client
        .session_create_with_agent(base_params("internal"), spec)
        .await
        .expect("the default Ollama endpoint must be accepted");
    let session_id = created["session_id"].as_str().unwrap();
    let session = client.session_get(session_id).await.unwrap();
    assert_eq!(session["agent"]["endpoint"], "http://localhost:11434");

    let spec = SessionAgentSpec {
        provider: Some("openai".to_string()),
        model: Some("gpt-4o".to_string()),
        endpoint: Some("http://127.0.0.1:8081".to_string()),
        ..Default::default()
    };
    let err = client
        .session_create_with_agent(base_params("internal"), spec)
        .await
        .expect_err("an unconfigured loopback endpoint must refuse the create");
    assert!(err.to_string().contains("-32602"), "got: {err}");

    server.shutdown().await;
}
