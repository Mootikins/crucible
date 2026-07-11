use super::*;
use crucible_core::config::{AgentProfile, BackendType, DelegationConfig};
use crucible_core::session::OutputValidation;
use crucible_core::traits::chat::AgentHandle;
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

pub(super) use crate::test_support::{MockSubagentBehavior, MockSubagentHandle};

pub(super) fn test_session_agent(delegation_config: Option<DelegationConfig>) -> SessionAgent {
    SessionAgent {
        mode: None,
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
        autocompact_threshold: None,
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
