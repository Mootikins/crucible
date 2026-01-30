//! Agent factory - creates AgentHandle from configuration
//!
//! Supports both ACP (external) agents and internal (direct LLM) agents.
//! Selection priority:
//! 1. Explicit CLI flag (--internal or --acp)
//! 2. Config file setting (chat.agent_preference)
//! 3. Default: Internal (Crucible's built-in Rig-based agents)
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
    Acp,
    /// Internal direct LLM agent (Crucible's built-in Rig-based agents)
    #[default]
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
    /// Optional kiln context for knowledge base access
    /// When provided, agent will have semantic_search, read_note, list_notes tools
    pub kiln_context: Option<crucible_rig::KilnContext>,
    /// Force local agent execution (skip daemon).
    /// Default (None): try daemon first, fall back to local.
    /// Some(true): skip daemon, use local agent directly.
    pub force_local: Option<bool>,
    /// Resume an existing daemon session instead of creating a new one.
    /// If Some(session_id), resume that specific session.
    /// If Some(""), resume most recent session for the workspace.
    pub resume_session_id: Option<String>,
}

impl AgentInitParams {
    pub fn new() -> Self {
        Self {
            agent_type: None,
            agent_name: None,
            provider_key: None,
            read_only: false,
            max_context_tokens: None,
            env_overrides: std::collections::HashMap::new(),
            working_dir: None,
            kiln_context: None,
            force_local: None,
            resume_session_id: None,
        }
    }

    pub fn with_force_local(mut self, force: bool) -> Self {
        self.force_local = Some(force);
        self
    }

    pub fn with_resume_session_id(mut self, session_id: Option<String>) -> Self {
        self.resume_session_id = session_id;
        self
    }

    /// Set kiln context for knowledge base access
    ///
    /// When provided, the internal agent will have access to kiln tools:
    /// - semantic_search: Search notes using embeddings
    /// - read_note: Read note content from the kiln
    /// - list_notes: List notes in a directory
    pub fn with_kiln_context(mut self, ctx: crucible_rig::KilnContext) -> Self {
        self.kiln_context = Some(ctx);
        self
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

/// Check if a model likely supports reasoning_content in its streaming responses
///
/// Models that use extended thinking (Qwen3 with thinking, DeepSeek-R1, etc.)
/// return their reasoning in a `reasoning_content` field that requires custom
/// SSE parsing to extract.
fn supports_reasoning_content(model_name: &str) -> bool {
    let name_lower = model_name.to_lowercase();
    // Qwen3 thinking variants
    name_lower.contains("qwen3") && name_lower.contains("thinking")
        // DeepSeek R1 reasoning models
        || name_lower.contains("deepseek") && name_lower.contains("r1")
        // Any model with explicit "reasoning" in name
        || name_lower.contains("reasoning")
}

/// Create an internal agent using the Rig framework
pub async fn create_internal_agent(
    config: &CliAppConfig,
    params: AgentInitParams,
) -> Result<Box<dyn AgentHandle + Send + Sync>> {
    use crucible_config::LlmProvider;
    use crucible_context::{LayeredPromptBuilder, PromptBuilder};
    use crucible_core::prompts::{base_prompt_for_size, ModelSize};
    use crucible_rig::{build_agent_with_kiln_tools, AgentComponents, RigAgentHandle};

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

    // Detect model size (or use Medium if size-aware prompts disabled)
    let model_size = if config.chat.size_aware_prompts {
        let detected = ModelSize::from_model_name(&model);
        info!("Detected model size: {:?} for {}", detected, model);
        detected
    } else {
        debug!("Size-aware prompts disabled, using standard prompts and all tools");
        ModelSize::Medium
    };

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
    let workspace_root = params
        .working_dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| config.kiln_path.clone()));

    // Add project rules if workspace has them (AGENTS.md, .rules, etc.)
    // Use configured rules_files or defaults
    let rules_files = config
        .context
        .as_ref()
        .map(|c| c.rules_files.clone())
        .unwrap_or_else(|| {
            vec![
                "AGENTS.md".to_string(),
                ".rules".to_string(),
                ".github/copilot-instructions.md".to_string(),
            ]
        });
    prompt_builder = prompt_builder.with_project_rules_hierarchical(&workspace_root, &rules_files);

    // Skill context injection (eager mode)
    // Discover skills from standard paths and inject descriptions into context
    let skill_discovery =
        crucible_skills::FolderDiscovery::with_default_paths(&workspace_root, None);
    match skill_discovery.discover() {
        Ok(skills) if !skills.is_empty() => {
            let skill_context = crucible_skills::format_skills_for_context(&skills);
            debug!(
                "Injecting {} skills into context ({} chars)",
                skills.len(),
                skill_context.len()
            );
            prompt_builder.add_layer(crucible_context::priorities::SKILL, "skills", skill_context);
        }
        Ok(_) => {
            debug!("No skills found in default paths");
        }
        Err(e) => {
            tracing::warn!("Skill discovery failed, continuing without skills: {}", e);
        }
    }

    let system_prompt = prompt_builder.build();

    let mut agent_config = crucible_rig::AgentConfig::new(&model, &system_prompt);
    if let Some(temp) = config.chat.temperature {
        agent_config = agent_config.with_temperature(temp as f64);
    }
    if let Some(tokens) = config.chat.max_tokens {
        agent_config = agent_config.with_max_tokens(tokens);
    }

    use crucible_config::{LlmProviderConfig, LlmProviderType};

    let mut reasoning_endpoint: Option<String> = None;
    let mut ollama_endpoint: Option<String> = None;

    let client = match config.chat.provider {
        LlmProvider::Ollama => {
            let endpoint = config
                .chat
                .endpoint
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string());

            ollama_endpoint = Some(endpoint.clone());

            // For custom Ollama endpoints (not localhost), use OpenAI-compatible client
            // This provides better tool calling support via /v1/chat/completions
            let is_local = endpoint.contains("localhost") || endpoint.contains("127.0.0.1");

            if is_local {
                // Local Ollama - check for reasoning support
                if supports_reasoning_content(&model) {
                    // For local Ollama, the OpenAI-compatible endpoint is at /v1
                    reasoning_endpoint = Some(format!("{}/v1", endpoint.trim_end_matches('/')));
                    info!("Enabling reasoning extraction for model: {}", model);
                }
                // Local Ollama - use native client
                crucible_rig::create_client(
                    &LlmProviderConfig::builder(LlmProviderType::Ollama)
                        .endpoint(endpoint)
                        .model(model.clone())
                        .maybe_timeout_secs(config.chat.timeout_secs)
                        .build(),
                )?
            } else {
                // Remote Ollama-compatible (e.g., llama-swappo) - use OpenAI-compatible client
                // Append /v1 if not already present for OpenAI-compatible endpoint
                let compat_endpoint = if endpoint.ends_with("/v1") {
                    endpoint.clone()
                } else {
                    format!("{}/v1", endpoint.trim_end_matches('/'))
                };

                // Check for reasoning support
                if supports_reasoning_content(&model) {
                    reasoning_endpoint = Some(compat_endpoint.clone());
                    info!("Enabling reasoning extraction for model: {}", model);
                }

                info!(
                    "Using OpenAI-compatible endpoint for remote Ollama: {}",
                    compat_endpoint
                );
                crucible_rig::create_client(
                    &LlmProviderConfig::builder(LlmProviderType::OpenAI)
                        .endpoint(compat_endpoint)
                        .model(model.clone())
                        .maybe_timeout_secs(config.chat.timeout_secs)
                        .build(),
                )?
            }
        }
        LlmProvider::OpenAI => {
            agent_config = agent_config
                .with_additional_params(serde_json::json!({"parallel_tool_calls": true}));
            crucible_rig::create_client(
                &LlmProviderConfig::builder(LlmProviderType::OpenAI)
                    .maybe_endpoint(config.chat.endpoint.clone())
                    .model(model.clone())
                    .maybe_timeout_secs(config.chat.timeout_secs)
                    .api_key_from_env()
                    .build(),
            )?
        }
        LlmProvider::Anthropic => crucible_rig::create_client(
            &LlmProviderConfig::builder(LlmProviderType::Anthropic)
                .maybe_endpoint(config.chat.endpoint.clone())
                .model(model.clone())
                .maybe_timeout_secs(config.chat.timeout_secs)
                .api_key_from_env()
                .build(),
        )?,
    };

    let has_kiln = params.kiln_context.is_some();
    info!(
        "Building Rig agent with {:?} tools{} for: {}",
        model_size,
        if has_kiln { " + kiln tools" } else { "" },
        workspace_root.display()
    );

    let initial_mode = if params.read_only { "plan" } else { "normal" };

    // Build Rig agent with size-appropriate tools (and kiln tools if context provided)
    let kiln_ctx = params.kiln_context;
    match client {
        crucible_rig::RigClient::Ollama(ollama_client) => {
            let (agent, ws_ctx) = build_agent_with_kiln_tools(
                &agent_config,
                &ollama_client,
                &workspace_root,
                model_size,
                kiln_ctx.clone(),
            )?;

            let mut components = AgentComponents::new(
                agent_config.clone(),
                crucible_rig::RigClient::Ollama(ollama_client),
                ws_ctx.clone(),
            )
            .with_model_size(model_size);

            if let Some(kc) = kiln_ctx {
                components = components.with_kiln(kc);
            }
            if let Some(ref endpoint) = ollama_endpoint {
                components = components.with_ollama_endpoint(endpoint.clone());
            }

            let mut handle = RigAgentHandle::new(agent)
                .with_ollama_components(components)
                .with_initial_mode(initial_mode);

            if let Some(endpoint) = reasoning_endpoint {
                handle = handle.with_reasoning_endpoint(endpoint, model);
            }
            Ok(Box::new(handle))
        }
        crucible_rig::RigClient::OpenAI(openai_client) => {
            let (agent, ws_ctx) = build_agent_with_kiln_tools(
                &agent_config,
                &openai_client,
                &workspace_root,
                model_size,
                kiln_ctx,
            )?;
            let mut handle = RigAgentHandle::new(agent)
                .with_workspace_context(ws_ctx)
                .with_initial_mode(initial_mode)
                .with_model(model.clone());
            if let Some(endpoint) = ollama_endpoint.clone() {
                handle = handle.with_ollama_endpoint(endpoint);
            }
            if let Some(endpoint) = reasoning_endpoint {
                handle = handle.with_reasoning_endpoint(endpoint, model);
            }
            Ok(Box::new(handle))
        }
        crucible_rig::RigClient::OpenAICompat(compat_client) => {
            let (agent, ws_ctx) = build_agent_with_kiln_tools(
                &agent_config,
                &compat_client,
                &workspace_root,
                model_size,
                kiln_ctx,
            )?;
            let mut handle = RigAgentHandle::new(agent)
                .with_workspace_context(ws_ctx)
                .with_initial_mode(initial_mode)
                .with_model(model.clone());
            if let Some(endpoint) = ollama_endpoint.clone() {
                handle = handle.with_ollama_endpoint(endpoint);
            }
            if let Some(endpoint) = reasoning_endpoint {
                handle = handle.with_reasoning_endpoint(endpoint, model);
            }
            Ok(Box::new(handle))
        }
        crucible_rig::RigClient::Anthropic(anthropic_client) => {
            let (agent, ws_ctx) = build_agent_with_kiln_tools(
                &agent_config,
                &anthropic_client,
                &workspace_root,
                model_size,
                kiln_ctx,
            )?;
            Ok(Box::new(
                RigAgentHandle::new(agent)
                    .with_workspace_context(ws_ctx)
                    .with_initial_mode(initial_mode),
            ))
        }
        crucible_rig::RigClient::GitHubCopilot(copilot_client) => {
            let api_token = copilot_client
                .api_token()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get Copilot API token: {}", e))?;
            let api_base = copilot_client
                .api_base()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get Copilot API base: {}", e))?;

            let compat_client = crucible_rig::create_openai_compat_client(&api_token, &api_base)?;

            let (agent, ws_ctx) = build_agent_with_kiln_tools(
                &agent_config,
                &compat_client,
                &workspace_root,
                model_size,
                kiln_ctx,
            )?;
            let handle = RigAgentHandle::new(agent)
                .with_workspace_context(ws_ctx)
                .with_initial_mode(initial_mode);
            Ok(if let Some(endpoint) = reasoning_endpoint {
                Box::new(handle.with_reasoning_endpoint(endpoint, model))
            } else {
                Box::new(handle)
            })
        }
    }
}

/// Create an agent via daemon (auto-starts daemon if needed)
pub async fn create_daemon_agent(
    config: &CliAppConfig,
    params: &AgentInitParams,
) -> Result<Box<dyn AgentHandle + Send + Sync>> {
    use crucible_core::session::SessionAgent;
    use crucible_daemon_client::{DaemonAgentHandle, DaemonClient};
    use std::sync::Arc;

    info!("Connecting to daemon (auto-start if needed)");
    let (client, event_rx) = DaemonClient::connect_or_start_with_events()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;

    let client = Arc::new(client);

    let workspace = params
        .working_dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| config.kiln_path.clone()));

    let (session_id, is_new_session) = match &params.resume_session_id {
        Some(id) if !id.is_empty() => {
            info!("Resuming specific daemon session: {}", id);
            match client.session_resume(id).await {
                Ok(_) => {}
                Err(e) => {
                    info!("Session resume skipped (may already be active): {}", e);
                }
            }
            (id.clone(), false)
        }
        Some(_) => {
            let sessions = client
                .session_list(
                    Some(&config.kiln_path),
                    Some(&workspace),
                    Some("chat"),
                    Some("active"),
                )
                .await?;

            let empty = vec![];
            let sessions = sessions.as_array().unwrap_or(&empty);
            if let Some(session) = sessions.first() {
                let id = session["session_id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid session data"))?
                    .to_string();
                info!("Resuming most recent daemon session: {}", id);
                client.session_resume(&id).await?;
                (id, false)
            } else {
                info!("No existing session to resume, creating new one");
                (
                    create_new_daemon_session(&client, config, &workspace).await?,
                    true,
                )
            }
        }
        None => (
            create_new_daemon_session(&client, config, &workspace).await?,
            true,
        ),
    };

    if is_new_session {
        let model = config
            .chat
            .model
            .clone()
            .unwrap_or_else(|| "llama3.2".to_string());

        let session_agent = SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some(format!("{:?}", config.chat.provider).to_lowercase()),
            provider: format!("{:?}", config.chat.provider).to_lowercase(),
            model: model.clone(),
            system_prompt: String::new(),
            temperature: config.chat.temperature.map(|t| t as f64),
            max_tokens: config.chat.max_tokens,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: config.chat.endpoint.clone(),
            env_overrides: std::collections::HashMap::new(),
            mcp_servers: vec![],
            agent_card_name: None,
        };

        client
            .session_configure_agent(&session_id, &session_agent)
            .await?;
    }

    info!(
        session_id = %session_id,
        resumed = !is_new_session,
        "Daemon agent handle ready"
    );
    let handle = DaemonAgentHandle::new_and_subscribe(client, session_id, event_rx).await?;

    Ok(Box::new(handle))
}

async fn create_new_daemon_session(
    client: &crucible_daemon_client::DaemonClient,
    config: &CliAppConfig,
    workspace: &std::path::Path,
) -> Result<String> {
    let result = client
        .session_create("chat", &config.kiln_path, Some(workspace), vec![])
        .await?;

    let session_id = result["session_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No session_id in response"))?
        .to_string();

    info!("Created new daemon session: {}", session_id);
    Ok(session_id)
}

/// Create an agent based on configuration and parameters
///
/// Routing priority:
/// 1. If force_local=true, skip daemon entirely
/// 2. Try daemon first (auto-start if needed)
/// 3. Fall back to local agent on daemon failure
///
/// Agent type priority:
/// 1. Explicit agent_type in params
/// 2. Config file setting (chat.agent_preference)
/// 3. Default: Internal
pub async fn create_agent(
    config: &CliAppConfig,
    params: AgentInitParams,
) -> Result<InitializedAgent> {
    use crucible_config::AgentPreference;

    // Try daemon first unless force_local is set
    let use_daemon = !params.force_local.unwrap_or(false);

    if use_daemon {
        match create_daemon_agent(config, &params).await {
            Ok(handle) => {
                info!("Using daemon-backed agent");
                return Ok(InitializedAgent::Internal(handle));
            }
            Err(e) => {
                tracing::warn!("Daemon agent creation failed, falling back to local: {}", e);
            }
        }
    }

    // Determine agent type from params or config
    let agent_type = params
        .agent_type
        .unwrap_or(match config.chat.agent_preference {
            AgentPreference::Crucible => AgentType::Internal,
            AgentPreference::Acp => AgentType::Acp,
        });

    match agent_type {
        AgentType::Internal => {
            info!("Initializing local internal agent");
            let handle = create_internal_agent(config, params).await?;
            Ok(InitializedAgent::Internal(handle))
        }
        AgentType::Acp => {
            info!("Initializing ACP agent");
            use crate::acp::{discover_agent, CrucibleAcpClient};

            let agent_name = params
                .agent_name
                .or_else(|| config.acp.default_agent.clone());
            let mut agent = discover_agent(agent_name.as_deref()).await?;

            debug!("Discovered agent: {}", agent.name);

            if let Some(profile) = config.acp.agents.get(&agent.name) {
                if !profile.env.is_empty() {
                    let keys: Vec<_> = profile.env.keys().collect();
                    info!("Applying config profile env vars: {:?}", keys);
                    agent.env_vars.extend(profile.env.clone());
                }
            }

            if !params.env_overrides.is_empty() {
                let keys: Vec<_> = params.env_overrides.keys().collect();
                info!("Applying CLI env overrides: {:?}", keys);
                agent.env_vars.extend(params.env_overrides);
            }

            let mut client =
                CrucibleAcpClient::with_acp_config(agent, params.read_only, config.acp.clone());

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
        assert_eq!(AgentType::default(), AgentType::Internal);
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
        assert!(!params.read_only);
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

    #[test]
    fn test_supports_reasoning_content() {
        // Qwen3 thinking models
        assert!(supports_reasoning_content("qwen3-4b-thinking-2507-q8_0"));
        assert!(supports_reasoning_content("Qwen3-8B-Thinking"));
        assert!(supports_reasoning_content("qwen3-thinking-32b"));

        // DeepSeek R1 models
        assert!(supports_reasoning_content("deepseek-r1-8b"));
        assert!(supports_reasoning_content("DeepSeek-R1-Distill"));

        // Generic reasoning in name
        assert!(supports_reasoning_content("my-reasoning-model"));

        // Not reasoning models
        assert!(!supports_reasoning_content("qwen3-4b-instruct")); // qwen3 but no thinking
        assert!(!supports_reasoning_content("llama3.2"));
        assert!(!supports_reasoning_content("gpt-4o"));
        assert!(!supports_reasoning_content("claude-3-5-sonnet"));
    }
}
