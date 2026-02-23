use async_trait::async_trait;
use crucible_acp::discovery::default_agent_profiles;
use crucible_config::{AcpConfig, AgentProfile, BackendType, DelegationConfig};
use crucible_core::background::{JobResult, JobStatus, SubagentBlockingConfig};
use crucible_core::session::{SessionAgent, SessionType};
use crucible_core::traits::chat::{AgentHandle, ChatChunk};
use crucible_core::traits::ChatResult;
use crucible_daemon::background_manager::{BackgroundJobManager, SubagentContext, SubagentFactory};
use crucible_daemon::protocol::SessionEventMessage;
use crucible_daemon::{AgentManager, FileSessionStorage, KilnManager, SessionManager};
use futures::stream::{self, BoxStream};
use futures::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout, Instant};

#[derive(Clone)]
enum MockSubagentBehavior {
    ImmediateSuccess(String),
    DelayedSuccess { output: String, delay: Duration },
    Pending,
}

struct MockSubagentHandle {
    behavior: MockSubagentBehavior,
}

impl MockSubagentHandle {
    fn new(behavior: MockSubagentBehavior) -> Self {
        Self { behavior }
    }
}

#[async_trait]
impl AgentHandle for MockSubagentHandle {
    fn send_message_stream(
        &mut self,
        _message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        match self.behavior.clone() {
            MockSubagentBehavior::ImmediateSuccess(output) => stream::iter(vec![Ok(ChatChunk {
                delta: output,
                done: true,
                tool_calls: None,
                tool_results: None,
                reasoning: None,
                usage: None,
                subagent_events: None,
            })])
            .boxed(),
            MockSubagentBehavior::DelayedSuccess { output, delay } => stream::once(async move {
                sleep(delay).await;
                Ok(ChatChunk {
                    delta: output,
                    done: true,
                    tool_calls: None,
                    tool_results: None,
                    reasoning: None,
                    usage: None,
                    subagent_events: None,
                })
            })
            .boxed(),
            MockSubagentBehavior::Pending => stream::pending().boxed(),
        }
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

fn test_session_agent(enabled: bool, max_concurrent_delegations: u32) -> SessionAgent {
    SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("parent-agent".to_string()),
        provider_key: None,
        provider: BackendType::Mock,
        model: "mock-model".to_string(),
        system_prompt: "test".to_string(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: Some(DelegationConfig {
            enabled,
            max_depth: 2,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations,
        }),
        precognition_enabled: false,
    }
}

fn test_root_session_agent(enabled: bool, max_concurrent_delegations: u32) -> SessionAgent {
    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
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
        capabilities: None,
        agent_description: None,
        delegation_config: Some(DelegationConfig {
            enabled,
            max_depth: 2,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations,
        }),
        precognition_enabled: false,
    }
}

fn setup_with_delegation_config(allowed_targets: Option<Vec<String>>) -> SessionAgent {
    let mut root_agent = test_root_session_agent(true, 3);
    root_agent.delegation_config = Some(DelegationConfig {
        enabled: true,
        max_depth: 2,
        allowed_targets,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    });
    root_agent
}

fn make_subagent_factory(behavior: MockSubagentBehavior) -> SubagentFactory {
    Box::new(move |_agent: &SessionAgent, _workspace: &Path| {
        let behavior = behavior.clone();
        Box::pin(async move {
            Ok(Box::new(MockSubagentHandle::new(behavior)) as Box<dyn AgentHandle + Send + Sync>)
        })
    })
}

fn make_observing_subagent_factory(
    observed: Arc<StdMutex<Option<SessionAgent>>>,
    behavior: MockSubagentBehavior,
) -> SubagentFactory {
    Box::new(move |agent: &SessionAgent, _workspace: &Path| {
        let mut lock = observed
            .lock()
            .expect("observation lock should be available");
        *lock = Some(agent.clone());
        let behavior = behavior.clone();
        Box::pin(async move {
            Ok(Box::new(MockSubagentHandle::new(behavior)) as Box<dyn AgentHandle + Send + Sync>)
        })
    })
}

fn register_delegation_context(
    manager: &BackgroundJobManager,
    session_id: &str,
    workspace: &Path,
    parent_session_dir: Option<&Path>,
    enabled: bool,
    max_concurrent_delegations: u32,
) {
    manager.register_subagent_context(
        session_id,
        SubagentContext {
            agent: test_session_agent(enabled, max_concurrent_delegations),
            available_agents: HashMap::new(),
            workspace: workspace.to_path_buf(),
            parent_session_id: Some(session_id.to_string()),
            parent_session_dir: parent_session_dir.map(|p| p.to_path_buf()),
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 0,
        },
    );
}

async fn next_event(
    rx: &mut broadcast::Receiver<SessionEventMessage>,
    event_name: &str,
) -> SessionEventMessage {
    timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(event) = rx.recv().await {
                if event.event == event_name {
                    return event;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

async fn wait_for_terminal_result(manager: &BackgroundJobManager, job_id: &str) -> JobResult {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(result) = manager.get_job_result(&job_id.to_string()) {
            if result.info.status.is_terminal() {
                return result;
            }
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for terminal result"
        );
        sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn delegation_pipeline_background_emits_events_and_persists_result() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, mut rx) = broadcast::channel(32);
    let manager = Arc::new(BackgroundJobManager::new(event_tx).with_subagent_factory(
        make_subagent_factory(MockSubagentBehavior::DelayedSuccess {
            output: "delegation-result".to_string(),
            delay: Duration::from_millis(40),
        }),
    ));

    let session_id = "session-delegation-events";
    register_delegation_context(
        manager.as_ref(),
        session_id,
        temp.path(),
        Some(temp.path()),
        true,
        3,
    );

    let delegation_id = manager
        .spawn_subagent(
            session_id,
            "Delegate this task".to_string(),
            Some("integration test".to_string()),
        )
        .await
        .expect("delegation spawn should succeed");

    let spawned = next_event(&mut rx, "delegation_spawned").await;
    let completed = next_event(&mut rx, "delegation_completed").await;

    let spawned_id = spawned.data["delegation_id"]
        .as_str()
        .expect("delegation_spawned should contain delegation_id");
    assert_eq!(spawned_id, delegation_id);
    assert_eq!(spawned.session_id, session_id);
    assert_eq!(spawned.data["parent_session_id"].as_str(), Some(session_id));

    assert_eq!(completed.session_id, session_id);
    assert_eq!(completed.data["delegation_id"].as_str(), Some(spawned_id));
    assert_eq!(
        completed.data["parent_session_id"].as_str(),
        Some(session_id)
    );

    let result = wait_for_terminal_result(manager.as_ref(), &delegation_id).await;
    assert_eq!(result.info.status, JobStatus::Completed);
    assert_eq!(result.output.as_deref(), Some("delegation-result"));
}

#[tokio::test]
async fn delegation_pipeline_blocking_returns_after_completion_with_result() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(BackgroundJobManager::new(event_tx).with_subagent_factory(
        make_subagent_factory(MockSubagentBehavior::DelayedSuccess {
            output: "blocking-result".to_string(),
            delay: Duration::from_millis(120),
        }),
    ));

    let session_id = "session-delegation-blocking";
    register_delegation_context(
        manager.as_ref(),
        session_id,
        temp.path(),
        Some(temp.path()),
        true,
        3,
    );

    let start = Instant::now();
    let output = manager
        .spawn_subagent_blocking(
            session_id,
            "Run blocking".to_string(),
            Some("blocking test".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("blocking delegation should succeed");
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(100),
        "blocking call returned too early: {:?}",
        elapsed
    );

    assert_eq!(output.info.status, JobStatus::Completed);
    assert_eq!(output.output.as_deref(), Some("blocking-result"));
}

#[tokio::test]
async fn delegation_pipeline_enforces_concurrent_limit() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(
        BackgroundJobManager::new(event_tx)
            .with_subagent_factory(make_subagent_factory(MockSubagentBehavior::Pending)),
    );

    let session_id = "session-delegation-concurrency";
    register_delegation_context(
        manager.as_ref(),
        session_id,
        temp.path(),
        Some(temp.path()),
        true,
        3,
    );

    let mut spawned_ids = Vec::new();
    for _ in 0..3 {
        let delegation_id = manager
            .spawn_subagent(
                session_id,
                "pending delegation".to_string(),
                Some("concurrency test".to_string()),
            )
            .await
            .expect("delegation under limit should spawn");
        spawned_ids.push(delegation_id);
    }

    let err = manager
        .spawn_subagent(
            session_id,
            "fourth delegation".to_string(),
            Some("should fail".to_string()),
        )
        .await
        .expect_err("fourth delegation should be rejected");
    assert!(err
        .to_string()
        .contains("Maximum concurrent delegations (3) exceeded"));

    for id in spawned_ids {
        manager.cancel_job(&id).await;
    }
}

#[tokio::test]
async fn delegation_pipeline_rejects_when_disabled() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(BackgroundJobManager::new(event_tx).with_subagent_factory(
        make_subagent_factory(MockSubagentBehavior::ImmediateSuccess("unused".to_string())),
    ));

    let session_id = "session-delegation-disabled";
    register_delegation_context(
        manager.clone().as_ref(),
        session_id,
        temp.path(),
        None,
        false,
        3,
    );

    let err = manager
        .spawn_subagent_blocking(
            session_id,
            "should not run".to_string(),
            Some("disabled test".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("disabled delegation should fail immediately");
    assert!(err.to_string().contains("Delegation is disabled"));

    assert!(manager.list_jobs(session_id).is_empty());
}

#[tokio::test]
async fn test_root_session_delegation_succeeds() {
    let temp = TempDir::new().expect("temp dir");
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));

    let (event_tx, _) = broadcast::channel(32);
    let background_manager = Arc::new(
        BackgroundJobManager::new(event_tx.clone()).with_subagent_factory(make_subagent_factory(
            MockSubagentBehavior::ImmediateSuccess("root-delegation-result".to_string()),
        )),
    );
    let agent_manager = AgentManager::new(
        Arc::new(KilnManager::new()),
        session_manager.clone(),
        background_manager.clone(),
        None,
        None,
        None,
        None,
    );

    let session = session_manager
        .create_session(
            SessionType::Chat,
            temp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .expect("session should be created");

    agent_manager
        .configure_agent(&session.id, test_root_session_agent(true, 3))
        .await
        .expect("agent should be configured");

    let _ = agent_manager
        .send_message(&session.id, "prime agent cache".to_string(), &event_tx)
        .await
        .expect("agent handle should be created");

    let output = background_manager
        .spawn_subagent_blocking(
            &session.id,
            "run root delegation".to_string(),
            Some("root delegation integration".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("root session delegation should succeed");

    assert_eq!(output.info.status, JobStatus::Completed);
    assert_eq!(output.output.as_deref(), Some("root-delegation-result"));
}

#[tokio::test]
async fn test_delegation_to_acp_agent_creates_acp_session() {
    let temp = TempDir::new().expect("temp dir");
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let observed = Arc::new(StdMutex::new(None));

    let (event_tx, _) = broadcast::channel(32);
    let background_manager = Arc::new(
        BackgroundJobManager::new(event_tx.clone()).with_subagent_factory(
            make_observing_subagent_factory(
                observed.clone(),
                MockSubagentBehavior::ImmediateSuccess("delegated-acp-result".to_string()),
            ),
        ),
    );
    let mut acp_config = AcpConfig::default();
    acp_config.agents.insert(
        "opencode".to_string(),
        AgentProfile {
            extends: None,
            command: Some("opencode".to_string()),
            args: Some(vec!["acp".to_string()]),
            env: HashMap::new(),
            description: Some("OpenCode ACP".to_string()),
            capabilities: None,
            delegation: None,
        },
    );

    let agent_manager = AgentManager::new(
        Arc::new(KilnManager::new()),
        session_manager.clone(),
        background_manager.clone(),
        None,
        None,
        Some(acp_config),
        None,
    );

    let session = session_manager
        .create_session(
            SessionType::Chat,
            temp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .expect("session should be created");

    let mut root_agent = test_root_session_agent(true, 3);
    root_agent.delegation_config = Some(DelegationConfig {
        enabled: true,
        max_depth: 2,
        allowed_targets: Some(vec!["opencode".to_string()]),
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    });

    agent_manager
        .configure_agent(&session.id, root_agent)
        .await
        .expect("agent should be configured");

    let _ = agent_manager
        .send_message(&session.id, "prime agent cache".to_string(), &event_tx)
        .await
        .expect("agent handle should be created");

    let output = background_manager
        .spawn_subagent_blocking(
            &session.id,
            "delegate to opencode".to_string(),
            Some("Delegation ID: deleg-1\nTarget agent: opencode\nDescription: test".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation to opencode should succeed");

    assert_eq!(output.info.status, JobStatus::Completed);
    assert_eq!(output.output.as_deref(), Some("delegated-acp-result"));

    let observed_agent = observed
        .lock()
        .expect("observation lock should be available")
        .clone()
        .expect("subagent factory should capture child session agent");
    assert_eq!(observed_agent.agent_type, "acp");
    assert_eq!(observed_agent.agent_name.as_deref(), Some("opencode"));
    assert_eq!(observed_agent.model, "opencode");
}

#[tokio::test]
async fn test_cross_agent_delegation_full_pipeline() {
    let temp = TempDir::new().expect("temp dir");
    let storage = Arc::new(FileSessionStorage::new());
    let session_manager = Arc::new(SessionManager::with_storage(storage));
    let observed = Arc::new(StdMutex::new(None));

    let (event_tx, _) = broadcast::channel(32);
    let background_manager = Arc::new(
        BackgroundJobManager::new(event_tx.clone()).with_subagent_factory(
            make_observing_subagent_factory(
                observed.clone(),
                MockSubagentBehavior::ImmediateSuccess("delegated-opencode-result".to_string()),
            ),
        ),
    );

    let available_agents = default_agent_profiles();
    let opencode_profile = available_agents
        .get("opencode")
        .expect("default profiles should include opencode");
    assert_eq!(opencode_profile.command.as_deref(), Some("opencode"));
    assert_eq!(
        opencode_profile.args.as_ref(),
        Some(&vec!["acp".to_string()])
    );

    let session = session_manager
        .create_session(
            SessionType::Chat,
            temp.path().to_path_buf(),
            None,
            vec![],
            None,
        )
        .await
        .expect("session should be created");

    let root_agent = setup_with_delegation_config(Some(vec!["opencode".to_string()]));
    let subagent_context = SubagentContext {
        agent: root_agent,
        available_agents: available_agents.clone(),
        workspace: temp.path().to_path_buf(),
        parent_session_id: Some(session.id.clone()),
        parent_session_dir: Some(session.storage_path()),
        delegator_agent_name: Some("parent-agent".to_string()),
        target_agent_name: None,
        delegation_depth: 0,
    };
    assert!(subagent_context.available_agents.contains_key("opencode"));
    background_manager.register_subagent_context(&session.id, subagent_context);

    let context = "Delegation ID: deleg-e2e\nTarget agent: opencode\nDescription: end-to-end";
    let output = background_manager
        .spawn_subagent_blocking(
            &session.id,
            "delegate to opencode end-to-end".to_string(),
            Some(context.to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("cross-agent delegation should succeed");

    assert_eq!(output.info.status, JobStatus::Completed);
    assert_eq!(output.output.as_deref(), Some("delegated-opencode-result"));

    let observed_agent = observed
        .lock()
        .expect("observation lock should be available")
        .clone()
        .expect("subagent factory should capture child session agent");
    assert_eq!(observed_agent.agent_type, "acp");
    assert_eq!(observed_agent.agent_name.as_deref(), Some("opencode"));
    assert_eq!(observed_agent.model, "opencode");
}

// --- Edge case tests ---

#[tokio::test]
async fn test_delegation_unknown_target_lists_available() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(BackgroundJobManager::new(event_tx).with_subagent_factory(
        make_subagent_factory(MockSubagentBehavior::ImmediateSuccess("unused".to_string())),
    ));

    let session_id = "session-unknown-target";
    let mut available_agents = HashMap::new();
    available_agents.insert(
        "claude".to_string(),
        AgentProfile {
            extends: None,
            command: Some("claude".to_string()),
            args: Some(vec!["acp".to_string()]),
            env: HashMap::new(),
            description: Some("Claude ACP".to_string()),
            capabilities: None,
            delegation: None,
        },
    );
    available_agents.insert(
        "opencode".to_string(),
        AgentProfile {
            extends: None,
            command: Some("opencode".to_string()),
            args: Some(vec!["acp".to_string()]),
            env: HashMap::new(),
            description: Some("OpenCode ACP".to_string()),
            capabilities: None,
            delegation: None,
        },
    );

    manager.register_subagent_context(
        session_id,
        SubagentContext {
            agent: test_session_agent(true, 3),
            available_agents,
            workspace: temp.path().to_path_buf(),
            parent_session_id: Some(session_id.to_string()),
            parent_session_dir: Some(temp.path().to_path_buf()),
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 0,
        },
    );

    let err = manager
        .spawn_subagent_blocking(
            session_id,
            "delegate to nonexistent".to_string(),
            Some("Target agent: nonexistent".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unknown target should fail");

    let msg = err.to_string();
    assert!(
        msg.contains("Delegation target 'nonexistent' not found"),
        "error should name the unknown target, got: {msg}"
    );
    assert!(
        msg.contains("claude"),
        "error should list available agent 'claude', got: {msg}"
    );
    assert!(
        msg.contains("opencode"),
        "error should list available agent 'opencode', got: {msg}"
    );
}

#[tokio::test]
async fn test_delegation_depth_limit_enforced() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(BackgroundJobManager::new(event_tx).with_subagent_factory(
        make_subagent_factory(MockSubagentBehavior::ImmediateSuccess("unused".to_string())),
    ));

    let session_id = "session-depth-limit";
    // Register context at depth 2; child will be depth 3 which hits hard cap (>= 3)
    manager.register_subagent_context(
        session_id,
        SubagentContext {
            agent: test_session_agent(true, 3),
            available_agents: HashMap::new(),
            workspace: temp.path().to_path_buf(),
            parent_session_id: Some(session_id.to_string()),
            parent_session_dir: Some(temp.path().to_path_buf()),
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 2,
        },
    );

    let err = manager
        .spawn_subagent_blocking(
            session_id,
            "delegate deeper".to_string(),
            Some("depth test".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("depth limit should reject delegation");

    let msg = err.to_string();
    assert!(
        msg.contains("Delegation depth limit exceeded"),
        "error should mention depth limit, got: {msg}"
    );
    assert!(
        msg.contains("hard cap at 3"),
        "error should mention hard cap value, got: {msg}"
    );
}

#[tokio::test]
async fn test_delegation_concurrent_limit_enforced() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(
        BackgroundJobManager::new(event_tx)
            .with_subagent_factory(make_subagent_factory(MockSubagentBehavior::Pending)),
    );

    let session_id = "session-concurrent-limit-1";
    // Set max_concurrent_delegations to 1 — tightest possible limit
    register_delegation_context(
        manager.as_ref(),
        session_id,
        temp.path(),
        Some(temp.path()),
        true,
        1,
    );

    let first_id = manager
        .spawn_subagent(
            session_id,
            "first delegation".to_string(),
            Some("concurrent test".to_string()),
        )
        .await
        .expect("first delegation should succeed with limit of 1");

    let err = manager
        .spawn_subagent(
            session_id,
            "second delegation".to_string(),
            Some("should be rejected".to_string()),
        )
        .await
        .expect_err("second delegation should be rejected at limit 1");

    let msg = err.to_string();
    assert!(
        msg.contains("Maximum concurrent delegations (1) exceeded"),
        "error should mention the exact limit of 1, got: {msg}"
    );

    manager.cancel_job(&first_id).await;
}

#[tokio::test]
async fn test_delegation_empty_available_agents_error() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(BackgroundJobManager::new(event_tx).with_subagent_factory(
        make_subagent_factory(MockSubagentBehavior::ImmediateSuccess("unused".to_string())),
    ));

    let session_id = "session-empty-agents";
    // Register with zero available_agents — any target lookup must fail clearly
    manager.register_subagent_context(
        session_id,
        SubagentContext {
            agent: test_session_agent(true, 3),
            available_agents: HashMap::new(),
            workspace: temp.path().to_path_buf(),
            parent_session_id: Some(session_id.to_string()),
            parent_session_dir: Some(temp.path().to_path_buf()),
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: None,
            delegation_depth: 0,
        },
    );

    let err = manager
        .spawn_subagent_blocking(
            session_id,
            "delegate to missing".to_string(),
            Some("Target agent: phantom".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("target with empty agents should fail");

    let msg = err.to_string();
    assert!(
        msg.contains("Delegation target 'phantom' not found"),
        "error should name the missing target, got: {msg}"
    );
    assert!(
        msg.contains("(none)"),
        "error should show (none) when no agents registered, got: {msg}"
    );
}
