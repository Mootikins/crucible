use super::*;
use crucible_config::{AgentProfile, BackendType, DelegationConfig};
use crucible_core::background::JobStatus;
use crucible_core::session::OutputValidation;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult};
use futures::stream::{self, BoxStream};
use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use std::time::Instant;
use tokio::sync::broadcast;

fn create_manager() -> BackgroundJobManager {
    let (tx, _) = broadcast::channel(16);
    BackgroundJobManager::new(tx)
}

#[derive(Clone)]
enum MockSubagentBehavior {
    ImmediateSuccess(String),
    DelayedSuccess { output: String, delay: Duration },
    DelayedFailure { error: String, delay: Duration },
    Pending,
    StreamFailure(String),
}

struct MockSubagentHandle {
    behavior: MockSubagentBehavior,
}

impl MockSubagentHandle {
    fn new(behavior: MockSubagentBehavior) -> Self {
        Self { behavior }
    }
}

fn chunk(delta: String, done: bool) -> ChatChunk {
    ChatChunk {
        delta,
        done,
        tool_calls: None,
        tool_results: None,
        reasoning: None,
        usage: None,
        subagent_events: None,
        precognition_notes_count: None,
        precognition_notes: None,
    }
}

#[async_trait]
impl AgentHandle for MockSubagentHandle {
    fn send_message_stream(
        &mut self,
        _message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        match self.behavior.clone() {
            MockSubagentBehavior::ImmediateSuccess(output) => {
                Box::pin(stream::iter(vec![Ok(chunk(output, true))]))
            }
            MockSubagentBehavior::DelayedSuccess { output, delay } => {
                Box::pin(stream::once(async move {
                    tokio::time::sleep(delay).await;
                    Ok(chunk(output, true))
                }))
            }
            MockSubagentBehavior::DelayedFailure { error, delay } => {
                Box::pin(stream::once(async move {
                    tokio::time::sleep(delay).await;
                    Err(ChatError::Internal(error))
                }))
            }
            MockSubagentBehavior::Pending => Box::pin(stream::pending()),
            MockSubagentBehavior::StreamFailure(message) => {
                Box::pin(stream::iter(vec![Err(ChatError::Internal(message))]))
            }
        }
    }

    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true
    }
}

fn test_session_agent(delegation_config: Option<DelegationConfig>) -> SessionAgent {
    SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("test-agent".to_string()),
        provider_key: None,
        provider: BackendType::Custom,
        model: "test-agent".to_string(),
        system_prompt: String::new(),
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
        delegation_config,
        precognition_enabled: false,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
    }
}

fn default_enabled_delegation_config() -> DelegationConfig {
    DelegationConfig {
        enabled: true,
        max_depth: 1,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }
}

fn test_agent_profile(command: &str, args: &[&str]) -> AgentProfile {
    AgentProfile {
        extends: None,
        command: Some(command.to_string()),
        args: Some(args.iter().map(|arg| arg.to_string()).collect()),
        env: HashMap::new(),
        description: Some("delegation test target".to_string()),
        capabilities: None,
        delegation: None,
        permissions: None,
    }
}

fn make_subagent_manager_with_factory_and_identity(
    factory: SubagentFactory,
    delegation_config: Option<DelegationConfig>,
    delegator_agent_name: Option<&str>,
    target_agent_name: Option<&str>,
) -> BackgroundJobManager {
    let delegation_config = delegation_config.or_else(|| Some(default_enabled_delegation_config()));
    let (tx, _) = broadcast::channel(16);
    let manager = BackgroundJobManager::new(tx).with_subagent_factory(factory);
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(delegation_config),
            available_agents: HashMap::new(),
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: None,
            delegator_agent_name: delegator_agent_name.map(str::to_string),
            target_agent_name: target_agent_name.map(str::to_string),
            delegation_depth: 0,
        },
    );
    manager
}

fn make_subagent_manager_with_factory(
    factory: SubagentFactory,
    delegation_config: Option<DelegationConfig>,
) -> BackgroundJobManager {
    make_subagent_manager_with_factory_and_identity(
        factory,
        delegation_config,
        Some("parent-agent"),
        Some("worker-agent"),
    )
}

fn make_subagent_manager_with_factory_and_events(
    factory: SubagentFactory,
    delegation_config: Option<DelegationConfig>,
) -> (
    BackgroundJobManager,
    broadcast::Receiver<SessionEventMessage>,
) {
    let delegation_config = delegation_config.or_else(|| Some(default_enabled_delegation_config()));
    let (tx, rx) = broadcast::channel(32);
    let manager = BackgroundJobManager::new(tx).with_subagent_factory(factory);
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(delegation_config),
            available_agents: HashMap::new(),
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: None,
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 0,
        },
    );
    (manager, rx)
}

fn behavior_factory(behavior: MockSubagentBehavior) -> SubagentFactory {
    Box::new(move |_agent, _workspace| {
        let behavior = behavior.clone();
        Box::pin(async move {
            Ok(Box::new(MockSubagentHandle::new(behavior)) as Box<dyn AgentHandle + Send + Sync>)
        })
    })
}

#[tokio::test]
async fn spawn_subagent_blocking_returns_job_result_with_output() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::DelayedSuccess {
            output: "blocking-complete".to_string(),
            delay: Duration::from_millis(75),
        }),
        None,
    );
    let start = Instant::now();

    let result: Result<JobResult, BackgroundError> = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await;

    let result = result.expect("blocking subagent should complete");
    assert!(start.elapsed() >= Duration::from_millis(70));
    assert_eq!(result.info.status, JobStatus::Completed);
    assert_eq!(result.output.as_deref(), Some("blocking-complete"));
}

#[tokio::test]
async fn spawn_subagent_blocking_timeout_returns_failed_job_result() {
    let manager =
        make_subagent_manager_with_factory(behavior_factory(MockSubagentBehavior::Pending), None);

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig {
                timeout: Duration::from_millis(50),
                result_max_bytes: 51200,
            },
            None,
        )
        .await
        .expect("timeout should return JobResult");

    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result.error.as_deref().unwrap_or("").contains("timed out"));
}

#[tokio::test]
async fn spawn_subagent_blocking_cancellation_marks_job_cancelled() {
    let manager =
        make_subagent_manager_with_factory(behavior_factory(MockSubagentBehavior::Pending), None);
    let (cancel_tx, cancel_rx) = oneshot::channel();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = cancel_tx.send(());
    });

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            Some(cancel_rx),
        )
        .await
        .expect("cancelled execution should return JobResult");

    assert_eq!(result.info.status, JobStatus::Cancelled);
    assert!(result.error.as_deref().unwrap_or("").contains("cancelled"));
}

#[tokio::test]
async fn spawn_subagent_blocking_factory_failure_returns_background_error() {
    let manager = make_subagent_manager_with_factory(
        Box::new(move |_agent, _workspace| {
            Box::pin(async move { Err("factory failed".to_string()) })
        }),
        None,
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("factory failure should return BackgroundError");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
}

#[tokio::test]
async fn spawn_subagent_blocking_execution_failure_returns_failed_job_result() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::StreamFailure(
            "agent-stream-broke".to_string(),
        )),
        None,
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("execution failure should still return JobResult");

    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result
        .error
        .as_deref()
        .unwrap_or("")
        .contains("agent-stream-broke"));
}

#[tokio::test]
async fn spawn_subagent_blocking_truncates_output_to_configured_max_bytes() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("x".repeat(512))),
        None,
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig {
                timeout: Duration::from_secs(1),
                result_max_bytes: 32,
            },
            None,
        )
        .await
        .expect("subagent should complete");

    let output = result.output.unwrap_or_default();
    assert!(output.len() <= 32, "output length was {}", output.len());
}

#[tokio::test]
async fn spawn_subagent_blocking_disables_nested_delegation_before_factory() {
    let observed = Arc::new(StdMutex::new(None));
    let observed_for_factory = observed.clone();
    let manager = make_subagent_manager_with_factory_and_identity(
        Box::new(move |agent, _workspace| {
            let mut lock = observed_for_factory
                .lock()
                .expect("observation mutex should be available");
            *lock = Some(agent.delegation_config.clone());
            Box::pin(async move {
                Ok(Box::new(MockSubagentHandle::new(
                    MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                )) as Box<dyn AgentHandle + Send + Sync>)
            })
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 2,
            allowed_targets: Some(vec!["worker-agent".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        Some("worker-agent"),
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "do it".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("blocking run should succeed");

    let observed = observed
        .lock()
        .expect("observation mutex should be available")
        .clone();
    assert_eq!(observed, Some(None));
}

#[tokio::test]
async fn delegation_happy_path_returns_result_to_parent() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess(
            "delegation-result".to_string(),
        )),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("delegation-context".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation should succeed");

    assert_eq!(result.info.status, JobStatus::Completed);
    assert_eq!(result.output.as_deref(), Some("delegation-result"));
}

#[tokio::test]
async fn delegation_rejected_when_disabled() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: false,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("disabled delegation should be rejected");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("Delegation is disabled"));
}

#[tokio::test]
async fn delegation_rejected_when_target_not_allowed() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["allowed-agent".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unauthorized delegation target should be rejected");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("not allowed"));
}

#[tokio::test]
async fn test_delegation_with_target_uses_different_session_agent() {
    let observed = Arc::new(StdMutex::new(None));
    let observed_for_factory = observed.clone();
    let manager = make_subagent_manager_with_factory_and_identity(
        Box::new(move |agent, _workspace| {
            let mut lock = observed_for_factory
                .lock()
                .expect("observation mutex should be available");
            *lock = Some(agent.clone());
            Box::pin(async move {
                Ok(Box::new(MockSubagentHandle::new(
                    MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                )) as Box<dyn AgentHandle + Send + Sync>)
            })
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["cursor".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        Some("worker-agent"),
    );
    let mut agent_profiles = HashMap::new();
    agent_profiles.insert(
        "cursor".to_string(),
        test_agent_profile("cursor-acp", &["acp"]),
    );
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["cursor".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            })),
            available_agents: agent_profiles,
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: None,
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 0,
        },
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("Target agent: cursor".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation with explicit target should succeed");

    let observed = observed
        .lock()
        .expect("observation mutex should be available")
        .clone()
        .expect("factory should have observed target agent config");
    assert_eq!(observed.agent_name.as_deref(), Some("cursor"));
    assert_eq!(observed.model, "cursor");
}

#[tokio::test]
async fn test_delegation_with_target_validates_allowed() {
    let manager = make_subagent_manager_with_factory_and_identity(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["allowed-agent".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        None,
    );
    let mut agent_profiles = HashMap::new();
    agent_profiles.insert(
        "cursor".to_string(),
        test_agent_profile("cursor-acp", &["acp"]),
    );
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: Some(vec!["allowed-agent".to_string()]),
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            })),
            available_agents: agent_profiles,
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: None,
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: None,
            delegation_depth: 0,
        },
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("Target agent: cursor".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unauthorized explicit target should be rejected");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("not allowed"));
}

#[tokio::test]
async fn test_delegation_with_unknown_target_returns_available_agents() {
    let manager = make_subagent_manager_with_factory_and_identity(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["ghost".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        None,
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("Target agent: ghost".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unknown explicit target should fail with available list");

    let msg = err.to_string();
    assert!(msg.contains("Delegation target 'ghost' not found"));
    assert!(msg.contains("Available agents:"));
}

#[tokio::test]
async fn test_delegation_without_target_uses_parent_agent() {
    let observed = Arc::new(StdMutex::new(None));
    let observed_for_factory = observed.clone();
    let manager = make_subagent_manager_with_factory_and_identity(
        Box::new(move |agent, _workspace| {
            let mut lock = observed_for_factory
                .lock()
                .expect("observation mutex should be available");
            *lock = Some(agent.clone());
            Box::pin(async move {
                Ok(Box::new(MockSubagentHandle::new(
                    MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
                )) as Box<dyn AgentHandle + Send + Sync>)
            })
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        None,
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            Some("Delegation ID: deleg-1\nDescription: no explicit target".to_string()),
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation without explicit target should succeed");

    let observed = observed
        .lock()
        .expect("observation mutex should be available")
        .clone()
        .expect("factory should have observed parent agent config");
    assert_eq!(observed.agent_name.as_deref(), Some("test-agent"));
    assert_eq!(observed.model, "test-agent");
}

#[tokio::test]
async fn delegation_timeout_returns_failed_job_result() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::Pending),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig {
                timeout: Duration::from_millis(30),
                result_max_bytes: 51200,
            },
            None,
        )
        .await
        .expect("timeout should return a failed JobResult");

    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result.error.as_deref().unwrap_or("").contains("timed out"));
}

#[tokio::test]
async fn delegation_unavailable_agent_returns_error() {
    let manager = make_subagent_manager_with_factory(
        Box::new(move |_agent, _workspace| {
            Box::pin(async move { Err("command not found: mock-subagent".to_string()) })
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("unavailable target agent should return error");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("command not found"));
}

#[tokio::test]
async fn delegation_self_delegation_guard_rejects_same_agent() {
    let manager = make_subagent_manager_with_factory_and_identity(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: Some(vec!["parent-agent".to_string()]),
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
        Some("parent-agent"),
        Some("parent-agent"),
    );

    let err = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect_err("self delegation must be rejected");

    assert!(matches!(err, BackgroundError::SpawnFailed(_)));
    assert!(err.to_string().contains("self-delegation"));
}

#[tokio::test]
async fn delegation_result_truncation_respects_config_limit() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("y".repeat(200))),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 16,
            max_concurrent_delegations: 3,
        }),
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig {
                timeout: Duration::from_secs(1),
                result_max_bytes: 16,
            },
            None,
        )
        .await
        .expect("delegation should complete");

    let output = result.output.unwrap_or_default();
    assert!(output.len() <= 16, "output length was {}", output.len());
}

#[tokio::test]
async fn delegation_blocking_emits_spawned_and_completed_events() {
    let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess(
            "eventful-result".to_string(),
        )),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 3,
        }),
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("blocking delegation should succeed");

    assert_eq!(result.info.status, JobStatus::Completed);

    let mut saw_spawned = false;
    let mut saw_completed = false;
    for _ in 0..5 {
        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout waiting for delegation event")
            .expect("failed to receive delegation event");

        if event.event == events::SUBAGENT_SPAWNED {
            saw_spawned = true;
        }
        if event.event == events::SUBAGENT_COMPLETED {
            saw_completed = true;
        }

        if saw_spawned && saw_completed {
            break;
        }
    }

    assert!(saw_spawned, "expected {} event", events::SUBAGENT_SPAWNED);
    assert!(
        saw_completed,
        "expected {} event",
        events::SUBAGENT_COMPLETED
    );
}

#[tokio::test]
async fn delegation_rejected_when_max_concurrent_delegations_reached() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::Pending),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 1,
        }),
    );

    let first_job_id = manager
        .spawn_subagent("session-1", "delegate first".to_string(), None)
        .await
        .expect("first delegation should spawn");

    tokio::time::sleep(Duration::from_millis(25)).await;

    let err = manager
        .spawn_subagent("session-1", "delegate second".to_string(), None)
        .await
        .expect_err("second delegation should be rejected at concurrency limit");

    assert!(err
        .to_string()
        .contains("Maximum concurrent delegations (1) exceeded"));

    manager.cancel_job(&first_job_id).await;
}

#[tokio::test]
async fn delegation_under_max_concurrent_delegations_is_allowed() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::Pending),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 2,
        }),
    );

    let first_job_id = manager
        .spawn_subagent("session-1", "delegate first".to_string(), None)
        .await
        .expect("first delegation should spawn");
    let second_job_id = manager
        .spawn_subagent("session-1", "delegate second".to_string(), None)
        .await
        .expect("second delegation should still be allowed under limit");

    manager.cancel_job(&first_job_id).await;
    manager.cancel_job(&second_job_id).await;
}

#[tokio::test]
async fn completed_delegation_frees_concurrency_slot() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::DelayedSuccess {
            output: "done".to_string(),
            delay: Duration::from_millis(80),
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 1,
        }),
    );

    let _first = manager
        .spawn_subagent("session-1", "delegate first".to_string(), None)
        .await
        .expect("first delegation should spawn");

    tokio::time::sleep(Duration::from_millis(10)).await;

    let blocked = manager
        .spawn_subagent("session-1", "delegate blocked".to_string(), None)
        .await;
    assert!(
        blocked.is_err(),
        "second delegation should be blocked while first is running"
    );

    tokio::time::sleep(Duration::from_millis(120)).await;

    let second = manager
        .spawn_subagent("session-1", "delegate second".to_string(), None)
        .await
        .expect("delegation slot should be freed after completion");

    manager.cancel_job(&second).await;
}

#[tokio::test]
async fn failed_delegation_frees_concurrency_slot() {
    let manager = make_subagent_manager_with_factory(
        behavior_factory(MockSubagentBehavior::DelayedFailure {
            error: "boom".to_string(),
            delay: Duration::from_millis(80),
        }),
        Some(DelegationConfig {
            enabled: true,
            max_depth: 1,
            allowed_targets: None,
            result_max_bytes: 51200,
            max_concurrent_delegations: 1,
        }),
    );

    let _first = manager
        .spawn_subagent("session-1", "delegate first".to_string(), None)
        .await
        .expect("first delegation should spawn");

    tokio::time::sleep(Duration::from_millis(10)).await;

    let blocked = manager
        .spawn_subagent("session-1", "delegate blocked".to_string(), None)
        .await;
    assert!(
        blocked.is_err(),
        "second delegation should be blocked while first is running"
    );

    tokio::time::sleep(Duration::from_millis(120)).await;

    let second = manager
        .spawn_subagent("session-1", "delegate second".to_string(), None)
        .await
        .expect("delegation slot should be freed after failure");

    manager.cancel_job(&second).await;
}

#[tokio::test]
async fn delegation_writes_parent_session_id_and_incremented_depth_to_child_session() {
    let parent_dir = tempfile::TempDir::new().expect("temp dir should be created");
    let (tx, _) = broadcast::channel(16);
    let manager = BackgroundJobManager::new(tx).with_subagent_factory(behavior_factory(
        MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
    ));
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(Some(DelegationConfig {
                enabled: true,
                max_depth: 1,
                allowed_targets: None,
                result_max_bytes: 51200,
                max_concurrent_delegations: 3,
            })),
            available_agents: HashMap::new(),
            workspace: std::env::temp_dir(),
            parent_session_id: Some("session-1".to_string()),
            parent_session_dir: Some(parent_dir.path().to_path_buf()),
            delegator_agent_name: Some("parent-agent".to_string()),
            target_agent_name: Some("worker-agent".to_string()),
            delegation_depth: 0,
        },
    );

    let result = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate this".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation should complete");

    let session_path = result
        .info
        .session_path
        .expect("subagent session path should exist");
    let jsonl_path = session_path.join("session.jsonl");
    let contents = tokio::fs::read_to_string(&jsonl_path)
        .await
        .expect("subagent session jsonl should be readable");

    let metadata_line = contents
        .lines()
        .find_map(|line| {
            let event: serde_json::Value = serde_json::from_str(line).ok()?;
            if event.get("type")?.as_str()? != "system" {
                return None;
            }
            let content = event.get("content")?.as_str()?;
            serde_json::from_str::<serde_json::Value>(content).ok()
        })
        .expect("delegation metadata system event should exist");

    assert_eq!(
        metadata_line["delegation_metadata"]["parent_session_id"]
            .as_str()
            .expect("parent_session_id should be present"),
        "session-1"
    );
    assert_eq!(
        metadata_line["delegation_metadata"]["delegation_depth"]
            .as_u64()
            .expect("delegation_depth should be present"),
        1
    );
}

#[tokio::test]
async fn spawn_bash_returns_job_id_immediately() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "echo hello".to_string(), None, None)
        .await
        .unwrap();

    assert!(job_id.starts_with("job-"));
}

#[tokio::test]
async fn job_appears_in_list_while_running() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 5".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let jobs = manager.list_jobs("session-1");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, job_id);
    assert_eq!(jobs[0].status, JobStatus::Running);

    manager.cancel_job(&job_id).await;
}

#[tokio::test]
async fn completed_job_moves_to_history() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "echo done".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());

    let result = result.unwrap();
    assert!(result.info.status.is_terminal());
}

#[tokio::test]
async fn cancel_job_stops_running_job() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 60".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(manager.running.contains_key(&job_id));

    let cancelled = manager.cancel_job(&job_id).await;
    assert!(cancelled);

    assert!(!manager.running.contains_key(&job_id));
}

#[tokio::test]
async fn history_eviction_at_limit() {
    let (tx, _) = broadcast::channel(16);
    let mut manager = BackgroundJobManager::new(tx);
    manager.max_history = 3;

    for i in 0..5 {
        let _ = manager
            .spawn_bash("session-1", format!("echo job-{i}"), None, None)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    let jobs = manager.list_jobs("session-1");
    assert!(
        jobs.len() <= 3,
        "Should have at most 3 jobs, got {}",
        jobs.len()
    );
}

#[tokio::test]
async fn get_job_result_for_running_job() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 5".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());
    assert_eq!(result.unwrap().info.status, JobStatus::Running);

    manager.cancel_job(&job_id).await;
}

#[tokio::test]
async fn cleanup_session_cancels_all_jobs() {
    let manager = create_manager();

    for i in 0..3 {
        let _ = manager
            .spawn_bash("session-1", format!("sleep {}", 10 + i), None, None)
            .await
            .unwrap();
    }

    let _ = manager
        .spawn_bash("session-2", "sleep 10".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(manager.running_count("session-1"), 3);
    assert_eq!(manager.running_count("session-2"), 1);

    manager.cleanup_session("session-1", true).await;

    assert_eq!(manager.running_count("session-1"), 0);
    assert_eq!(manager.running_count("session-2"), 1);

    manager.cleanup_session("session-2", false).await;
}

#[tokio::test]
async fn job_timeout() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash(
            "session-1",
            "sleep 10".to_string(),
            None,
            Some(Duration::from_millis(100)),
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());

    let result = result.unwrap();
    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result
        .error
        .as_ref()
        .is_some_and(|e| e.contains("timed out")));
}

#[tokio::test]
async fn different_sessions_have_separate_histories() {
    let manager = create_manager();

    let _ = manager
        .spawn_bash("session-1", "echo one".to_string(), None, None)
        .await
        .unwrap();
    let _ = manager
        .spawn_bash("session-2", "echo two".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let jobs_1 = manager.list_jobs("session-1");
    let jobs_2 = manager.list_jobs("session-2");

    assert_eq!(jobs_1.len(), 1);
    assert_eq!(jobs_2.len(), 1);
    assert_ne!(jobs_1[0].id, jobs_2[0].id);
}

#[tokio::test]
async fn completed_job_preserves_started_at_for_duration() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 0.1".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = manager.get_job_result(&job_id).unwrap();
    let duration = result
        .info
        .duration()
        .expect("completed job should have duration");
    let millis = duration.num_milliseconds();

    assert!(
        millis >= 100,
        "Duration {}ms should be >= 100ms (job ran sleep 0.1)",
        millis
    );
    assert!(
        millis < 5000,
        "Duration {}ms should be < 5000ms (sanity check)",
        millis
    );
}

#[tokio::test]
async fn failed_bash_command_has_error_output() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "false".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());

    let result = result.unwrap();
    assert_eq!(result.info.status, JobStatus::Failed);
    assert!(result.error.is_some());
    let error = result.error.unwrap();
    assert!(error.contains("Exit code") || error.contains("1"));
}

#[tokio::test]
async fn bash_with_workdir_executes_in_directory() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash(
            "session-1",
            "pwd".to_string(),
            Some(PathBuf::from("/tmp")),
            None,
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = manager.get_job_result(&job_id);
    assert!(result.is_some());

    let result = result.unwrap();
    assert!(result.info.status.is_terminal());
    let output = result.output.unwrap_or_default();
    assert!(output.contains("/tmp"));
}

#[tokio::test]
async fn cancel_job_for_wrong_session_is_denied() {
    let manager = create_manager();

    let job_id = manager
        .spawn_bash("session-1", "sleep 60".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let cancelled = manager
        .cancel_job_for_session(&job_id, Some("session-2"))
        .await;
    assert!(!cancelled);

    assert!(manager.running.contains_key(&job_id));

    manager.cancel_job(&job_id).await;
}

#[tokio::test]
async fn cancel_nonexistent_job_returns_false() {
    let manager = create_manager();

    let fake_job_id = JobId::from("job-nonexistent");
    let cancelled = manager.cancel_job(&fake_job_id).await;

    assert!(!cancelled);
}

#[tokio::test]
async fn bash_events_are_broadcast() {
    let (tx, mut rx) = broadcast::channel(16);
    let manager = BackgroundJobManager::new(tx);

    let _job_id = manager
        .spawn_bash("session-1", "echo test".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timeout waiting for event")
        .expect("failed to receive event");

    assert_eq!(event.session_id, "session-1");
    assert_eq!(event.event, events::BASH_SPAWNED);

    tokio::time::sleep(Duration::from_millis(200)).await;

    let completion_event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("timeout waiting for completion event")
        .expect("failed to receive completion event");

    assert_eq!(completion_event.session_id, "session-1");
    assert!(
        completion_event.event == events::BASH_COMPLETED
            || completion_event.event == events::BASH_FAILED
    );
}

#[tokio::test]
async fn total_running_count_across_sessions() {
    let manager = create_manager();

    let job_id_1 = manager
        .spawn_bash("session-1", "sleep 10".to_string(), None, None)
        .await
        .unwrap();

    let job_id_2 = manager
        .spawn_bash("session-2", "sleep 10".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(manager.total_running_count(), 2);

    manager.cancel_job(&job_id_1).await;

    assert_eq!(manager.total_running_count(), 1);

    manager.cancel_job(&job_id_2).await;

    assert_eq!(manager.total_running_count(), 0);
}

#[tokio::test]
async fn cleanup_session_with_clear_history_removes_history() {
    let manager = create_manager();

    let _job_id = manager
        .spawn_bash("session-1", "echo done".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let jobs_before = manager.list_jobs("session-1");
    assert_eq!(jobs_before.len(), 1);

    manager.cleanup_session("session-1", true).await;

    let jobs_after = manager.list_jobs("session-1");
    assert_eq!(jobs_after.len(), 0);
}

#[tokio::test]
async fn cleanup_session_preserves_history_when_clear_history_false() {
    let manager = create_manager();

    let _job_id = manager
        .spawn_bash("session-1", "echo done".to_string(), None, None)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    let jobs_before = manager.list_jobs("session-1");
    assert_eq!(jobs_before.len(), 1);

    manager.cleanup_session("session-1", false).await;

    let jobs_after = manager.list_jobs("session-1");
    assert_eq!(jobs_after.len(), 1);
}

#[tokio::test]
async fn background_spawner_trait_spawn_bash() {
    let manager = create_manager();
    let spawner: &dyn BackgroundSpawner = &manager;

    let job_id = spawner
        .spawn_bash("session-1", "echo trait".to_string(), None, None)
        .await
        .unwrap();

    assert!(job_id.starts_with("job-"));

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = spawner.get_job_result(&job_id);
    assert!(result.is_some());
    assert!(result.unwrap().info.status.is_terminal());
}

#[test]
fn subagent_context_default_delegation_depth_is_zero() {
    let ctx = SubagentContext {
        agent: test_session_agent(None),
        available_agents: HashMap::new(),
        workspace: std::env::temp_dir(),
        parent_session_id: Some("session-1".to_string()),
        parent_session_dir: None,
        delegator_agent_name: None,
        target_agent_name: None,
        delegation_depth: 0,
    };

    assert_eq!(ctx.delegation_depth, 0);
}

#[test]
fn enforce_delegation_capabilities_rejects_depth_at_hard_cap() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        3,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation depth limit exceeded"));
}

#[test]
fn enforce_delegation_capabilities_rejects_depth_above_hard_cap() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        5,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation depth limit exceeded"));
}

#[test]
fn enforce_delegation_capabilities_allows_depth_below_hard_cap() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        0,
        "session-1",
    );

    assert!(result.is_ok());
}

#[test]
fn enforce_delegation_capabilities_allows_depth_one() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        1,
        "session-1",
    );

    assert!(result.is_ok());
}

#[test]
fn enforce_delegation_capabilities_allows_depth_two() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        2,
        "session-1",
    );

    assert!(result.is_ok());
}

#[test]
fn enforce_delegation_capabilities_hard_cap_checked_before_enabled_check() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: false,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        3,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation depth limit exceeded"));
}

#[tokio::test]
async fn delegation_spawned_event_emitted_on_parent_channel() {
    let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess("ok".to_string())),
        None,
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate task".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation should succeed");

    let mut saw_delegation_spawned = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Ok(event)) => {
                if event.event == events::DELEGATION_SPAWNED {
                    saw_delegation_spawned = true;
                    assert_eq!(event.session_id, "session-1");
                    assert!(event.data["delegation_id"].as_str().is_some());
                    assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                    assert!(event.data["prompt"]
                        .as_str()
                        .unwrap_or("")
                        .contains("delegate"));
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_delegation_spawned,
        "expected delegation_spawned event on parent channel"
    );
}

#[tokio::test]
async fn delegation_completed_event_emitted_on_parent_channel() {
    let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
        behavior_factory(MockSubagentBehavior::ImmediateSuccess(
            "result-data".to_string(),
        )),
        None,
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate task".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("delegation should succeed");

    let mut saw_delegation_completed = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Ok(event)) => {
                if event.event == events::DELEGATION_COMPLETED {
                    saw_delegation_completed = true;
                    assert_eq!(event.session_id, "session-1");
                    assert!(event.data["delegation_id"].as_str().is_some());
                    assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                    assert!(event.data["result_summary"]
                        .as_str()
                        .unwrap_or("")
                        .contains("result-data"));
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_delegation_completed,
        "expected delegation_completed event on parent channel"
    );
}

#[tokio::test]
async fn delegation_failed_event_emitted_on_parent_channel() {
    let (manager, mut rx) = make_subagent_manager_with_factory_and_events(
        behavior_factory(MockSubagentBehavior::StreamFailure(
            "agent-crashed".to_string(),
        )),
        None,
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "delegate task".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("failed delegation still returns a JobResult");

    let mut saw_delegation_failed = false;
    for _ in 0..10 {
        match tokio::time::timeout(Duration::from_secs(1), rx.recv()).await {
            Ok(Ok(event)) => {
                if event.event == events::DELEGATION_FAILED {
                    saw_delegation_failed = true;
                    assert_eq!(event.session_id, "session-1");
                    assert!(event.data["delegation_id"].as_str().is_some());
                    assert_eq!(event.data["parent_session_id"].as_str(), Some("session-1"));
                    assert!(event.data["error"]
                        .as_str()
                        .unwrap_or("")
                        .contains("agent-crashed"));
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_delegation_failed,
        "expected delegation_failed event on parent channel"
    );
}

#[tokio::test]
async fn non_delegation_subagent_does_not_emit_delegation_events() {
    let (tx, mut rx) = broadcast::channel(32);
    let manager = BackgroundJobManager::new(tx).with_subagent_factory(behavior_factory(
        MockSubagentBehavior::ImmediateSuccess("ok".to_string()),
    ));
    manager.register_subagent_context(
        "session-1",
        SubagentContext {
            agent: test_session_agent(Some(default_enabled_delegation_config())),
            available_agents: HashMap::new(),
            workspace: std::env::temp_dir(),
            parent_session_id: None,
            parent_session_dir: None,
            delegator_agent_name: Some("parent".to_string()),
            target_agent_name: Some("child".to_string()),
            delegation_depth: 0,
        },
    );

    let _ = manager
        .spawn_subagent_blocking(
            "session-1",
            "do task".to_string(),
            None,
            SubagentBlockingConfig::default(),
            None,
        )
        .await
        .expect("subagent should succeed");

    let mut delegation_events = vec![];
    while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await {
        if event.event == events::DELEGATION_SPAWNED
            || event.event == events::DELEGATION_COMPLETED
            || event.event == events::DELEGATION_FAILED
        {
            delegation_events.push(event.event.clone());
        }
    }
    assert!(
        delegation_events.is_empty(),
        "non-delegation subagent should not emit delegation events, got: {:?}",
        delegation_events
    );
}
