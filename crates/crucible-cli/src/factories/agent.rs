//! Agent factory - creates AgentHandle from configuration
//!
//! Supports both ACP (external) agents and internal (direct LLM) agents.
//! Selection priority:
//! 1. Explicit CLI flag (--internal or --acp)
//! 2. Config file setting (agent_type)
//! 3. Default: ACP agent if available, else internal
//!
//! Internal agents use the Rig framework for LLM interaction.

use anyhow::Result;
use tracing::{debug, info};

use crucible_config::CliAppConfig;
use crucible_core::traits::chat::AgentHandle;

/// Agent type selection
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum AgentType {
    /// External ACP agent (claude-code, etc.)
    #[default]
    Acp,
    /// Internal direct LLM agent
    Internal,
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
    /// Environment variable overrides for ACP agents
    /// These are merged with any env vars from config profiles
    pub env_overrides: std::collections::HashMap<String, String>,
    /// Working directory for the agent (where it should operate)
    /// Distinct from kiln_path which is where knowledge is stored.
    pub working_dir: Option<std::path::PathBuf>,
}

impl AgentInitParams {
    pub fn new() -> Self {
        Self {
            agent_type: None,
            agent_name: None,
            provider_key: None,
            read_only: true,
            max_context_tokens: None,
            env_overrides: std::collections::HashMap::new(),
            working_dir: None,
        }
    }

    /// Set the working directory for the agent
    ///
    /// This is where the agent will operate (for file operations, git, etc.).
    pub fn with_working_dir(mut self, path: std::path::PathBuf) -> Self {
        self.working_dir = Some(path);
        self
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

    /// Set agent name from Option (convenient for CLI flags)
    pub fn with_agent_name_opt(mut self, name: Option<String>) -> Self {
        self.agent_name = name;
        self
    }

    /// Set provider from Option (convenient for CLI flags)
    pub fn with_provider_opt(mut self, key: Option<String>) -> Self {
        self.provider_key = key;
        self
    }

    /// Set environment variable overrides for ACP agents
    ///
    /// These will be merged with any env vars from config profiles,
    /// with CLI overrides taking precedence.
    pub fn with_env_overrides(mut self, env: std::collections::HashMap<String, String>) -> Self {
        self.env_overrides = env;
        self
    }

    /// Set the model for an ACP agent (typically OpenCode)
    ///
    /// This adds the OPENCODE_MODEL environment variable, which tells OpenCode
    /// which model to use. Preserves any existing environment overrides.
    pub fn with_model(mut self, model_id: impl Into<String>) -> Self {
        self.env_overrides
            .insert("OPENCODE_MODEL".to_string(), model_id.into());
        self
    }
}

impl Default for AgentInitParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of agent initialization
#[allow(clippy::large_enum_variant)] // Acp variant is large but this enum is not frequently cloned
pub enum InitializedAgent {
    /// ACP agent (needs async spawning)
    Acp(crate::acp::CrucibleAcpClient),
    /// Internal agent (ready to use)
    ///
    /// This is boxed to support both native (InternalAgentHandle) and
    /// Rig (RigAgentHandle) backends via trait object erasure.
    Internal(Box<dyn AgentHandle + Send + Sync>),
}

impl InitializedAgent {
    /// Get the display name for this agent type
    pub fn display_name(&self) -> &str {
        match self {
            Self::Acp(client) => client.agent_name(),
            Self::Internal(_) => "internal",
        }
    }

    /// Get as AgentHandle trait object for unified usage
    /// Note: For ACP agents, must call spawn() first
    pub fn into_boxed(self) -> Box<dyn AgentHandle> {
        match self {
            Self::Acp(client) => Box::new(client),
            Self::Internal(handle) => handle,
        }
    }
}

/// Create an internal agent using the Rig framework
pub async fn create_internal_agent(
    config: &CliAppConfig,
    _params: AgentInitParams,
) -> Result<Box<dyn AgentHandle + Send + Sync>> {
    use crucible_config::LlmProvider;
    use crucible_context::{LayeredPromptBuilder, PromptBuilder};
    use crucible_core::prompts::{base_prompt_for_size, ModelSize};
    use crucible_rig::{build_agent_with_model_size, RigAgentHandle};

    // Get model name from config
    let model = config
        .chat
        .model
        .clone()
        .unwrap_or_else(|| match config.chat.provider {
            LlmProvider::Ollama => "llama3.2".to_string(),
            LlmProvider::OpenAI => "gpt-4o".to_string(),
            LlmProvider::Anthropic => "claude-3-5-sonnet-20241022".to_string(),
        });

    // Detect model size and get appropriate prompt
    let model_size = ModelSize::from_model_name(&model);
    info!("Detected model size: {:?} for {}", model_size, model);

    // Build layered system prompt
    let mut prompt_builder = LayeredPromptBuilder::new();

    // Replace the default base prompt with size-appropriate one
    prompt_builder.remove_layer("base");
    prompt_builder.add_layer(
        crucible_context::priorities::BASE,
        "base",
        base_prompt_for_size(model_size).to_string(),
    );

    // Get workspace root for project rules loading
    let workspace_root = _params
        .working_dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| config.kiln_path.clone()));

    // Add project rules if workspace has them (AGENTS.md, CLAUDE.md, .rules, etc.)
    prompt_builder = prompt_builder.with_project_rules(&workspace_root);

    let system_prompt = prompt_builder.build();

    let agent_config = crucible_rig::AgentConfig::new(&model, &system_prompt);

    use crucible_config::{LlmProviderConfig, LlmProviderType};

    // Create Rig client based on provider
    let client = match config.chat.provider {
        LlmProvider::Ollama => {
            let endpoint = config
                .chat
                .endpoint
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string());

            // For custom Ollama endpoints (not localhost), use OpenAI-compatible client
            // This provides better tool calling support via /v1/chat/completions
            let is_local = endpoint.contains("localhost") || endpoint.contains("127.0.0.1");

            if is_local {
                // Local Ollama - use native client
                crucible_rig::create_client(&LlmProviderConfig {
                    provider_type: LlmProviderType::Ollama,
                    endpoint: Some(endpoint),
                    default_model: Some(model.clone()),
                    temperature: config.chat.temperature,
                    max_tokens: config.chat.max_tokens,
                    timeout_secs: config.chat.timeout_secs,
                    api_key: None,
                })?
            } else {
                // Remote Ollama-compatible (e.g., llama-swappo) - use OpenAI-compatible client
                // Append /v1 if not already present for OpenAI-compatible endpoint
                let compat_endpoint = if endpoint.ends_with("/v1") {
                    endpoint
                } else {
                    format!("{}/v1", endpoint.trim_end_matches('/'))
                };
                info!(
                    "Using OpenAI-compatible endpoint for remote Ollama: {}",
                    compat_endpoint
                );
                crucible_rig::create_client(&LlmProviderConfig {
                    provider_type: LlmProviderType::OpenAI,
                    endpoint: Some(compat_endpoint),
                    default_model: Some(model.clone()),
                    temperature: config.chat.temperature,
                    max_tokens: config.chat.max_tokens,
                    timeout_secs: config.chat.timeout_secs,
                    api_key: None,
                })?
            }
        }
        LlmProvider::OpenAI => crucible_rig::create_client(&LlmProviderConfig {
            provider_type: LlmProviderType::OpenAI,
            endpoint: config.chat.endpoint.clone(),
            default_model: Some(model.clone()),
            temperature: config.chat.temperature,
            max_tokens: config.chat.max_tokens,
            timeout_secs: config.chat.timeout_secs,
            api_key: Some("OPENAI_API_KEY".to_string()),
        })?,
        LlmProvider::Anthropic => crucible_rig::create_client(&LlmProviderConfig {
            provider_type: LlmProviderType::Anthropic,
            endpoint: config.chat.endpoint.clone(),
            default_model: Some(model.clone()),
            temperature: config.chat.temperature,
            max_tokens: config.chat.max_tokens,
            timeout_secs: config.chat.timeout_secs,
            api_key: Some("ANTHROPIC_API_KEY".to_string()),
        })?,
    };

    info!(
        "Building Rig agent with {:?} tools for: {}",
        model_size,
        workspace_root.display()
    );

    // Build Rig agent with size-appropriate tools based on client type
    match client {
        crucible_rig::RigClient::Ollama(ollama_client) => {
            let agent = build_agent_with_model_size(
                &agent_config,
                &ollama_client,
                &workspace_root,
                model_size,
            )?;
            Ok(Box::new(RigAgentHandle::new(agent)))
        }
        crucible_rig::RigClient::OpenAI(openai_client) => {
            let agent = build_agent_with_model_size(
                &agent_config,
                &openai_client,
                &workspace_root,
                model_size,
            )?;
            Ok(Box::new(RigAgentHandle::new(agent)))
        }
        crucible_rig::RigClient::OpenAICompat(compat_client) => {
            let agent = build_agent_with_model_size(
                &agent_config,
                &compat_client,
                &workspace_root,
                model_size,
            )?;
            Ok(Box::new(RigAgentHandle::new(agent)))
        }
        crucible_rig::RigClient::Anthropic(anthropic_client) => {
            let agent = build_agent_with_model_size(
                &agent_config,
                &anthropic_client,
                &workspace_root,
                model_size,
            )?;
            Ok(Box::new(RigAgentHandle::new(agent)))
        }
    }
}

/// Create an agent based on configuration and parameters
///
/// Selection priority:
/// 1. Explicit agent_type in params
/// 2. Config file setting (chat.agent_preference)
/// 3. Default: ACP
pub async fn create_agent(
    config: &CliAppConfig,
    params: AgentInitParams,
) -> Result<InitializedAgent> {
    use crucible_config::AgentPreference;

    // Determine agent type from params or config
    let agent_type = params
        .agent_type
        .unwrap_or(match config.chat.agent_preference {
            AgentPreference::Crucible => AgentType::Internal,
            AgentPreference::Acp => AgentType::Acp,
        });

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
            let agent_name = params
                .agent_name
                .or_else(|| config.acp.default_agent.clone());
            let mut agent = discover_agent(agent_name.as_deref()).await?;

            debug!("Discovered agent: {}", agent.name);

            // Merge config profile env vars first (lower priority)
            if let Some(profile) = config.acp.agents.get(&agent.name) {
                if !profile.env.is_empty() {
                    let keys: Vec<_> = profile.env.keys().collect();
                    info!("Applying config profile env vars: {:?}", keys);
                    agent.env_vars.extend(profile.env.clone());
                }
            }

            // Merge CLI env overrides (highest priority - overwrites config)
            if !params.env_overrides.is_empty() {
                let keys: Vec<_> = params.env_overrides.keys().collect();
                info!("Applying CLI env overrides: {:?}", keys);
                agent.env_vars.extend(params.env_overrides);
            }

            // Create ACP client
            let mut client =
                CrucibleAcpClient::with_acp_config(agent, params.read_only, config.acp.clone());

            // Set working directory if provided
            if let Some(working_dir) = params.working_dir {
                info!("Setting agent working directory: {}", working_dir.display());
                client = client.with_working_dir(working_dir);
            }

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

    #[test]
    fn test_params_with_model_injects_env_var() {
        let params = AgentInitParams::default();
        let modified = params.with_model("anthropic/claude-sonnet-4");
        assert_eq!(
            modified.env_overrides.get("OPENCODE_MODEL"),
            Some(&"anthropic/claude-sonnet-4".to_string())
        );
    }

    #[test]
    fn test_params_with_model_preserves_other_env_vars() {
        let mut env = std::collections::HashMap::new();
        env.insert("EXISTING_VAR".to_string(), "value".to_string());

        let params = AgentInitParams::default()
            .with_env_overrides(env)
            .with_model("test-model");

        assert_eq!(
            params.env_overrides.get("EXISTING_VAR"),
            Some(&"value".to_string())
        );
        assert_eq!(
            params.env_overrides.get("OPENCODE_MODEL"),
            Some(&"test-model".to_string())
        );
    }

    #[tokio::test]
    #[ignore = "Requires Ollama to be running - Rig uses config.chat.provider, not params.provider_key"]
    async fn test_create_internal_agent_with_default_config() {
        // This test verifies that internal agent creation works with default config
        // Requires Ollama to be running since default provider is Ollama
        let config = CliAppConfig::default();
        let params = AgentInitParams::new();

        let result = create_internal_agent(&config, params).await;
        // Should succeed if Ollama is available
        assert!(result.is_ok());
    }

    #[test]
    fn test_agent_types_equality() {
        assert_eq!(AgentType::Acp, AgentType::Acp);
        assert_eq!(AgentType::Internal, AgentType::Internal);
        assert_ne!(AgentType::Acp, AgentType::Internal);
    }

    #[test]
    fn test_agent_init_params_with_env_overrides() {
        use std::collections::HashMap;

        let mut env = HashMap::new();
        env.insert(
            "LOCAL_ENDPOINT".to_string(),
            "http://localhost:11434".to_string(),
        );
        env.insert("ANTHROPIC_MODEL".to_string(), "claude-3-opus".to_string());

        let params = AgentInitParams::new()
            .with_type(AgentType::Acp)
            .with_env_overrides(env.clone());

        assert_eq!(params.env_overrides.len(), 2);
        assert_eq!(
            params.env_overrides.get("LOCAL_ENDPOINT"),
            Some(&"http://localhost:11434".to_string())
        );
        assert_eq!(
            params.env_overrides.get("ANTHROPIC_MODEL"),
            Some(&"claude-3-opus".to_string())
        );
    }

    #[test]
    fn test_agent_init_params_default_has_empty_env_overrides() {
        let params = AgentInitParams::default();
        assert!(params.env_overrides.is_empty());
    }
}
