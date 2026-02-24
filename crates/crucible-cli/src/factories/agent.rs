//! Agent factory - creates AgentHandle via daemon
//!
//! All agents route through the daemon (auto-started if needed).
//! Supports ACP (external) agents and internal (direct LLM) agents.
//! Selection priority:
//! 1. Explicit `-a <name>` CLI flag
//! 2. Config file setting (chat.agent_preference)
//! 3. Default: Internal (Crucible's built-in Rig-based agents)

use anyhow::Result;
use tracing::info;

use crucible_config::{BackendType, CliAppConfig};
use crucible_core::session::SessionAgent;
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
    /// Resume an existing daemon session instead of creating a new one.
    /// If Some(session_id), resume that specific session.
    /// If Some(""), resume most recent session for the workspace.
    pub resume_session_id: Option<String>,
    /// Recording mode for the session ("granular" or "coarse")
    pub recording_mode: Option<String>,
    /// Custom path for the recording output file
    pub recording_path: Option<std::path::PathBuf>,
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
            resume_session_id: None,
            recording_mode: None,
            recording_path: None,
        }
    }

    pub fn with_resume_session_id(mut self, session_id: Option<String>) -> Self {
        self.resume_session_id = session_id;
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

    pub fn with_recording_mode(mut self, mode: Option<String>) -> Self {
        self.recording_mode = mode;
        self
    }

    pub fn with_recording_path(mut self, path: Option<std::path::PathBuf>) -> Self {
        self.recording_path = path;
        self
    }
}

impl Default for AgentInitParams {
    fn default() -> Self {
        Self::new()
    }
}

fn build_acp_session_agent(params: &AgentInitParams, config: &CliAppConfig) -> SessionAgent {
    let delegation_config = params
        .agent_name
        .as_ref()
        .and_then(|agent_name| config.acp.agents.get(agent_name))
        .and_then(|profile| profile.delegation.clone());

    SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: params.agent_name.clone(),
        provider_key: None,
        provider: BackendType::Custom,
        model: String::new(),
        system_prompt: String::new(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: params.env_overrides.clone(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config,
        precognition_enabled: true,
    }
}

fn build_internal_session_agent(config: &CliAppConfig) -> SessionAgent {
    let effective_llm = config.effective_llm_provider().ok();
    let model = effective_llm
        .as_ref()
        .map(|p| p.model.clone())
        .or_else(|| config.chat.model.clone())
        .unwrap_or_else(|| crucible_config::DEFAULT_CHAT_MODEL.to_string());
    let mcp_servers = config
        .mcp
        .as_ref()
        .map(|mcp| mcp.servers.iter().map(|s| s.name.clone()).collect())
        .unwrap_or_default();
    let backend_type = effective_llm
        .as_ref()
        .map(|p| p.provider_type)
        .unwrap_or(crucible_config::BackendType::Ollama);
    let provider_key = effective_llm
        .as_ref()
        .map(|p| p.key.clone())
        .unwrap_or_else(|| backend_type.as_str().to_string());

    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: None,
        provider_key: Some(provider_key),
        provider: backend_type,
        model,
        system_prompt: String::new(),
        temperature: effective_llm
            .as_ref()
            .map(|p| p.temperature as f64)
            .or_else(|| config.chat.temperature.map(|t| t as f64)),
        max_tokens: effective_llm
            .as_ref()
            .map(|p| p.max_tokens)
            .or(config.chat.max_tokens),
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: effective_llm
            .as_ref()
            .map(|p| p.endpoint.clone())
            .or_else(|| config.chat.endpoint.clone()),
        env_overrides: std::collections::HashMap::new(),
        mcp_servers,
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: true,
    }
}

/// Create an agent via daemon (auto-starts daemon if needed)
pub async fn create_daemon_agent(
    config: &CliAppConfig,
    params: &AgentInitParams,
) -> Result<Box<dyn AgentHandle + Send + Sync>> {
    use crucible_rpc::{DaemonAgentHandle, DaemonClient};
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
                    create_new_daemon_session(
                        &client,
                        config,
                        &workspace,
                        params.recording_mode.as_deref(),
                        params.recording_path.as_deref(),
                    )
                    .await?,
                    true,
                )
            }
        }
        None => (
            create_new_daemon_session(
                &client,
                config,
                &workspace,
                params.recording_mode.as_deref(),
                params.recording_path.as_deref(),
            )
            .await?,
            true,
        ),
    };

    let is_acp = params
        .agent_type
        .map(|t| t == AgentType::Acp)
        .unwrap_or_else(|| config.chat.agent_preference == crucible_config::AgentPreference::Acp);

    let session_agent = if is_new_session {
        let agent = if is_acp {
            build_acp_session_agent(params, config)
        } else {
            build_internal_session_agent(config)
        };
        client.session_configure_agent(&session_id, &agent).await?;
        Some(agent)
    } else {
        None
    };
    info!(
        session_id = %session_id,
        resumed = !is_new_session,
        "Daemon agent handle ready"
    );
    let handle = DaemonAgentHandle::new_and_subscribe(client, session_id, event_rx)
        .await?
        .with_kiln_path(config.kiln_path.clone())
        .with_workspace(workspace.clone());
    let handle = match session_agent {
        Some(agent) => handle.with_agent_config(agent),
        None => handle,
    };

    Ok(Box::new(handle))
}

async fn create_new_daemon_session(
    client: &crucible_rpc::DaemonClient,
    config: &CliAppConfig,
    workspace: &std::path::Path,
    recording_mode: Option<&str>,
    recording_path: Option<&std::path::Path>,
) -> Result<String> {
    let result = client
        .session_create(
            "chat",
            &config.kiln_path,
            Some(workspace),
            vec![],
            recording_mode,
            recording_path,
        )
        .await?;

    let session_id = result["session_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No session_id in response"))?
        .to_string();

    info!("Created new daemon session: {}", session_id);
    Ok(session_id)
}

/// Returns `(no-op agent handle, replay_session_id, event_rx)`.
/// Real events flow through `event_rx` to a consumer task; agent handle is for type compatibility.
pub async fn create_daemon_replay_agent(
    replay_path: &std::path::Path,
    speed: f64,
) -> Result<(
    Box<dyn AgentHandle + Send + Sync>,
    String,
    tokio::sync::mpsc::UnboundedReceiver<crucible_rpc::SessionEvent>,
)> {
    use crucible_rpc::{DaemonAgentHandle, DaemonClient};
    use std::sync::Arc;

    info!("Connecting to daemon for replay session");
    let (client, event_rx) = DaemonClient::connect_or_start_with_events()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;

    let replay_response = client
        .session_replay(replay_path, speed)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start daemon replay: {}", e))?;

    let replay_session_id = replay_response
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No session_id in replay response"))?
        .to_string();

    let client = Arc::new(client);

    client
        .session_subscribe(&[&replay_session_id])
        .await
        .map_err(|e| anyhow::anyhow!("Failed to subscribe to replay session: {}", e))?;

    info!(
        session_id = %replay_session_id,
        speed = speed,
        "Daemon replay session created and subscribed"
    );

    let (_, dummy_rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = DaemonAgentHandle::new(client, replay_session_id.clone(), dummy_rx);

    Ok((Box::new(handle), replay_session_id, event_rx))
}

/// Create an agent via daemon (auto-starts if needed).
pub async fn create_agent(
    config: &CliAppConfig,
    params: AgentInitParams,
) -> Result<Box<dyn AgentHandle + Send + Sync>> {
    create_daemon_agent(config, &params).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_config::{AgentProfile, DelegationConfig};

    fn test_delegation_config() -> DelegationConfig {
        DelegationConfig {
            enabled: true,
            max_depth: 2,
            allowed_targets: Some(vec!["tool-agent".to_string(), "search-agent".to_string()]),
            result_max_bytes: 102400,
            max_concurrent_delegations: 4,
        }
    }

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
    fn test_build_acp_session_agent_includes_delegation_config_when_present() {
        let mut config = CliAppConfig::default();
        config.acp.agents.insert(
            "delegating-agent".to_string(),
            AgentProfile {
                delegation: Some(test_delegation_config()),
                ..Default::default()
            },
        );

        let params = AgentInitParams::default().with_agent_name("delegating-agent");
        let session_agent = build_acp_session_agent(&params, &config);

        assert_eq!(
            session_agent.delegation_config,
            Some(test_delegation_config())
        );
    }

    #[test]
    fn test_build_acp_session_agent_omits_delegation_config_when_missing() {
        let config = CliAppConfig::default();
        let params = AgentInitParams::default().with_agent_name("non-delegating-agent");

        let session_agent = build_acp_session_agent(&params, &config);

        assert_eq!(session_agent.delegation_config, None);
    }
}
