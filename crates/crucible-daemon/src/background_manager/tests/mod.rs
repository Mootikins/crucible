use super::*;
use crucible_core::config::{AgentProfile, BackendType, DelegationConfig};
use crucible_core::session::OutputValidation;
use crucible_core::traits::chat::{AgentHandle, ChatResult};
use crucible_core::turn::{StopReason, TurnError, TurnEvent};
use std::collections::HashMap;
use std::sync::Mutex as StdMutex;
use tokio::sync::broadcast;

mod bash;
mod capabilities;
mod delegation;
mod delegation_events;
mod subagent;

pub(super) fn create_manager() -> BackgroundJobManager {
    let (tx, _) = broadcast::channel(16);
    BackgroundJobManager::new(tx)
}

#[derive(Clone)]
pub(super) enum MockSubagentBehavior {
    ImmediateSuccess(String),
    DelayedSuccess { output: String, delay: Duration },
    DelayedFailure { error: String, delay: Duration },
    Pending,
    StreamFailure(String),
}

pub(super) struct MockSubagentHandle {
    behavior: MockSubagentBehavior,
}

#[async_trait]
impl crucible_core::turn::Agent for MockSubagentHandle {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        _ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        let behavior = self.behavior.clone();
        let body = async_stream::stream! {
            match behavior {
                MockSubagentBehavior::ImmediateSuccess(output) => {
                    yield TurnEvent::TextDelta(output);
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                }
                MockSubagentBehavior::DelayedSuccess { output, delay } => {
                    tokio::time::sleep(delay).await;
                    yield TurnEvent::TextDelta(output);
                    yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
                }
                MockSubagentBehavior::DelayedFailure { error, delay } => {
                    tokio::time::sleep(delay).await;
                    yield TurnEvent::Error(TurnError::Internal(error));
                }
                MockSubagentBehavior::Pending => {
                    futures::future::pending::<()>().await;
                }
                MockSubagentBehavior::StreamFailure(message) => {
                    yield TurnEvent::Error(TurnError::Internal(message));
                }
            }
        };
        Ok(Box::pin(body))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

impl MockSubagentHandle {
    pub(super) fn new(behavior: MockSubagentBehavior) -> Self {
        Self { behavior }
    }
}

#[async_trait]
impl AgentHandle for MockSubagentHandle {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }
    async fn set_mode_str(&mut self, _mode_id: &str) -> ChatResult<()> {
        Ok(())
    }
}

pub(super) fn test_session_agent(delegation_config: Option<DelegationConfig>) -> SessionAgent {
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
        precognition_results: 5,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
    }
}

pub(super) fn default_enabled_delegation_config() -> DelegationConfig {
    DelegationConfig {
        enabled: true,
        max_depth: 1,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }
}

pub(super) fn test_agent_profile(command: &str, args: &[&str]) -> AgentProfile {
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

pub(super) fn make_subagent_manager_with_factory_and_identity(
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

pub(super) fn make_subagent_manager_with_factory(
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

pub(super) fn make_subagent_manager_with_factory_and_events(
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

pub(super) fn behavior_factory(behavior: MockSubagentBehavior) -> SubagentFactory {
    Box::new(move |_agent, _workspace| {
        let behavior = behavior.clone();
        Box::pin(async move {
            Ok(Box::new(MockSubagentHandle::new(behavior)) as Box<dyn AgentHandle + Send + Sync>)
        })
    })
}
