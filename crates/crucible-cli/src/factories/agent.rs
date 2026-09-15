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

use crucible_core::config::CliAppConfig;
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
    /// Card discovery and composition belong to session.create in the daemon.
    pub agent_card: Option<String>,
    /// Preferred LLM provider key (for internal type)
    pub provider_key: Option<String>,
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
            agent_card: None,
            provider_key: None,
            env_overrides: std::collections::HashMap::new(),
            working_dir: None,
            resume_session_id: None,
            recording_mode: None,
            recording_path: None,
        }
    }

    pub fn with_agent_card(mut self, card: Option<String>) -> Self {
        self.agent_card = card;
        self
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

    #[cfg(test)]
    pub fn with_provider(mut self, key: impl Into<String>) -> Self {
        self.provider_key = Some(key.into());
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

/// Create an agent via daemon (auto-starts daemon if needed)
pub async fn create_daemon_agent(
    config: &CliAppConfig,
    params: &AgentInitParams,
) -> Result<Box<dyn AgentHandle + Send + Sync>> {
    let (handle, _session_id, _raw_rx) = create_daemon_agent_inner(config, params, false).await?;
    Ok(handle)
}

/// Like [`create_daemon_agent`], but also returns the raw SessionEvent
/// receiver for the session. Used by the live TUI, which consumes
/// SessionEvents directly instead of through `Agent::turn`.
pub async fn create_daemon_agent_with_events(
    config: &CliAppConfig,
    params: &AgentInitParams,
) -> Result<(
    Box<dyn AgentHandle + Send + Sync>,
    String,
    tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>,
)> {
    let (handle, session_id, raw_rx) = create_daemon_agent_inner(config, params, true).await?;
    let raw_rx = raw_rx.ok_or_else(|| {
        anyhow::anyhow!("Raw event receiver missing from daemon handle (internal error)")
    })?;
    Ok((handle, session_id, raw_rx))
}

/// The single internal-vs-ACP rule, shared by every entry point (interactive
/// chat, one-shot chat, `session create`, config preference). A present agent
/// name (`chat -a claude`) implies ACP — previously only the interactive path
/// set the ACP type, so one-shot `chat -a <agent>` silently ran the internal
/// agent.
pub(crate) fn resolve_is_acp(
    agent_type: Option<AgentType>,
    agent_name: Option<&str>,
    preference: &crucible_core::config::AgentPreference,
) -> bool {
    agent_type == Some(AgentType::Acp)
        || agent_name.is_some()
        || *preference == crucible_core::config::AgentPreference::Acp
}

async fn create_daemon_agent_inner(
    config: &CliAppConfig,
    params: &AgentInitParams,
    raw_forwarding: bool,
) -> Result<(
    Box<dyn AgentHandle + Send + Sync>,
    String,
    Option<tokio::sync::mpsc::UnboundedReceiver<crucible_daemon::SessionEvent>>,
)> {
    use crucible_daemon::DaemonAgentHandle;
    use std::sync::Arc;

    info!("Connecting to daemon (auto-start if needed)");
    let (client, event_rx) = crate::common::daemon_client_with_events()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;

    let client = Arc::new(client);

    // Subscribe-first: wildcard-subscribe BEFORE session.create so the
    // setup task's events (emitted the moment the session is registered)
    // are not missed by the race where the client subscribes after create
    // returns. The secondary specific-session subscribe later in
    // `new_and_subscribe` is idempotent.
    //
    // If this fails, the TUI would hang on "Loading..." forever waiting on
    // setup events that never arrive, so propagate the error and let the
    // CLI exit with a clear message.
    client
        .session_subscribe(&["*"])
        .await
        .map_err(|e| anyhow::anyhow!("failed to subscribe to session events: {}", e))?;

    let workspace = params
        .working_dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| config.kiln_path.clone()));

    // Compute up-front so `session.create` can tell the daemon which agent type
    // this will be (Task 1.2f's setup task branches on it).
    let is_acp = params.agent_card.is_none()
        && resolve_is_acp(
            params.agent_type,
            params.agent_name.as_deref(),
            &config.chat.agent_preference,
        );
    let create_agent_type = if is_acp { "acp" } else { "internal" };

    let session_id = match &params.resume_session_id {
        Some(id) if !id.is_empty() => {
            info!("Resuming specific daemon session: {}", id);
            match client.session_resume(id).await {
                Ok(_) => {}
                Err(e) => {
                    info!("Session resume skipped (may already be active): {}", e);
                }
            }
            id.clone()
        }
        Some(_) => {
            let session_kiln = config.session_kiln_name();
            let sessions = client
                .session_list(
                    session_kiln.as_ref(),
                    Some(&workspace),
                    Some("chat"),
                    Some("active"),
                    None,
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
                id
            } else {
                info!("No existing session to resume, creating new one");
                create_new_daemon_session(&client, config, &workspace, params, create_agent_type)
                    .await?
            }
        }
        None => {
            create_new_daemon_session(&client, config, &workspace, params, create_agent_type)
                .await?
        }
    };

    info!(
        session_id = %session_id,
        "Daemon agent handle ready"
    );
    let mut handle = if raw_forwarding {
        DaemonAgentHandle::new_and_subscribe_with_raw_forwarding(
            client,
            session_id.clone(),
            event_rx,
        )
        .await?
    } else {
        DaemonAgentHandle::new_and_subscribe(client, session_id.clone(), event_rx).await?
    }
    .with_kiln(config.session_kiln_name())
    .with_workspace(workspace.clone());

    let raw_rx = if raw_forwarding {
        handle.take_raw_event_receiver()
    } else {
        None
    };

    Ok((Box::new(handle), session_id, raw_rx))
}

async fn create_new_daemon_session(
    client: &crucible_daemon::DaemonClient,
    config: &CliAppConfig,
    workspace: &std::path::Path,
    params: &AgentInitParams,
    agent_type: &str,
) -> Result<String> {
    let create = crucible_daemon::rpc_client::SessionCreateParams {
        session_type: "chat".into(),
        kilns: config.session_kiln_name().into_iter().collect(),
        workspace: Some(workspace.to_path_buf()),
        recording_mode: params.recording_mode.clone(),
        recording_path: params.recording_path.clone(),
        agent_type: Some(agent_type.into()),
        isolation: None,
    };
    let legacy_chat_defaults = agent_type == "internal"
        && params.provider_key.is_none()
        && config.llm.default_provider().is_none();
    let result = client
        .session_create_with_agent(
            create,
            crucible_daemon::rpc_client::SessionAgentSpec {
                agent_name: (agent_type == "acp")
                    .then(|| {
                        params
                            .agent_name
                            .clone()
                            .or_else(|| config.acp.default_agent.clone())
                    })
                    .flatten(),
                agent_card: params.agent_card.clone(),
                provider_key: params.provider_key.clone(),
                env_overrides: params.env_overrides.clone(),
                // Legacy [chat] fallbacks are client config inputs, not a second
                // SessionAgent constructor. Provider and card defaults stay daemon-owned.
                model: legacy_chat_defaults
                    .then(|| config.chat.model.clone())
                    .flatten(),
                endpoint: legacy_chat_defaults
                    .then(|| config.chat.endpoint.clone())
                    .flatten(),
                ..Default::default()
            },
        )
        .await?;

    let session_id = result["session_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No session_id in response"))?
        .to_string();

    info!("Created new daemon session: {}", session_id);
    Ok(session_id)
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
    use crucible_core::config::AgentPreference;

    #[tokio::test]
    async fn chat_creation_sends_agent_selection_in_one_rpc() {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        let tmp = tempfile::tempdir().unwrap();
        let socket = tmp.path().join("daemon.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            let mut requests = Vec::new();
            while let Some(line) = lines.next_line().await.unwrap() {
                let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                let response = serde_json::json!({"jsonrpc":"2.0", "id":req["id"],
                    "result":{"session_id":"fixture"}});
                write
                    .write_all(format!("{response}\n").as_bytes())
                    .await
                    .unwrap();
                requests.push(req);
                if requests.len() == 3 {
                    break;
                }
            }
            requests
        });
        let client = crucible_daemon::DaemonClient::connect_to(&socket)
            .await
            .unwrap();
        let mut config = CliAppConfig::default();
        config.chat.model = Some("legacy-model".into());
        config.chat.endpoint = Some("http://legacy.test".into());
        let cases = [
            ("internal", AgentInitParams::default()),
            (
                "internal",
                AgentInitParams::default().with_provider("chosen"),
            ),
            (
                "acp",
                AgentInitParams::default()
                    .with_agent_name("opencode")
                    .with_model("explicit-model"),
            ),
        ];
        for (kind, params) in cases {
            let id = create_new_daemon_session(&client, &config, tmp.path(), &params, kind)
                .await
                .unwrap();
            assert_eq!(id, "fixture");
        }
        let requests = server.await.unwrap();
        for request in &requests {
            assert_eq!(request["method"], "session.create");
            assert_eq!(request["params"]["configure_agent"], true);
        }
        assert_eq!(requests[0]["params"]["model"], "legacy-model");
        assert_eq!(requests[0]["params"]["endpoint"], "http://legacy.test");
        assert_eq!(requests[1]["params"]["provider_key"], "chosen");
        assert!(requests[1]["params"].get("model").is_none());
        assert_eq!(requests[2]["params"]["agent_name"], "opencode");
        assert_eq!(
            requests[2]["params"]["env_overrides"]["OPENCODE_MODEL"],
            "explicit-model"
        );
    }

    #[test]
    fn is_acp_rule_converges_across_entry_points() {
        // A named agent (`chat -a claude`) implies ACP even with the default
        // Crucible preference and no explicit type — the one-shot bug.
        assert!(resolve_is_acp(
            None,
            Some("claude"),
            &AgentPreference::Crucible
        ));
        // Explicit ACP type.
        assert!(resolve_is_acp(
            Some(AgentType::Acp),
            None,
            &AgentPreference::Crucible
        ));
        // Config preference.
        assert!(resolve_is_acp(None, None, &AgentPreference::Acp));
        // Bare `cru chat`: no name, no type, default preference → internal.
        assert!(!resolve_is_acp(None, None, &AgentPreference::Crucible));
    }

    #[test]
    fn test_agent_type_default() {
        assert_eq!(AgentType::default(), AgentType::Internal);
    }

    #[test]
    fn test_agent_init_params_builder() {
        let params = AgentInitParams::new()
            .with_type(AgentType::Internal)
            .with_provider("local".to_string());

        assert_eq!(params.agent_type, Some(AgentType::Internal));
        assert_eq!(params.provider_key, Some("local".to_string()));
    }

    #[test]
    fn test_agent_init_params_default() {
        let params = AgentInitParams::default();
        assert_eq!(params.agent_type, None);
        assert_eq!(params.agent_name, None);
        assert_eq!(params.provider_key, None);
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
}
