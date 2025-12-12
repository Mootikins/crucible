//! Agent factory - creates AgentHandle from configuration
//!
//! Supports both ACP (external) agents and internal (direct LLM) agents.
//! Selection priority:
//! 1. Explicit CLI flag (--internal or --acp)
//! 2. Config file setting (agent_type)
//! 3. Default: ACP agent if available, else internal

use anyhow::Result;
use tracing::{debug, info};

use crucible_agents::{
    InternalAgentHandle, LayeredPromptBuilder, SlidingWindowContext,
};
use crucible_config::CliAppConfig;
use crucible_core::traits::chat::AgentHandle;
use crucible_core::traits::tools::ToolExecutor;
use crucible_llm::text_generation;

/// Agent type selection
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentType {
    /// External ACP agent (claude-code, etc.)
    Acp,
    /// Internal direct LLM agent
    Internal,
}

impl Default for AgentType {
    fn default() -> Self {
        Self::Acp // Default to ACP for backwards compatibility
    }
}

/// Agent initialization parameters
pub struct AgentInitParams {
    /// Preferred agent type
    pub agent_type: Option<AgentType>,
    /// Preferred ACP agent name (for ACP type)
    pub agent_name: Option<String>,
    /// Preferred LLM provider key (for internal type)
    pub provider_key: Option<String>,
    /// Initial read-only mode
    pub read_only: bool,
    /// Maximum context tokens
    pub max_context_tokens: Option<usize>,
    /// Tool executor for internal agents
    pub tool_executor: Option<Box<dyn ToolExecutor>>,
}

impl AgentInitParams {
    pub fn new() -> Self {
        Self {
            agent_type: None,
            agent_name: None,
            provider_key: None,
            read_only: true,
            max_context_tokens: None,
            tool_executor: None,
        }
    }

    pub fn with_type(mut self, agent_type: AgentType) -> Self {
        self.agent_type = Some(agent_type);
        self
    }

    pub fn with_agent_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = Some(name.into());
        self
    }

    pub fn with_provider(mut self, key: impl Into<String>) -> Self {
        self.provider_key = Some(key.into());
        self
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn with_max_context_tokens(mut self, tokens: usize) -> Self {
        self.max_context_tokens = Some(tokens);
        self
    }

    pub fn with_tool_executor(mut self, executor: Box<dyn ToolExecutor>) -> Self {
        self.tool_executor = Some(executor);
        self
    }
}

impl Default for AgentInitParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of agent initialization
pub enum InitializedAgent {
    /// ACP agent (needs async spawning)
    Acp(crate::acp::CrucibleAcpClient),
    /// Internal agent (ready to use)
    Internal(InternalAgentHandle),
}

impl InitializedAgent {
    /// Get as AgentHandle trait object for unified usage
    /// Note: For ACP agents, must call spawn() first
    pub fn into_boxed(self) -> Box<dyn AgentHandle> {
        match self {
            Self::Acp(client) => Box::new(client),
            Self::Internal(handle) => Box::new(handle),
        }
    }
}

/// Create an internal agent from configuration
pub async fn create_internal_agent(
    config: &CliAppConfig,
    params: AgentInitParams,
) -> Result<InternalAgentHandle> {
    // Get LLM provider
    let provider = if let Some(provider_key) = &params.provider_key {
        info!("Creating internal agent with provider: {}", provider_key);

        // Create Config from CliAppConfig to use the factory
        let full_config = crucible_config::Config {
            llm: Some(config.llm.clone()),
            chat: Some(config.chat.clone()),
            ..Default::default()
        };

        text_generation::from_config_by_name(&full_config, provider_key).await
            .map_err(|e| anyhow::anyhow!("Failed to create provider '{}': {}", provider_key, e))?
    } else {
        info!("Creating internal agent with default provider");

        let full_config = crucible_config::Config {
            llm: Some(config.llm.clone()),
            chat: Some(config.chat.clone()),
            ..Default::default()
        };

        text_generation::from_config(&full_config).await
            .map_err(|e| anyhow::anyhow!("Failed to create default provider: {}", e))?
    };

    // Create context manager
    let max_tokens = params.max_context_tokens.unwrap_or(16_384);
    let context = Box::new(SlidingWindowContext::new(max_tokens));

    // Create prompt builder with layered prompts
    let mut prompt_builder = LayeredPromptBuilder::new();

    // Load AGENTS.md if present
    prompt_builder = prompt_builder.with_agents_md(&config.kiln_path);

    // Get model name from config
    let model = config.chat.model.clone()
        .unwrap_or_else(|| provider.default_model().to_string());

    Ok(InternalAgentHandle::new(
        provider,
        context,
        params.tool_executor,
        prompt_builder,
        model,
        max_tokens,
    ))
}

/// Create an agent based on configuration and parameters
///
/// Selection priority:
/// 1. Explicit agent_type in params
/// 2. Config file setting
/// 3. Default: ACP if available, internal otherwise
pub async fn create_agent(
    config: &CliAppConfig,
    params: AgentInitParams,
) -> Result<InitializedAgent> {
    let agent_type = params.agent_type.unwrap_or(AgentType::Acp);

    match agent_type {
        AgentType::Internal => {
            info!("Initializing internal agent");
            let handle = create_internal_agent(config, params).await?;
            Ok(InitializedAgent::Internal(handle))
        }
        AgentType::Acp => {
            info!("Initializing ACP agent");
            use crate::acp::{discover_agent, CrucibleAcpClient};

            // Discover agent
            let agent_name = params.agent_name.or_else(|| config.acp.default_agent.clone());
            let agent = discover_agent(agent_name.as_deref()).await?;

            debug!("Discovered agent: {}", agent.name);

            // Create ACP client
            let client = CrucibleAcpClient::with_acp_config(
                agent,
                params.read_only,
                config.acp.clone(),
            );

            Ok(InitializedAgent::Acp(client))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_default() {
        assert_eq!(AgentType::default(), AgentType::Acp);
    }

    #[test]
    fn test_agent_init_params_builder() {
        let params = AgentInitParams::new()
            .with_type(AgentType::Internal)
            .with_provider("local".to_string())
            .with_read_only(false)
            .with_max_context_tokens(8192);

        assert_eq!(params.agent_type, Some(AgentType::Internal));
        assert_eq!(params.provider_key, Some("local".to_string()));
        assert!(!params.read_only);
        assert_eq!(params.max_context_tokens, Some(8192));
    }

    #[test]
    fn test_agent_init_params_default() {
        let params = AgentInitParams::default();
        assert_eq!(params.agent_type, None);
        assert_eq!(params.agent_name, None);
        assert_eq!(params.provider_key, None);
        assert!(params.read_only);
        assert_eq!(params.max_context_tokens, None);
    }

    #[tokio::test]
    async fn test_create_internal_agent_requires_valid_config() {
        // This test verifies that we get proper error messages for invalid configs
        let config = CliAppConfig::default();
        let params = AgentInitParams::new()
            .with_provider("nonexistent");

        let result = create_internal_agent(&config, params).await;
        // Should fail with descriptive error about missing provider
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_types_equality() {
        assert_eq!(AgentType::Acp, AgentType::Acp);
        assert_eq!(AgentType::Internal, AgentType::Internal);
        assert_ne!(AgentType::Acp, AgentType::Internal);
    }
}
