//! ACP agent handle for daemon-managed external agents.
//!
//! Implements `AgentHandle` by wrapping `crucible-daemon (acp module)`'s protocol layer,
//! allowing the daemon to spawn and manage ACP agents (claude-code, opencode,
//! codex, etc.) with the same lifecycle as internal Rig agents.
//!
//! The daemon handles session persistence, event streaming, permission hooks,
//! and Lua handlers — ACP agents get all of these for free by routing through
//! this handle instead of being spawned directly by the CLI.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};

use crate::acp::client::{ClientConfig, CrucibleAcpClient, PermissionRequestHandler};
use crate::acp::streaming::{channel_callback, StreamingChunk};
use crate::mcp_host::InProcessMcpHost;
use crate::tools::DelegationContext;
use crucible_core::background::BackgroundSpawner;
use crucible_core::config::{AcpConfig, DataClassification, DelegationConfig};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::{AgentHandle, ChatError, ChatResult};
use crucible_core::traits::KnowledgeRepository;
use crucible_core::types::acp::schema::SessionModeState;
use crucible_core::types::mode::default_internal_modes;

/// Errors specific to ACP agent handle creation and management.
#[derive(Error, Debug)]
pub enum AcpHandleError {
    #[error("ACP connection failed: {0}")]
    Connection(String),

    #[error("Agent not found: {0}")]
    AgentNotFound(String),

    #[error("ACP protocol error: {0}")]
    Protocol(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Daemon-side handle to an ACP agent process.
///
/// Wraps the low-level ACP protocol client and implements `AgentHandle` so the
/// daemon's `AgentManager` can treat ACP agents identically to internal Rig agents.
///
/// Unlike the old CLI adapter, this handle:
/// - Does NOT manage its own history (daemon's `SessionManager` does that)
/// - Does NOT do context enrichment (daemon's precognition does that)
/// - Does NOT use `unsafe` lifetime transmutation
/// - Routes through daemon's event system for multi-client consistency
pub struct AcpAgentHandle {
    client: Arc<Mutex<Option<CrucibleAcpClient>>>,
    _mcp_host: Option<InProcessMcpHost>,
    agent_name: String,
    mode_id: String,
    mode_state: SessionModeState,
    session_id: Option<String>,
    cached_temperature: Option<f64>,
    cached_max_tokens: Option<u32>,
    cached_thinking_budget: Option<i64>,
}

/// Parameters for creating a new ACP agent handle.
pub struct AcpAgentHandleParams<'a> {
    pub agent_config: &'a SessionAgent,
    pub workspace: &'a Path,
    pub kiln_path: Option<&'a Path>,
    pub knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
    pub embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
    pub background_spawner: Option<Arc<dyn BackgroundSpawner>>,
    pub parent_session_id: Option<&'a str>,
    pub delegation_config: Option<&'a DelegationConfig>,
    pub acp_config: Option<&'a AcpConfig>,
    pub permission_handler: Option<PermissionRequestHandler>,
}

impl AcpAgentHandle {
    /// Create and connect a new ACP agent handle.
    ///
    /// This spawns the external agent process, performs the ACP protocol handshake,
    /// and optionally starts an in-process MCP server for tool execution.
    ///
    /// # Arguments
    ///
    /// * `agent_config` - Session agent configuration with `agent_type: "acp"`
    /// * `workspace` - Working directory for the agent
    /// * `kiln_path` - Optional kiln path for MCP server
    /// * `knowledge_repo` - Optional repository for MCP semantic search
    /// * `embedding_provider` - Optional embedding provider for MCP
    /// * `background_spawner` - Optional spawner used by delegate_session
    /// * `parent_session_id` - Parent daemon session id
    /// * `delegation_config` - Delegation limits and allowlist for this agent
    /// * `acp_config` - Optional ACP configuration (timeouts, etc.)
    pub async fn new(params: AcpAgentHandleParams<'_>) -> Result<Self, AcpHandleError> {
        let AcpAgentHandleParams {
            agent_config,
            workspace,
            kiln_path,
            knowledge_repo,
            embedding_provider,
            background_spawner,
            parent_session_id,
            delegation_config,
            acp_config,
            permission_handler,
        } = params;

        let agent_name = agent_config
            .agent_name
            .clone()
            .unwrap_or_else(|| "acp".to_string());

        info!(agent = %agent_name, workspace = %workspace.display(), "Creating ACP agent handle");

        let client_config = build_client_config(agent_config, workspace, acp_config)?;
        let mut client = CrucibleAcpClient::with_name(client_config.clone(), agent_name.clone());
        if let Some(ref handler) = permission_handler {
            client = client.with_permission_handler(handler.clone());
        }
        let delegation_context = parent_session_id.and_then(|session_id| {
            background_spawner.clone().map(|spawner| DelegationContext {
                background_spawner: spawner,
                session_id: session_id.to_string(),
                targets: delegation_config
                    .and_then(|c| c.allowed_targets.clone())
                    .unwrap_or_default(),
                enabled: delegation_config.map(|c| c.enabled).unwrap_or(false),
                depth: 0,
                data_classification: kiln_path
                    .and_then(|kiln| {
                        crate::trust_resolution::resolve_kiln_classification(workspace, kiln)
                    })
                    .unwrap_or(DataClassification::Public),
            })
        });

        let mcp_host = if kiln_path.is_some() {
            let repo = knowledge_repo.unwrap_or_else(|| Arc::new(EmptyKnowledgeRepository));
            let embed = embedding_provider.unwrap_or_else(|| Arc::new(EmptyEmbeddingProvider));

            match InProcessMcpHost::start(workspace.to_path_buf(), repo, embed, delegation_context)
                .await
            {
                Ok(host) => {
                    info!(url = %host.mcp_url(), "In-process MCP server started");
                    debug!(
                        url = %host.mcp_url(),
                        "In-process MCP server started — will attempt HTTP transport"
                    );
                    Some(host)
                }
                Err(e) => {
                    warn!(
                        "Failed to start in-process MCP server: {}, falling back to stdio",
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        let mcp_url = mcp_host.as_ref().map(|h| h.mcp_url());
        debug!(
            mcp_url = ?mcp_url,
            "Selecting MCP transport for ACP agent"
        );
        let (session, mcp_host) = match client.connect_with_best_mcp(mcp_url.as_deref()).await {
            Ok(s) => (s, mcp_host),
            Err(e) if mcp_host.is_some() => {
                // HTTP MCP transport failed (e.g. agent rejects `type: "http"` at
                // Zod level). Drop the HTTP server, create a fresh agent process,
                // and retry with stdio-only transport.
                warn!(
                    agent = %agent_name,
                    error = %e,
                    "HTTP MCP failed, retrying with stdio transport"
                );
                drop(mcp_host);
                let mut retry_client =
                    CrucibleAcpClient::with_name(client_config.clone(), agent_name.clone());
                if let Some(ref handler) = permission_handler {
                    retry_client = retry_client.with_permission_handler(handler.clone());
                }
                let session = retry_client
                    .connect_with_best_mcp(None)
                    .await
                    .map_err(|e| AcpHandleError::Connection(e.to_string()))?;
                client = retry_client;
                (session, None)
            }
            Err(e) => return Err(AcpHandleError::Connection(e.to_string())),
        };

        let session_id = session.id().to_string();
        info!(session_id = %session_id, "ACP agent connected");

        let mode_id = "normal".to_string();

        Ok(Self {
            client: Arc::new(Mutex::new(Some(client))),
            _mcp_host: mcp_host,
            agent_name,
            mode_id,
            mode_state: default_internal_modes(),
            session_id: Some(session_id),
            cached_temperature: agent_config.temperature,
            cached_max_tokens: agent_config.max_tokens,
            cached_thinking_budget: agent_config.thinking_budget,
        })
    }
}

#[async_trait]
impl AgentHandle for AcpAgentHandle {
    async fn send_message_fire_and_forget(&mut self, _message: String) -> ChatResult<()> {
        // ACP handles are daemon-side — the TUI never calls this directly.
        Err(ChatError::NotSupported(
            "AcpAgentHandle::send_message_fire_and_forget — use Agent::turn".to_string(),
        ))
    }

    fn get_mode_id(&self) -> &str {
        &self.mode_id
    }

    async fn set_mode_str(&mut self, mode_id: &str) -> ChatResult<()> {
        info!(mode = %mode_id, "Setting ACP agent mode");

        if let Some(session_id) = &self.session_id {
            let session_id = session_id.clone();
            let mut guard = self.client.lock().await;
            if let Some(client) = guard.as_mut() {
                client
                    .set_session_mode(&session_id, mode_id)
                    .await
                    .map_err(|e| {
                        ChatError::ModeChange(format!(
                            "ACP agent rejected mode '{}': {}",
                            mode_id, e
                        ))
                    })?;
            }
        }

        self.mode_id = mode_id.to_string();
        Ok(())
    }

    async fn clear_history(&mut self) -> ChatResult<()> {
        // ACP agents own their conversation state; clearing requires
        // terminating and restarting the agent process, which the CLI
        // path (DaemonAgentHandle::clear_history) refuses for ACP
        // sessions. Surface the same error here in case this handle is
        // ever invoked directly.
        Err(ChatError::NotSupported(
            "ACP agents manage their own history; clearing would require restarting the agent"
                .into(),
        ))
    }

    async fn set_temperature(&mut self, temperature: f64) -> ChatResult<()> {
        debug!(temperature, "Caching temperature for ACP agent");
        self.cached_temperature = Some(temperature);
        Ok(())
    }

    fn get_temperature(&self) -> Option<f64> {
        self.cached_temperature
    }

    async fn set_thinking_budget(&mut self, budget: i64) -> ChatResult<()> {
        debug!(budget, "Caching thinking budget for ACP agent");
        self.cached_thinking_budget = Some(budget);
        Ok(())
    }

    fn get_thinking_budget(&self) -> Option<i64> {
        self.cached_thinking_budget
    }

    async fn set_max_tokens(&mut self, max_tokens: Option<u32>) -> ChatResult<()> {
        debug!(?max_tokens, "Caching max tokens for ACP agent");
        self.cached_max_tokens = max_tokens;
        Ok(())
    }

    fn get_max_tokens(&self) -> Option<u32> {
        self.cached_max_tokens
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        Err(ChatError::NotSupported(format!(
            "ACP agents manage their own model. Cannot switch to '{}'",
            model_id
        )))
    }

    fn current_model(&self) -> Option<&str> {
        None
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        Some(&self.mode_state)
    }

    async fn cancel(&self) -> ChatResult<()> {
        debug!("Cancel requested for ACP agent (graceful only)");
        Ok(())
    }
}

// -- Native `Agent` impl ----------------------------------------------------
//
// ACP agents run their own tool loop server-side; this impl translates each
// streaming chunk directly to a `TurnEvent`. No inbound channel is consumed
// (ACP observes, doesn't re-enter).

#[async_trait]
impl crucible_core::turn::Agent for AcpAgentHandle {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities {
            streaming: true,
            tool_calls: true,
            thinking: true,
            model_switching: false,
            usage_reporting: true,
            cancellation: false,
            temperature_control: false,
            max_tokens_control: false,
            owns_history: true,
            modes: true,
        }
    }

    async fn turn<'a>(
        &'a mut self,
        ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        use async_stream::stream;
        use crucible_core::turn::{StopReason, TurnError, TurnEvent};
        use tokio::sync::mpsc;

        let message = ctx.content;

        let Some(session_id) = self.session_id.clone() else {
            let body = stream! {
                yield TurnEvent::Error(TurnError::AgentUnavailable(
                    "ACP agent not connected".to_string(),
                ));
            };
            return Ok(Box::pin(body));
        };

        let client_arc = Arc::clone(&self.client);
        let client_opt = {
            // &mut self prevents concurrent calls at compile time,
            // so this lock is never contended during normal operation.
            let mut guard = match client_arc.try_lock() {
                Ok(g) => g,
                Err(_) => {
                    let body = stream! {
                        yield TurnEvent::Error(TurnError::AgentUnavailable(
                            "ACP client lock contention (concurrent turn)".to_string(),
                        ));
                    };
                    return Ok(Box::pin(body));
                }
            };
            guard.take()
        };

        let Some(client) = client_opt else {
            let body = stream! {
                yield TurnEvent::Error(TurnError::AgentUnavailable(
                    "ACP client is busy (already streaming)".to_string(),
                ));
            };
            return Ok(Box::pin(body));
        };

        let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel::<StreamingChunk>();
        let callback = channel_callback(chunk_tx);
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            use agent_client_protocol::{ContentBlock, PromptRequest, SessionId};

            let prompt_request = PromptRequest::new(
                SessionId::from(session_id),
                vec![ContentBlock::from(message)],
            );

            let mut owned_client = client;
            let result = owned_client
                .send_prompt_with_callback(prompt_request, callback)
                .await;
            // Capture usage now while we still own the client; stream code
            // parsed it from the ACP PromptResponse and stashed it there.
            let usage = owned_client.take_last_usage();

            {
                let mut guard = client_arc.lock().await;
                *guard = Some(owned_client);
            }

            let _ = result_tx
                .send(result.map(|(content, tools, response)| (content, tools, response, usage)));
        });

        let body = stream! {
            let mut announced_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut tool_names_by_id: HashMap<String, String> = HashMap::new();

            while let Some(chunk) = chunk_rx.recv().await {
                match chunk {
                    StreamingChunk::Text(text) => {
                        debug!(chunk_type = "text", len = text.len(), "ACP streaming chunk");
                        yield TurnEvent::TextDelta(text);
                    }
                    StreamingChunk::Thinking(text) => {
                        debug!(chunk_type = "thinking", len = text.len(), "ACP streaming chunk");
                        yield TurnEvent::Thinking(text);
                    }
                    StreamingChunk::ToolStart { name, id, arguments } => {
                        info!(tool = %name, tool_id = %id, "ACP tool call started");
                        tool_names_by_id.insert(id.clone(), name.clone());
                        announced_ids.insert(id.clone());
                        yield TurnEvent::ToolCall {
                            id,
                            name,
                            args: arguments.unwrap_or(serde_json::Value::Null),
                            // TODO(task-12b): populate from ACP ToolCallContent::Diff frames.
                            diffs: Vec::new(),
                        };
                    }
                    StreamingChunk::ToolEnd { id, result, error } => {
                        let name = tool_names_by_id
                            .remove(&id)
                            .unwrap_or_else(|| "unknown_tool".to_string());
                        info!(
                            tool = %name, tool_id = %id,
                            has_error = error.is_some(),
                            "ACP tool call completed"
                        );
                        yield TurnEvent::ToolResult {
                            id,
                            name,
                            result: serde_json::Value::String(result.unwrap_or_default()),
                            error,
                        };
                    }
                }
            }

            match result_rx.await {
                Ok(Ok((_content, acp_tool_calls, _response, usage))) => {
                    debug!(
                        tool_count = acp_tool_calls.len(),
                        has_usage = usage.is_some(),
                        "ACP stream completed"
                    );

                    // Emit any final tool calls the ACP client reported but
                    // the streaming callback hadn't announced.
                    for tc in acp_tool_calls {
                        let id = tc.id.clone().unwrap_or_default();
                        if announced_ids.contains(&id) {
                            continue;
                        }
                        yield TurnEvent::ToolCall {
                            id,
                            name: tc.title,
                            args: tc.arguments.unwrap_or(serde_json::Value::Null),
                            // TODO(task-12b): populate from ACP ToolCallContent::Diff frames.
                            diffs: Vec::new(),
                        };
                    }

                    if let Some(usage) = usage {
                        yield TurnEvent::Usage(usage);
                    }
                    yield TurnEvent::Done {
                        stop_reason: StopReason::EndTurn,
                    };
                }
                Ok(Err(e)) => {
                    warn!(error = %e, "ACP stream error");
                    let turn_err = match e {
                        crate::acp::ClientError::Connection(msg) => TurnError::Connection(
                            format!("ACP agent connection lost: {msg}"),
                        ),
                        crate::acp::ClientError::Timeout(msg) => {
                            TurnError::Communication(format!("ACP agent timed out: {msg}"))
                        }
                        crate::acp::ClientError::Session(msg) => {
                            TurnError::AgentUnavailable(format!("ACP session error: {msg}"))
                        }
                        crate::acp::ClientError::Protocol(err) => {
                            TurnError::Communication(format!("ACP protocol error: {err}"))
                        }
                        crate::acp::ClientError::PermissionDenied(msg) => {
                            TurnError::Communication(format!("ACP permission denied: {msg}"))
                        }
                        crate::acp::ClientError::InvalidConfig(msg) => {
                            TurnError::InvalidInput(format!("ACP configuration error: {msg}"))
                        }
                        crate::acp::ClientError::Validation(msg) => {
                            TurnError::InvalidInput(format!("ACP validation error: {msg}"))
                        }
                        crate::acp::ClientError::NotFound(msg) => {
                            TurnError::AgentUnavailable(format!("ACP resource not found: {msg}"))
                        }
                        crate::acp::ClientError::Io(err) => {
                            TurnError::Internal(format!("ACP error: {err}"))
                        }
                        crate::acp::ClientError::Serialization(err) => {
                            TurnError::Internal(format!("ACP error: {err}"))
                        }
                        crate::acp::ClientError::FileSystem(msg) => {
                            TurnError::Internal(format!("ACP error: {msg}"))
                        }
                        crate::acp::ClientError::Other(err) => {
                            TurnError::Internal(format!("ACP error: {err}"))
                        }
                    };
                    yield TurnEvent::Error(turn_err);
                }
                Err(_) => {
                    warn!("ACP streaming task dropped (oneshot cancelled)");
                    yield TurnEvent::Error(TurnError::AgentUnavailable(
                        "ACP agent process terminated unexpectedly".to_string(),
                    ));
                }
            }
        };

        Ok(Box::pin(body))
    }

    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        // Graceful cancel — matches the AgentHandle impl.
        Ok(())
    }

    async fn switch_model(
        &mut self,
        _model_id: &str,
    ) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

impl Drop for AcpAgentHandle {
    fn drop(&mut self) {
        let client_arc = Arc::clone(&self.client);
        let agent = self.agent_name.clone();
        let session_id = self.session_id.clone();
        // try_current() returns None if tokio runtime is gone (shutdown, sync context).
        // In that case CrucibleAcpClient drops synchronously — child process gets
        // SIGKILL when its stdin/stdout handles close.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Some(client) = client_arc.lock().await.take() {
                    drop(client);
                    info!(agent = %agent, session_id = ?session_id, "ACP session terminated");
                }
            });
        }
    }
}

/// Build a `ClientConfig` from `SessionAgent` fields.
///
/// Maps agent_name to command/args via discovery, merges env_overrides,
/// and applies ACP config timeouts.
fn build_client_config(
    agent_config: &SessionAgent,
    workspace: &Path,
    acp_config: Option<&AcpConfig>,
) -> Result<ClientConfig, AcpHandleError> {
    let agent_name = agent_config.agent_name.as_deref().unwrap_or("acp");

    let (command, args, env_vars) = resolve_agent_command(agent_name, agent_config, acp_config)?;

    // Protocol layer multiplies timeout_ms by 10, so: minutes * 60_000 / 10 = minutes * 6000
    let timeout_ms = acp_config
        .map(|c| c.streaming_timeout_minutes * 6000)
        .unwrap_or(90_000);

    Ok(ClientConfig {
        agent_path: PathBuf::from(command),
        agent_args: if args.is_empty() { None } else { Some(args) },
        working_dir: Some(workspace.to_path_buf()),
        env_vars: if env_vars.is_empty() {
            None
        } else {
            Some(env_vars)
        },
        timeout_ms: Some(timeout_ms),
        max_retries: None,
    })
}

/// Resolved command, arguments, and environment variables for an ACP agent.
type ResolvedCommand = (String, Vec<String>, Vec<(String, String)>);

/// Resolve agent name to (command, args, env_vars) using known agents list
/// and any profile overrides from AcpConfig.
fn resolve_agent_command(
    agent_name: &str,
    agent_config: &SessionAgent,
    acp_config: Option<&AcpConfig>,
) -> Result<ResolvedCommand, AcpHandleError> {
    let known: &[(&str, &str, &[&str])] = &[
        ("opencode", "opencode", &["acp"]),
        ("claude", "npx", &["@zed-industries/claude-agent-acp"]),
        ("gemini", "gemini", &[]),
        ("codex", "npx", &["@zed-industries/codex-acp"]),
        ("cursor", "cursor-acp", &[]),
    ];

    let (mut command, mut args) = known
        .iter()
        .find(|(name, _, _)| *name == agent_name)
        .map(|(_, cmd, ag)| (cmd.to_string(), ag.iter().map(|s| s.to_string()).collect()))
        .unwrap_or_else(|| (agent_name.to_string(), Vec::new()));

    if let Some(config) = acp_config {
        if let Some(profile) = config.agents.get(agent_name) {
            if let Some(ref cmd) = profile.command {
                command = cmd.clone();
            }
            if let Some(ref profile_args) = profile.args {
                args = profile_args.clone();
            }
        }
    }

    let mut env_vars: Vec<(String, String)> = agent_config
        .env_overrides
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if let Some(config) = acp_config {
        if let Some(profile) = config.agents.get(agent_name) {
            for (k, v) in &profile.env {
                if let Some(existing) = env_vars.iter_mut().find(|(ek, _)| ek == k) {
                    existing.1 = v.clone();
                } else {
                    env_vars.push((k.clone(), v.clone()));
                }
            }
        }
    }

    Ok((command, args, env_vars))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::BackendType;
    use std::collections::HashMap;

    fn test_session_agent(agent_name: &str) -> SessionAgent {
        SessionAgent {
            agent_type: "acp".to_string(),
            agent_name: Some(agent_name.to_string()),
            provider_key: None,
            provider: BackendType::Custom,
            model: "acp".to_string(),
            system_prompt: String::new(),
            temperature: None,
            max_tokens: None,
            max_context_tokens: None,
            thinking_budget: None,
            endpoint: None,
            env_overrides: HashMap::new(),
            mcp_servers: Vec::new(),
            agent_card_name: None,
            capabilities: None,
            agent_description: None,
            delegation_config: None,
            precognition_enabled: false,
            precognition_results: 5,
            max_iterations: None,
            execution_timeout_secs: None,
            context_budget: None,
            context_strategy: Default::default(),
            context_window: None,
            output_validation: Default::default(),
            validation_retries: 3,
        }
    }

    #[test]
    fn test_resolve_known_agent() {
        let config = test_session_agent("opencode");
        let (cmd, args, env) = resolve_agent_command("opencode", &config, None).unwrap();
        assert_eq!(cmd, "opencode");
        assert_eq!(args, vec!["acp"]);
        assert!(env.is_empty());
    }

    #[test]
    fn test_resolve_claude_agent() {
        let config = test_session_agent("claude");
        let (cmd, args, _) = resolve_agent_command("claude", &config, None).unwrap();
        assert_eq!(cmd, "npx");
        assert_eq!(args, vec!["@zed-industries/claude-agent-acp"]);
    }

    #[test]
    fn test_resolve_unknown_agent_uses_name_as_command() {
        let config = test_session_agent("my-custom-agent");
        let (cmd, args, _) = resolve_agent_command("my-custom-agent", &config, None).unwrap();
        assert_eq!(cmd, "my-custom-agent");
        assert!(args.is_empty());
    }

    #[test]
    fn test_env_overrides_from_session_agent() {
        let mut config = test_session_agent("opencode");
        config
            .env_overrides
            .insert("OPENCODE_MODEL".to_string(), "ollama/llama3.2".to_string());

        let (_, _, env) = resolve_agent_command("opencode", &config, None).unwrap();
        assert_eq!(env.len(), 1);
        assert_eq!(
            env[0],
            ("OPENCODE_MODEL".to_string(), "ollama/llama3.2".to_string())
        );
    }

    #[test]
    fn test_profile_overrides_command() {
        let config = test_session_agent("opencode");
        let mut acp_config = AcpConfig::default();

        let profile = crucible_core::config::AgentProfile {
            command: Some("/usr/local/bin/opencode".to_string()),
            ..Default::default()
        };
        acp_config.agents.insert("opencode".to_string(), profile);

        let (cmd, _, _) = resolve_agent_command("opencode", &config, Some(&acp_config)).unwrap();
        assert_eq!(cmd, "/usr/local/bin/opencode");
    }

    #[test]
    fn test_build_client_config() {
        let agent = test_session_agent("opencode");
        let config = build_client_config(&agent, Path::new("/tmp/workspace"), None).unwrap();

        assert_eq!(config.agent_path, PathBuf::from("opencode"));
        assert_eq!(config.agent_args, Some(vec!["acp".to_string()]));
        assert_eq!(config.working_dir, Some(PathBuf::from("/tmp/workspace")));
    }

    #[test]
    fn test_build_client_config_with_timeout() {
        let agent = test_session_agent("opencode");
        let acp_config = AcpConfig {
            streaming_timeout_minutes: 30,
            ..Default::default()
        };

        let config = build_client_config(&agent, Path::new("/tmp"), Some(&acp_config)).unwrap();
        assert_eq!(config.timeout_ms, Some(180_000));
    }
}
