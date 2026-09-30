//! Open a daemon session for `cru chat`.
//!
//! The daemon owns the session and its agent. This module asks the daemon to
//! create or resume a session, and gives the caller the client, the session id
//! and the event stream. It builds no agent on the client side.
//!
//! The agent is selected in this order:
//! 1. The explicit `-a <name>` flag.
//! 2. The config setting `chat.agent_preference`.
//! 3. The internal agent.

use crucible_core::protocol::requests::SessionCreateRequest;
use std::sync::Arc;

use anyhow::Result;
use tracing::info;

use crucible_core::config::CliAppConfig;
use crucible_core::interaction::{InteractionEvent, InteractionRequest};
use crucible_daemon::{DaemonClient, SessionEvent};
use tokio::sync::mpsc::UnboundedReceiver;

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

/// A daemon session that this client is attached to.
///
/// It holds no copy of the session state. Each read and each change goes to
/// the daemon, which is the only owner of that state.
#[derive(Clone)]
pub struct LiveSession {
    pub client: Arc<DaemonClient>,
    pub id: String,
}

impl LiveSession {
    /// End the session in the daemon. A later `cru chat --resume` opens it
    /// again from its stored state.
    pub async fn end(&self) {
        match self.client.session_end(&self.id).await {
            Ok(_) => info!(session_id = %self.id, "Session ended"),
            Err(e) => tracing::debug!(session_id = %self.id, error = %e, "session.end failed"),
        }
    }
}

/// A session that [`open_session`] opened, with the events of the session and
/// the prompts that already waited for an answer when the client attached.
pub struct OpenedSession {
    pub live: LiveSession,
    pub events: UnboundedReceiver<SessionEvent>,
    pub pending: Vec<InteractionEvent>,
    /// Where the session acts. A resumed session keeps the workspace it was
    /// created in, which need not be this process's directory.
    pub workspace: Option<std::path::PathBuf>,
    /// What a stored resume of this session did not bring back, from
    /// `session.resume`'s own reply. Empty for a session this call created,
    /// and for a resume that stayed in memory.
    pub resume_warnings: Vec<crucible_core::protocol::requests::ResumeWarning>,
}

/// Create or resume a daemon session, and subscribe to its events.
///
/// Starts the daemon when it does not run.
pub async fn open_session(
    config: &CliAppConfig,
    params: &AgentInitParams,
) -> Result<OpenedSession> {
    info!("Connecting to daemon (auto-start if needed)");
    let (client, events) = crate::common::daemon_client_with_events()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;
    let client = Arc::new(client);

    // Subscribe to every session BEFORE session.create. The setup task emits
    // its events as soon as the session exists, and a subscription made after
    // create returns would miss them. Without them the TUI waits at
    // "Loading..." for ever, so a failure here stops the command.
    client
        .session_subscribe(&["*"])
        .await
        .map_err(|e| anyhow::anyhow!("failed to subscribe to session events: {}", e))?;

    let ResolvedSession {
        id,
        resume_warnings,
    } = resolve_session_id(&client, config, params).await?;

    client
        .session_subscribe(&[id.as_str()])
        .await
        .map_err(|e| anyhow::anyhow!("failed to subscribe to session {id}: {e}"))?;
    let pending = pending_interactions(&client, &id).await;
    let workspace = match client.session_get(&id).await {
        Ok(session) => session_workspace(&session),
        Err(e) => {
            tracing::warn!(session_id = %id, error = %e, "Could not read the session's workspace");
            None
        }
    };
    info!(session_id = %id, "Daemon session ready");

    Ok(OpenedSession {
        live: LiveSession { client, id },
        events,
        pending,
        workspace,
        resume_warnings,
    })
}

/// The workspace of a `session.get` reply, when the session has one.
fn session_workspace(
    session: &crucible_core::session::SessionSummary,
) -> Option<std::path::PathBuf> {
    session
        .workspace
        .as_ref()
        .filter(|w| !w.as_os_str().is_empty())
        .cloned()
}

/// The prompts of `session_id` that wait for an answer.
///
/// A prompt asked before this client attached never comes as an event, so the
/// client reads the list once. A failed read loses only those prompts, and the
/// daemon still holds them, so it is a warning and not an error.
async fn pending_interactions(client: &DaemonClient, session_id: &str) -> Vec<InteractionEvent> {
    match client.session_pending_interactions().await {
        Ok(response) => pending_from_response(&response, session_id),
        Err(error) => {
            tracing::warn!(session_id = %session_id, error = %error, "Could not recover pending interactions");
            Vec::new()
        }
    }
}

fn pending_from_response(response: &serde_json::Value, session_id: &str) -> Vec<InteractionEvent> {
    response["pending"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| entry["session_id"].as_str() == Some(session_id))
        .filter_map(|entry| {
            Some(InteractionEvent {
                request_id: entry["request_id"].as_str()?.to_string(),
                request: serde_json::from_value::<InteractionRequest>(entry["request"].clone())
                    .ok()?,
            })
        })
        .collect()
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

/// The session id `open_session` resolved, and what a resume of it — if it
/// resumed one — cost, per the daemon's own `session.resume` reply. Empty
/// for a session this call created.
struct ResolvedSession {
    id: String,
    resume_warnings: Vec<crucible_core::protocol::requests::ResumeWarning>,
}

/// The `warnings` array of a `session.resume` reply, decoded. A reply that
/// carries none, or a shape this build does not recognize, answers empty:
/// the resume itself already succeeded, and a client that cannot read a
/// warning must not treat that as the resume having failed.
fn resume_warnings_of(
    reply: &serde_json::Value,
) -> Vec<crucible_core::protocol::requests::ResumeWarning> {
    reply
        .get("warnings")
        .cloned()
        .and_then(|w| serde_json::from_value(w).ok())
        .unwrap_or_default()
}

async fn resolve_session_id(
    client: &DaemonClient,
    config: &CliAppConfig,
    params: &AgentInitParams,
) -> Result<ResolvedSession> {
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

    let mut resume_warnings = Vec::new();
    let session_id = match &params.resume_session_id {
        Some(id) if !id.is_empty() => {
            info!("Resuming specific daemon session: {}", id);
            match client.session_resume(id).await {
                Ok(reply) => resume_warnings = resume_warnings_of(&reply),
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

            if let Some(session) = sessions.sessions.first() {
                let id = session.id.to_string();
                info!("Resuming most recent daemon session: {}", id);
                let reply = client.session_resume(&id).await?;
                resume_warnings = resume_warnings_of(&reply);
                id
            } else {
                info!("No existing session to resume, creating new one");
                create_new_daemon_session(client, config, &workspace, params, create_agent_type)
                    .await?
            }
        }
        None => {
            create_new_daemon_session(client, config, &workspace, params, create_agent_type).await?
        }
    };

    Ok(ResolvedSession {
        id: session_id,
        resume_warnings,
    })
}

async fn create_new_daemon_session(
    client: &crucible_daemon::DaemonClient,
    config: &CliAppConfig,
    workspace: &std::path::Path,
    params: &AgentInitParams,
    agent_type: &str,
) -> Result<String> {
    let create = SessionCreateRequest {
        session_type: "chat".into(),
        kilns: SessionCreateRequest::kiln_set(config.session_kiln_name()),
        workspace: Some(workspace.to_string_lossy().into_owned()),
        recording_mode: params.recording_mode.clone(),
        recording_path: params
            .recording_path
            .clone()
            .map(|p| p.to_string_lossy().into_owned()),
        agent_type: Some(agent_type.into()),
        ..Default::default()
    };
    let legacy_chat_defaults = agent_type == "internal"
        && params.provider_key.is_none()
        && config.llm.default_provider().is_none();
    let result = client
        .session_create(SessionCreateRequest {
            configure_agent: true,
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
            model: legacy_chat_defaults
                .then(|| config.chat.model.clone())
                .flatten(),
            endpoint: legacy_chat_defaults
                .then(|| config.chat.endpoint.clone())
                .flatten(),
            ..create
        })
        .await?;

    let session_id = result.id.to_string();

    info!("Created new daemon session: {}", session_id);
    Ok(session_id)
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
                    "result":{"session_id":"fixture", "type":"chat", "kilns":[], "state":"active",
                        "started_at":"2026-01-01T00:00:00Z", "event_count":0, "archived":false}});
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

    /// Regression: quitting the TUI must end the session, or the recording
    /// never gets its footer. The runner calls `end` when it exits.
    #[tokio::test]
    async fn ending_a_live_session_sends_session_end() {
        let daemon = crate::test_daemon::FakeDaemon::answering_null("chat-1").await;
        daemon.session.end().await;
        let calls = daemon.calls();
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert_eq!(calls[0].0, "session.end");
        assert_eq!(calls[0].1["session_id"], "chat-1");
    }

    #[test]
    fn the_workspace_comes_from_the_session_reply() {
        use crucible_core::session::{Session, SessionSummary, SessionType};

        let summary_with = |workspace: Option<&str>| {
            let session = Session::new(SessionType::Chat, Vec::new())
                .with_workspace(workspace.map(std::path::PathBuf::from));
            SessionSummary::from(&session)
        };

        let with = summary_with(Some("/work/project"));
        assert_eq!(
            session_workspace(&with),
            Some(std::path::PathBuf::from("/work/project"))
        );
        for without in [summary_with(None), summary_with(Some(""))] {
            assert_eq!(session_workspace(&without), None, "{without:?}");
        }
    }

    #[test]
    fn only_the_pending_prompts_of_this_session_are_recovered() {
        use crucible_core::interaction::AskRequest;
        let request = InteractionRequest::Ask(AskRequest::new("Which branch?"));
        let response = serde_json::json!({"pending": [
            {"session_id": "other", "request_id": "other-id", "request": request},
            {"session_id": "wanted", "request_id": "ask-id", "request": request},
        ]});
        let pending = pending_from_response(&response, "wanted");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].request_id, "ask-id");
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
