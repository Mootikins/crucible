//! Locating and configuring the spawned `mock-acp-agent` binary.
//!
//! Shared by every test that drives a real `AcpAgentHandle` (which spawns a
//! process and speaks ACP over its stdio) rather than a `CrucibleAcpClient`
//! over an in-process duplex pipe.
//!
//! `acp_support` is `#[path]`-included by several test binaries and each uses
//! only part of it, so every helper here is dead code in at least one of them.
//! The allows are per-item rather than a module-level `#![allow(unused)]` so
//! that an unused import or variable added later is still a warning.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crucible_core::config::{AcpConfig, AgentProfile, BackendType};
use crucible_core::session::SessionAgent;
use crucible_daemon::acp_handle::AcpAgentHandleParams;
use crucible_daemon::agent_manager::{TurnOutcome, TurnStatus};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::{AgentManagerParams, BackgroundJobManager, KilnManager, SessionManager};
use tokio::sync::{broadcast, oneshot};

/// Returns the path to the mock-acp-agent binary.
///
/// Prefers `CARGO_BIN_EXE_mock-acp-agent` (set by cargo when the bin target is
/// built, i.e. when the test-utils feature is enabled — honors any custom
/// target-dir). Falls back to the default workspace-root target path.
///
/// Panics with the build command when the binary is missing, so a test that
/// needs it fails on a message that names the fix rather than on a spawn
/// error that names nothing.
#[allow(dead_code)]
pub fn mock_agent_path() -> PathBuf {
    let path = option_env!("CARGO_BIN_EXE_mock-acp-agent")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/mock-acp-agent")
        });
    assert!(
        path.is_file(),
        "mock-acp-agent is missing at {}; build it with \
         `cargo build -p crucible-daemon --features test-utils --bin mock-acp-agent`",
        path.display()
    );
    path
}

/// Creates a SessionAgent configured for ACP with the given agent path.
///
/// This helper constructs a minimal SessionAgent with:
/// - agent_type: "acp"
/// - agent_name: the provided agent_path
/// - provider: Mock (for testing)
/// - All other fields set to sensible defaults
#[allow(dead_code)]
pub fn mock_session_agent(agent_path: &str) -> SessionAgent {
    SessionAgent {
        mode: None,
        agent_type: "acp".to_string(),
        agent_name: Some(agent_path.to_string()),
        provider_key: None,
        provider: BackendType::Mock,
        model: "mock-model".to_string(),
        system_prompt: "You are a helpful assistant.".to_string(),
        max_context_tokens: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        context_budget: None,
        context_strategy: Default::default(),
        tool_policy: None,
    }
}

/// `AcpAgentHandleParams` with everything optional switched off.
///
/// Every test that drives a spawned mock agent needs the same twelve-field
/// literal with only the two required references filled in; the struct cannot
/// `derive(Default)` because those two are borrows. Override a field with
/// functional-update syntax:
///
/// ```ignore
/// AcpAgentHandle::new(AcpAgentHandleParams {
///     acp_config: Some(&acp_config),
///     ..mock_handle_params(&agent_config, workspace.path())
/// })
/// ```
/// No global cards, no configured card directories.
static NO_CARD_ROOTS: crucible_daemon::runtime_path::SourceRoots =
    crucible_daemon::runtime_path::SourceRoots {
        config_home: None,
        agent_directories: Vec::new(),
        runtimepath: Vec::new(),
    };

#[allow(dead_code)]
pub fn mock_handle_params<'a>(
    agent_config: &'a SessionAgent,
    workspace: &'a Path,
) -> AcpAgentHandleParams<'a> {
    AcpAgentHandleParams {
        agent_config,
        workspace,
        kiln_path: None,
        knowledge_repo: None,
        embedding_provider: None,
        background_spawner: None,
        delegation_spawner: None,
        parent_session_id: None,
        delegation_config: None,
        source_roots: &NO_CARD_ROOTS,
        acp_config: Some(mock_path_acp_config()),
        permission_handler: None,
        sandbox_exec: None,
        containment: crucible_daemon::tools::containment::RootSet::Ambient,
        resume_acp_session_id: None,
        event_tx: None,
    }
}

/// The ACP profile name the daemon-level tests configure.
#[allow(dead_code)]
pub const MOCK_PROFILE: &str = "mock-acp";

/// An ACP profile that runs the mock binary with `env` as its hooks.
#[allow(dead_code)]
pub fn mock_profile(env: BTreeMap<String, String>) -> AgentProfile {
    AgentProfile {
        command: Some(mock_agent_path().to_string_lossy().into_owned()),
        args: Some(Vec::new()),
        env,
        description: Some("mock ACP agent".to_string()),
        ..Default::default()
    }
}

/// The `AcpConfig` a spawned-agent test resolves its agent through.
///
/// The daemon refuses an agent name that no profile defines; it does not run
/// the name itself as a command. [`mock_session_agent`] names the mock binary
/// by its absolute path, so that path needs a profile of its own.
#[allow(dead_code)]
pub fn mock_path_acp_config() -> &'static AcpConfig {
    static CONFIG: std::sync::OnceLock<AcpConfig> = std::sync::OnceLock::new();
    CONFIG.get_or_init(|| {
        let path = mock_agent_path().to_string_lossy().into_owned();
        AcpConfig {
            agents: [(
                path.clone(),
                AgentProfile {
                    command: Some(path),
                    args: Some(Vec::new()),
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        }
    })
}

/// A session agent that names the ACP profile `profile`, so the production
/// factory resolves it through `AcpConfig` the way a real session does.
#[allow(dead_code)]
pub fn profile_session_agent(profile: &str) -> SessionAgent {
    SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some(profile.to_string()),
        provider: BackendType::Custom,
        model: profile.to_string(),
        system_prompt: String::new(),
        ..mock_session_agent(profile)
    }
}

/// `AgentManagerParams` for a daemon whose ACP profiles are `agents`, with
/// every other collaborator absent. Override a field with struct-update
/// syntax when a test needs a real one (a kiln manager, a plugin loader).
#[allow(dead_code)]
pub fn acp_manager_params(
    session_manager: Arc<SessionManager>,
    agents: BTreeMap<String, AgentProfile>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> AgentManagerParams {
    AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager,
        background_manager: Arc::new(BackgroundJobManager::new(event_tx.clone())),
        mcp_gateway: None,
        llm_config: None,
        acp_config: Some(AcpConfig {
            default_agent: None,
            streaming_timeout_minutes: 1,
            agents,
        }),
        context_config: None,
        permission_config: None,
        plugin_loader: None,
        source_roots: Default::default(),
        review_snapshot_root: crucible_daemon::test_support::scratch_snapshot_root(),
    }
}

/// Await the completion channel of `send_message_notified` and require a
/// completed turn.
///
/// A turn that failed also resolves the channel, so discarding the outcome
/// lets a test go on to assert about a turn that never ran. The panic
/// carries the turn's own error.
#[allow(dead_code)]
pub async fn completed_turn(done: oneshot::Receiver<TurnOutcome>, limit: Duration) -> TurnOutcome {
    let outcome = tokio::time::timeout(limit, done)
        .await
        .expect("the turn did not finish inside the timeout")
        .expect("the turn's completion channel closed without an outcome");
    assert_eq!(
        outcome.status,
        TurnStatus::Completed,
        "the turn must complete; error: {:?}, text: {:?}",
        outcome.error,
        outcome.final_text
    );
    outcome
}

/// A chat session whose agent is the mock binary, built through the
/// production `AgentManager` and the profile [`MOCK_PROFILE`].
#[allow(dead_code)]
pub struct MockSession {
    pub agent_manager: Arc<crucible_daemon::AgentManager>,
    pub session_id: crucible_core::session::SessionId,
    pub event_tx: broadcast::Sender<SessionEventMessage>,
    /// Every event that the session broadcasts.
    pub events: broadcast::Receiver<SessionEventMessage>,
}

/// Register `kilns` and start a chat session that attaches the first one,
/// in `workspace` if it is given. The session agent is the mock binary,
/// which runs `script`.
#[allow(dead_code)]
pub async fn mock_session(
    kilns: &[(&str, &Path)],
    workspace: Option<&Path>,
    script: super::mock_agent::MockScript,
) -> MockSession {
    mock_session_with(kilns, workspace, script, None, None).await
}

/// [`mock_session`] with the agent card `tool_policy` and the operator
/// `[permissions]` rules `permissions`.
#[allow(dead_code)]
pub async fn mock_session_with(
    kilns: &[(&str, &Path)],
    workspace: Option<&Path>,
    script: super::mock_agent::MockScript,
    tool_policy: Option<crucible_core::agent::ToolPolicyMap>,
    permissions: Option<crucible_core::config::components::permissions::PermissionConfig>,
) -> MockSession {
    let session_manager = crucible_daemon::test_support::temp_session_manager_with_kilns(kilns);
    let (event_tx, events) = broadcast::channel(256);
    let profile = mock_profile(BTreeMap::from([script.env()]));
    let agent_manager = Arc::new(crucible_daemon::AgentManager::new(AgentManagerParams {
        permission_config: permissions,
        ..acp_manager_params(
            session_manager.clone(),
            BTreeMap::from([(MOCK_PROFILE.to_string(), profile)]),
            &event_tx,
        )
    }));
    let session = session_manager
        .create_session(
            crucible_core::session::SessionType::Chat,
            vec![crucible_daemon::test_support::kiln_name(kilns[0].0)],
            workspace.map(Path::to_path_buf),
            None,
        )
        .await
        .expect("session");
    agent_manager
        .configure_agent(
            &session.id,
            SessionAgent {
                tool_policy,
                ..profile_session_agent(MOCK_PROFILE)
            },
        )
        .await
        .expect("configure the agent");
    MockSession {
        agent_manager,
        session_id: session.id,
        event_tx,
        events,
    }
}

#[allow(dead_code)]
impl MockSession {
    /// Send `prompt` and require the turn to complete within `limit`.
    pub async fn turn(&self, prompt: &str, limit: Duration) -> TurnOutcome {
        let (_id, done) = self
            .agent_manager
            .send_message_notified(
                &self.session_id,
                prompt.to_string(),
                &self.event_tx,
                true,
                None,
            )
            .await
            .expect("the turn is accepted");
        completed_turn(done, limit).await
    }
}
