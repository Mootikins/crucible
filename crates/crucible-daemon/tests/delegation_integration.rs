use async_trait::async_trait;
use crucible_config::{BackendType, DelegationConfig};
use crucible_core::background::{JobResult, JobStatus, SubagentBlockingConfig};
use crucible_core::session::SessionAgent;
use crucible_core::traits::ChatResult;
use crucible_core::traits::chat::{AgentHandle, ChatChunk};
use crucible_daemon::background_manager::{BackgroundJobManager, SubagentContext, SubagentFactory};
use crucible_daemon::protocol::SessionEventMessage;
use futures::stream::{self, BoxStream};
use futures::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
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
    fn send_message_stream(&mut self, _message: String) -> BoxStream<'static, ChatResult<ChatChunk>> {
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
            MockSubagentBehavior::DelayedSuccess { output, delay } => {
                stream::once(async move {
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
                .boxed()
            }
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
    }
}

fn make_subagent_factory(behavior: MockSubagentBehavior) -> SubagentFactory {
    Box::new(move |_agent: &SessionAgent, _workspace: &Path| {
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

        assert!(Instant::now() < deadline, "timed out waiting for terminal result");
        sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn delegation_pipeline_background_emits_events_and_persists_result() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, mut rx) = broadcast::channel(32);
    let manager = Arc::new(
        BackgroundJobManager::new(event_tx).with_subagent_factory(make_subagent_factory(
            MockSubagentBehavior::DelayedSuccess {
                output: "delegation-result".to_string(),
                delay: Duration::from_millis(40),
            },
        )),
    );

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
    assert_eq!(completed.data["parent_session_id"].as_str(), Some(session_id));

    let result = wait_for_terminal_result(manager.as_ref(), &delegation_id).await;
    assert_eq!(result.info.status, JobStatus::Completed);
    assert_eq!(result.output.as_deref(), Some("delegation-result"));
}

#[tokio::test]
async fn delegation_pipeline_blocking_returns_after_completion_with_result() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(
        BackgroundJobManager::new(event_tx).with_subagent_factory(make_subagent_factory(
            MockSubagentBehavior::DelayedSuccess {
                output: "blocking-result".to_string(),
                delay: Duration::from_millis(120),
            },
        )),
    );

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
    assert!(
        err.to_string()
            .contains("Maximum concurrent delegations (3) exceeded")
    );

    for id in spawned_ids {
        manager.cancel_job(&id).await;
    }
}

#[tokio::test]
async fn delegation_pipeline_rejects_when_disabled() {
    let temp = TempDir::new().expect("temp dir");
    let (event_tx, _) = broadcast::channel(32);
    let manager = Arc::new(
        BackgroundJobManager::new(event_tx).with_subagent_factory(make_subagent_factory(
            MockSubagentBehavior::ImmediateSuccess("unused".to_string()),
        )),
    );

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
