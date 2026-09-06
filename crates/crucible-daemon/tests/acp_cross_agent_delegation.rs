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
use crucible_core::config::{AcpConfig, AgentProfile, BackendType, DelegationConfig};
use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
use crucible_daemon::delegation::{DelegationRequest, DelegationService, DelegationSpawner};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::session_lifecycle::SessionLifecycle;
use crucible_daemon::test_support::{kiln_name, temp_session_manager_with_kilns};
use crucible_daemon::{
    AgentManager, AgentManagerParams, FileSessionStorage, KilnManager, SessionManager,
};
use tempfile::TempDir;
use tokio::sync::broadcast;

#[path = "acp_support/mock_agent_bin.rs"]
mod mock_agent_bin;
use mock_agent_bin::mock_agent_path;

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
    let mut env = BTreeMap::new();
    env.insert("CRU_MOCK_STREAM_CHUNKS".to_string(), answer.to_string());
    AgentProfile {
        extends: None,
        command: Some(mock_agent_path().to_string_lossy().into_owned()),
        args: Some(Vec::new()),
        env,
        description: Some("mock ACP agent for delegation tests".to_string()),
        delegation: None,
        permissions: None,
    }
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
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: Some(delegation),
        precognition_enabled: false,
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        tool_policy: None,
        mode: None,
    }
}

/// An ACP parent that names one of the configured profiles. Delegation is
/// on, so this session can hand work to the *other* profile.
fn acp_parent(profile_name: &str, delegation: DelegationConfig) -> SessionAgent {
    let mut agent = internal_parent(delegation);
    agent.agent_type = "acp".to_string();
    agent.agent_name = Some(profile_name.to_string());
    agent.provider = BackendType::Custom;
    agent.provider_key = None;
    agent.model = profile_name.to_string();
    agent
}

struct Harness {
    _temp: TempDir,
    session_manager: Arc<SessionManager>,
    service: Arc<DelegationService>,
    parent_id: String,
    /// Held so the delegation lifecycle keeps working; dropping it would
    /// unbind the child-session cleanup the service relies on.
    _lifecycle: Arc<SessionLifecycle>,
    _agent_manager: Arc<AgentManager>,
    _event_rx: broadcast::Receiver<SessionEventMessage>,
}

/// A manager whose ACP profiles are `profiles`, with **no** agent factory
/// override — the delegated child is built by the production factory and is
/// therefore a real ACP handle over a real process.
async fn setup(parent: SessionAgent, profiles: &[(&str, &str)]) -> Harness {
    let temp = TempDir::new().expect("temp dir");
    let kiln = temp.path().join("kiln");
    std::fs::create_dir_all(&kiln).expect("kiln dir");
    let session_manager = temp_session_manager_with_kilns(&[("kiln", &kiln)]);
    let (event_tx, event_rx) = broadcast::channel(256);

    let mut agents = BTreeMap::new();
    for (name, answer) in profiles {
        agents.insert((*name).to_string(), mock_acp_profile(answer));
    }
    let acp_config = AcpConfig {
        default_agent: None,
        streaming_timeout_minutes: 1,
        agents,
    };

    let plugin_loader = Arc::new(tokio::sync::Mutex::new(None));
    let lifecycle = SessionLifecycle::new(session_manager.clone(), plugin_loader.clone());
    let service = DelegationService::new(session_manager.clone(), event_tx.clone());
    let agent_manager = Arc::new(AgentManager::new_with_delegation(
        AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager: session_manager.clone(),
            background_manager: Arc::new(crucible_daemon::BackgroundJobManager::new(
                event_tx.clone(),
            )),
            mcp_gateway: None,
            llm_config: None,
            acp_config: Some(acp_config),
            context_config: None,
            permission_config: None,
            plugin_loader: Some(plugin_loader),
            card_roots: Default::default(),
        },
        service.clone(),
    ));
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
        _lifecycle: lifecycle,
        _agent_manager: agent_manager,
        _event_rx: event_rx,
    }
}

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

    let error = h
        .service
        .spawn_delegation(request(&h, Some("mock-acp-typo"), "task"))
        .await
        .expect_err("an unknown target must be rejected");

    let message = error.to_string();
    assert!(
        message.contains("mock-acp-typo"),
        "the rejection must name what was asked for, got: {message}"
    );
    assert!(
        message.contains("mock-acp"),
        "the rejection must list the configured profile, got: {message}"
    );
}

/// A path the mock agent binary must exist at, or every test above fails
/// with a spawn error that says nothing useful. Fails fast and explains.
#[test]
fn the_mock_agent_binary_is_available() {
    let path = mock_agent_path();
    assert!(
        path.exists(),
        "mock-acp-agent is missing at {}; build it with \
         `cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent`",
        path.display()
    );
    assert!(Path::new(&path).is_file());
}
