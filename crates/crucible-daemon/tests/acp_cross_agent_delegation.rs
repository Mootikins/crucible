//! Delegation from one agent to an ACP agent running in its own process.
//!
//! `delegation_integration.rs` covers every policy rule the scheduler
//! enforces, but each of its children is a `MockSubagentHandle` installed by
//! an `AgentFactoryOverride`. The real factory never runs, so nothing there
//! proves that a delegation target which resolves to an *ACP profile* can be
//! spawned, handshaken and driven at all — the branch at
//! `agent_factory.rs`'s `agent_type == "acp"` is not on the path those tests
//! take.
//!
//! `acp_delegation_e2e.rs` covers the other half: an ACP agent calling the
//! delegation MCP tool. Its spawner is canned, so no child is ever built.
//!
//! These tests set no factory override. The child is a real
//! `AcpAgentHandle`, which spawns `mock-acp-agent`, completes the ACP
//! handshake over that process's stdio, and streams a turn back. What
//! crosses here and nowhere else: the scheduler's target resolution ->
//! `SessionAgent::from_profile` -> `create_agent_from_session_config`'s ACP
//! branch -> a second OS process -> the delegation result the parent reads.
//!
//! The final test makes both ends ACP — a Claude-shaped parent handing work
//! to a Codex-shaped child is the shape a user asks for, and it exercises
//! two concurrent agent processes plus the self-delegation guard's
//! profile-name arm.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crucible_core::background::JobStatus;
use crucible_core::config::{AgentProfile, BackendType, DelegationConfig};
use crucible_core::session::{SessionAgent, SessionType};
use crucible_daemon::daemon_plugins::DaemonPluginLoader;
use crucible_daemon::delegation::{DelegationRequest, DelegationService, DelegationSpawner};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::session_lifecycle::SessionLifecycle;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::{AgentManager, AgentManagerParams, FileSessionStorage, SessionManager};
use crucible_lua::PluginSource;
use tempfile::TempDir;
use tokio::sync::broadcast;

#[path = "acp_support/mock_agent.rs"]
mod mock_agent;
#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent::{MockScript, Step};
use mock_agent_bin::{acp_manager_params, mock_profile, profile_session_agent};

/// A spawn, a handshake and a turn against a second process. Generous, so a
/// loaded CI box does not turn a pass into a flake.
const DELEGATION_TIMEOUT: Duration = Duration::from_secs(90);

/// What the ACP child streams. Distinct from anything the parent could say,
/// so the assertion cannot pass on the parent's own output.
const CHILD_ANSWER: &str = "answered by the acp child process";

/// What a second, differently-named ACP profile streams.
const SECOND_CHILD_ANSWER: &str = "answered by the second acp profile";

/// An ACP profile that runs the mock agent binary and streams `answer`.
fn mock_acp_profile(answer: &str) -> AgentProfile {
    let script = MockScript {
        turn: vec![Step::Text(answer.to_string())],
        ..MockScript::default()
    };
    mock_profile(BTreeMap::from([script.env()]))
}

fn delegation_config(max_depth: u32) -> DelegationConfig {
    DelegationConfig {
        enabled: true,
        max_depth,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
        timeout_secs: 300,
    }
}

/// An internal parent, the shape `delegation_integration.rs` uses.
fn internal_parent(delegation: DelegationConfig) -> SessionAgent {
    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: Some("parent-agent".to_string()),
        provider_key: Some("ollama".to_string()),
        provider: BackendType::Ollama,
        model: "llama3.2".to_string(),
        system_prompt: "test".to_string(),
        max_context_tokens: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: Some(delegation),
        precognition_enabled: false,
        context_budget: None,
        context_strategy: Default::default(),
        tool_policy: None,
        mode: None,
    }
}

/// An ACP parent that names one of the configured profiles. Delegation is
/// on, so this session can hand work to the *other* profile.
fn acp_parent(profile_name: &str, delegation: DelegationConfig) -> SessionAgent {
    SessionAgent {
        delegation_config: Some(delegation),
        ..profile_session_agent(profile_name)
    }
}

struct Harness {
    _temp: TempDir,
    session_manager: Arc<SessionManager>,
    service: Arc<DelegationService>,
    parent_id: String,
    /// Held so the delegation lifecycle keeps working; dropping it would
    /// unbind the child-session cleanup the service relies on.
    lifecycle: Arc<SessionLifecycle>,
    _agent_manager: Arc<AgentManager>,
    _event_rx: broadcast::Receiver<SessionEventMessage>,
}

/// A manager whose ACP profiles are `profiles`, with **no** agent factory
/// override — the delegated child is built by the production factory and is
/// therefore a real ACP handle over a real process.
async fn setup(parent: SessionAgent, profiles: &[(&str, &str)]) -> Harness {
    setup_with_plugin(parent, profiles, None).await
}

/// `setup` with an optional one-file Lua plugin. The plugin runs in a real
/// `DaemonPluginLoader`, so its session-start hooks fire as in production.
async fn setup_with_plugin(
    parent: SessionAgent,
    profiles: &[(&str, &str)],
    plugin_init: Option<&str>,
) -> Harness {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let session_manager = temp_session_manager_with_kilns(&[("kiln", &kiln)]);
    let (event_tx, event_rx) = crucible_daemon::EventBus::channel(256);

    let agents: BTreeMap<String, AgentProfile> = profiles
        .iter()
        .map(|(name, answer)| ((*name).to_string(), mock_acp_profile(answer)))
        .collect();

    let loader = match plugin_init {
        Some(init) => Some(load_test_plugin(temp.path(), init).await),
        None => None,
    };
    let isolation = loader.as_ref().map(DaemonPluginLoader::isolation);
    let plugin_loader = Arc::new(tokio::sync::Mutex::new(loader));
    let lifecycle = SessionLifecycle::new(
        session_manager.clone(),
        plugin_loader.clone(),
        event_tx.clone(),
    );
    let service = DelegationService::new(session_manager.clone(), event_tx.clone());
    let agent_manager = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            plugin_loader: Some(plugin_loader),
            ..acp_manager_params(session_manager.clone(), agents, &event_tx)
        },
        service.clone(),
    ));
    if let Some(registry) = isolation {
        agent_manager.set_isolation(registry);
    }
    service.bind_agent_manager(&agent_manager);
    lifecycle.bind_agent_manager(&agent_manager);
    service.bind_session_lifecycle(lifecycle.clone());

    let session = session_manager
        .create_session(SessionType::Chat, vec![kiln_name("kiln")], None, None)
        .await
        .expect("parent session");
    agent_manager
        .configure_agent(&session.id, parent)
        .await
        .expect("configure parent agent");

    Harness {
        _temp: temp,
        session_manager,
        service,
        parent_id: session.id.to_string(),
        lifecycle,
        _agent_manager: agent_manager,
        _event_rx: event_rx,
    }
}

async fn load_test_plugin(temp: &Path, init: &str) -> DaemonPluginLoader {
    let root = temp.join("plugins");
    let dir = root.join("sandbox");
    std::fs::create_dir_all(&dir).expect("plugin dir");
    std::fs::write(dir.join("init.lua"), init).expect("init.lua");

    let mut loader = DaemonPluginLoader::new(HashMap::new()).expect("loader");
    loader
        .activate_discovered(&[(root, PluginSource::EnvPath)])
        .await
        .expect("load plugins");
    loader
}

/// A plugin that claims isolation for every session but offers no way to
/// launch a process inside its sandbox: the claim has an empty `exec`.
const CLAIMS_ISOLATION_WITHOUT_EXEC: &str = r#"
cru.on_session_start(function(session)
  cru.isolation.require{ session = session.id, plugin = "sandbox" }
end, { required = true })
return { name = "sandbox", version = "0.1.0", description = "test isolation claimer" }
"#;

fn request(h: &Harness, target: Option<&str>, prompt: &str) -> DelegationRequest {
    DelegationRequest {
        parent_session_id: h.parent_id.clone(),
        prompt: prompt.to_string(),
        context: None,
        target_agent: target.map(str::to_string),
        description: Some("cross-agent delegation test".to_string()),
    }
}

async fn load_child(h: &Harness, child_session_id: &str) -> crucible_core::session::Session {
    let storage = FileSessionStorage::new(h.session_manager.sessions_root().to_path_buf());
    crucible_daemon::session_storage::SessionStorage::load(
        &storage,
        &crucible_core::session::SessionId::parse(child_session_id).expect("child session id"),
    )
    .await
    .expect("child session persisted")
}

/// The headline: a named target that resolves to an ACP profile runs as a
/// separate agent process, and the text that process streamed is the
/// delegation result the parent receives.
#[tokio::test]
async fn delegating_to_an_acp_profile_runs_a_real_agent_process() {
    let h = setup(
        internal_parent(delegation_config(1)),
        &[("mock-acp", CHILD_ANSWER)],
    )
    .await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, Some("mock-acp"), "summarise the kiln"))
        .await
        .expect("spawning an ACP child succeeds");

    let result = h
        .service
        .await_delegation(&spawned.delegation_id, DELEGATION_TIMEOUT)
        .await
        .expect("the ACP child finishes its turn");

    assert_eq!(
        result.info.status,
        JobStatus::Completed,
        "the delegation must complete, got {:?}: {:?}",
        result.info.status,
        result.output
    );
    assert_eq!(
        result.output.as_deref().map(str::trim),
        Some(CHILD_ANSWER),
        "the parent must receive what the ACP agent process streamed"
    );
}

/// The child is a persisted, parent-linked session whose agent is the ACP
/// profile — not a clone of the internal parent. A resolution bug that fell
/// back to `parent_agent.clone()` would still produce output, and only this
/// assertion would notice.
#[tokio::test]
async fn an_acp_child_session_is_parent_linked_and_keeps_the_acp_agent_type() {
    let h = setup(
        internal_parent(delegation_config(1)),
        &[("mock-acp", CHILD_ANSWER)],
    )
    .await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, Some("mock-acp"), "task"))
        .await
        .expect("spawn");
    let _ = h
        .service
        .await_delegation(&spawned.delegation_id, DELEGATION_TIMEOUT)
        .await
        .expect("await");

    let child = load_child(&h, &spawned.child_session_id).await;
    assert_eq!(
        child.parent_session_id.as_deref(),
        Some(h.parent_id.as_str())
    );
    let agent = child.agent.as_ref().expect("the child has an agent");
    assert_eq!(
        agent.agent_type, "acp",
        "the child must be an ACP session, not a copy of the internal parent"
    );
    assert_eq!(agent.agent_name.as_deref(), Some("mock-acp"));
}

/// Two ACP profiles, both real processes: the parent is one, the child is
/// the other. The answers differ, so the assertion identifies which process
/// produced the result.
#[tokio::test]
async fn an_acp_parent_delegates_to_a_different_acp_profile() {
    let h = setup(
        acp_parent("first-acp", delegation_config(1)),
        &[
            ("first-acp", CHILD_ANSWER),
            ("second-acp", SECOND_CHILD_ANSWER),
        ],
    )
    .await;

    let spawned = h
        .service
        .spawn_delegation(request(&h, Some("second-acp"), "task"))
        .await
        .expect("an ACP parent can delegate to another ACP profile");

    let result = h
        .service
        .await_delegation(&spawned.delegation_id, DELEGATION_TIMEOUT)
        .await
        .expect("await");

    assert_eq!(
        result.info.status,
        JobStatus::Completed,
        "got {:?}: {:?}",
        result.info.status,
        result.output
    );
    assert_eq!(
        result.output.as_deref().map(str::trim),
        Some(SECOND_CHILD_ANSWER),
        "the result must come from the target profile, not the parent's own"
    );

    let child = load_child(&h, &spawned.child_session_id).await;
    assert_eq!(
        child.agent.as_ref().and_then(|a| a.agent_name.as_deref()),
        Some("second-acp")
    );
}

/// An ACP session may not delegate to the profile it is already running.
/// The guard reads `agent_name`, which for an ACP session is the profile
/// name, so this is the arm no internal-parent test can reach.
#[tokio::test]
async fn an_acp_session_may_not_delegate_to_its_own_profile() {
    let h = setup(
        acp_parent("first-acp", delegation_config(1)),
        &[
            ("first-acp", CHILD_ANSWER),
            ("second-acp", SECOND_CHILD_ANSWER),
        ],
    )
    .await;

    let error = h
        .service
        .spawn_delegation(request(&h, Some("first-acp"), "task"))
        .await
        .expect_err("delegating to the running profile must be rejected");

    assert!(
        error.to_string().contains("self-delegation"),
        "the rejection must name the guard, got: {error}"
    );
}

/// An unknown target names the ACP profiles that are configured, so a user
/// who mistypes one can see the real list.
#[tokio::test]
async fn an_unknown_target_lists_the_configured_acp_profiles() {
    let h = setup(
        internal_parent(delegation_config(1)),
        &[("mock-acp", CHILD_ANSWER)],
    )
    .await;

    // The asked-for name shares no text with the profile name, so the
    // listing assertion below can only pass on the listing itself.
    let error = h
        .service
        .spawn_delegation(request(&h, Some("nope"), "task"))
        .await
        .expect_err("an unknown target must be rejected");

    let message = error.to_string();
    assert!(
        message.contains("nope"),
        "the rejection must name what was asked for, got: {message}"
    );
    assert!(
        message.contains("mock-acp"),
        "the rejection must list the configured profile, got: {message}"
    );
}

/// A sandboxed parent may not delegate to an ACP agent that the sandbox
/// cannot hold.
///
/// The plugin claims isolation for the child too, so the backstop "the child
/// has no claim" does not fire. The claim has no `exec`, so the ACP agent
/// would start on the host and run its own tools there. Admission must
/// refuse the child with the "claims isolation" reason.
#[tokio::test]
async fn a_sandboxed_parent_cannot_delegate_to_an_acp_agent_without_a_sandbox_exec() {
    let h = setup_with_plugin(
        internal_parent(delegation_config(1)),
        &[("mock-acp", CHILD_ANSWER)],
        Some(CLAIMS_ISOLATION_WITHOUT_EXEC),
    )
    .await;

    // The parent goes through what `session.create` does. It is internal,
    // so the daemon can enforce its claim.
    h.lifecycle
        .enforce_session_start(&h.parent_id)
        .await
        .expect("an internal parent under a claim is admitted");
    let registry = h
        .lifecycle
        .isolation_registry()
        .await
        .expect("plugin isolation registry");
    assert!(
        registry.get(&h.parent_id).is_some(),
        "the parent must be sandboxed or this test asserts nothing"
    );

    let error = h
        .service
        .spawn_delegation(request(&h, Some("mock-acp"), "task"))
        .await
        .expect_err("an ACP child that the sandbox cannot hold must be refused");

    let message = error.to_string();
    assert!(
        message.contains("claims isolation") && message.contains("sandbox"),
        "the refusal must name the unenforceable claim, got: {message}"
    );
}
