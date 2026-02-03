//! ACP agent handle for daemon-managed external agents.
//!
//! Implements `AgentHandle` by wrapping `crucible-acp`'s protocol layer,
//! allowing the daemon to spawn and manage ACP agents (claude-code, opencode,
//! codex, etc.) with the same lifecycle as internal Rig agents.
//!
//! The daemon handles session persistence, event streaming, permission hooks,
//! and Lua handlers — ACP agents get all of these for free by routing through
//! this handle instead of being spawned directly by the CLI.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crucible_acp::client::{ClientConfig, CrucibleAcpClient};
use crucible_acp::streaming::{channel_callback, StreamingChunk};
use crucible_acp::InProcessMcpHost;
use crucible_config::AcpConfig;
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::{AgentHandle, ChatChunk, ChatError, ChatResult, ChatToolCall};
use crucible_core::traits::KnowledgeRepository;
use crucible_core::types::acp::schema::{AvailableCommand, SessionModeState};
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
    commands: Vec<AvailableCommand>,
    session_id: Option<String>,
    cached_temperature: Option<f64>,
    cached_max_tokens: Option<u32>,
    cached_thinking_budget: Option<i64>,
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
    /// * `acp_config` - Optional ACP configuration (timeouts, etc.)
    pub async fn new(
        agent_config: &SessionAgent,
        workspace: &Path,
        kiln_path: Option<&Path>,
        knowledge_repo: Option<Arc<dyn KnowledgeRepository>>,
        embedding_provider: Option<Arc<dyn EmbeddingProvider>>,
        acp_config: Option<&AcpConfig>,
    ) -> Result<Self, AcpHandleError> {
        let agent_name = agent_config
            .agent_name
            .clone()
            .unwrap_or_else(|| "acp".to_string());

        info!(agent = %agent_name, workspace = %workspace.display(), "Creating ACP agent handle");

        let client_config = build_client_config(agent_config, workspace, acp_config)?;
        let mut client = CrucibleAcpClient::with_name(client_config, agent_name.clone());
        let mcp_host = if let (Some(kiln), Some(repo), Some(embed)) =
            (kiln_path, knowledge_repo, embedding_provider)
        {
            match InProcessMcpHost::start(kiln.to_path_buf(), repo, embed).await {
                Ok(host) => {
                    info!(url = %host.sse_url(), "In-process MCP server started");
                    Some(host)
                }
                Err(e) => {
                    warn!("Failed to start in-process MCP server: {}, falling back to stdio", e);
                    None
                }
            }
        } else {
            None
        };

        let session = if let Some(ref host) = mcp_host {
            client
                .connect_with_sse_mcp(&host.sse_url())
                .await
                .map_err(|e| AcpHandleError::Connection(e.to_string()))?
        } else {
            client
                .connect_with_handshake()
                .await
                .map_err(|e| AcpHandleError::Connection(e.to_string()))?
        };

        let session_id = session.id().to_string();
        info!(session_id = %session_id, "ACP agent connected");

        let mode_id = "normal".to_string();

        let commands = client.available_commands().to_vec();

        Ok(Self {
            client: Arc::new(Mutex::new(Some(client))),
            _mcp_host: mcp_host,
            agent_name,
            mode_id,
            mode_state: default_internal_modes(),
            commands,
            session_id: Some(session_id),
            cached_temperature: agent_config.temperature,
            cached_max_tokens: agent_config.max_tokens,
            cached_thinking_budget: agent_config.thinking_budget,
        })
    }
}

#[async_trait]
impl AgentHandle for AcpAgentHandle {
    fn send_message_stream(
        &mut self,
        message: String,
    ) -> BoxStream<'static, ChatResult<ChatChunk>> {
        use tokio::sync::mpsc;

        let session_id = match &self.session_id {
            Some(id) => id.clone(),
            None => {
                return Box::pin(futures::stream::once(async {
                    Err(ChatError::AgentUnavailable(
                        "ACP agent not connected".to_string(),
                    ))
                }));
            }
        };

        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel::<StreamingChunk>();
        let callback = channel_callback(chunk_tx);

        let client_arc = Arc::clone(&self.client);
        let client_opt = {
            // SAFETY: &mut self prevents concurrent calls at compile time,
            // so this lock is never contended during normal operation.
            let mut guard = client_arc.try_lock().expect(
                "AcpAgentHandle: concurrent send_message_stream (should not happen with &mut self)",
            );
            guard.take()
        };

        let Some(client) = client_opt else {
            return Box::pin(futures::stream::once(async {
                Err(ChatError::AgentUnavailable(
                    "ACP client is busy (already streaming)".to_string(),
                ))
            }));
        };

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

            {
                let mut guard = client_arc.lock().await;
                *guard = Some(owned_client);
            }

            let _ = result_tx.send(result);
        });

        type UnfoldState = Option<(
            mpsc::UnboundedReceiver<StreamingChunk>,
            tokio::sync::oneshot::Receiver<
                Result<
                    (String, Vec<crucible_acp::ToolCallInfo>, agent_client_protocol::PromptResponse),
                    crucible_acp::ClientError,
                >,
            >,
            Vec<ChatToolCall>,
        )>;

        Box::pin(futures::stream::unfold(
            Some((chunk_rx, result_rx, Vec::new())) as UnfoldState,
            |state| async move {
                let (mut rx, result_rx, mut tool_calls): (
                    mpsc::UnboundedReceiver<StreamingChunk>,
                    _,
                    Vec<ChatToolCall>,
                ) = state?;

                match rx.recv().await {
                    Some(chunk) => {
                        let chat_chunk = match chunk {
                            StreamingChunk::Text(text) => ChatChunk {
                                delta: text,
                                done: false,
                                tool_calls: None,
                                tool_results: None,
                                reasoning: None,
                                usage: None,
                                subagent_events: None,
                            },
                            StreamingChunk::Thinking(text) => ChatChunk {
                                delta: String::new(),
                                done: false,
                                tool_calls: None,
                                tool_results: None,
                                reasoning: Some(text),
                                usage: None,
                                subagent_events: None,
                            },
                            StreamingChunk::ToolStart { name, id } => {
                                tool_calls.push(ChatToolCall {
                                    name: name.clone(),
                                    arguments: None,
                                    id: Some(id.clone()),
                                });
                                ChatChunk {
                                    delta: String::new(),
                                    done: false,
                                    tool_calls: Some(vec![ChatToolCall {
                                        name,
                                        arguments: None,
                                        id: Some(id),
                                    }]),
                                    tool_results: None,
                                    reasoning: None,
                                    usage: None,
                                    subagent_events: None,
                                }
                            }
                            StreamingChunk::ToolEnd { id: _ } => ChatChunk {
                                delta: String::new(),
                                done: false,
                                tool_calls: None,
                                tool_results: None,
                                reasoning: None,
                                usage: None,
                                subagent_events: None,
                            },
                        };
                        Some((Ok(chat_chunk), Some((rx, result_rx, tool_calls))))
                    }
                    None => {
                        match result_rx.await {
                            Ok(Ok((_content, acp_tool_calls, _response))) => {
                                let acp_tool_calls: Vec<crucible_acp::ToolCallInfo> =
                                    acp_tool_calls;
                                debug!(
                                    tool_count = acp_tool_calls.len(),
                                    "ACP stream completed"
                                );

                                let final_tool_calls: Vec<ChatToolCall> = acp_tool_calls
                                    .into_iter()
                                    .map(|t| ChatToolCall {
                                        name: t.title,
                                        arguments: t.arguments,
                                        id: t.id,
                                    })
                                    .collect();

                                Some((
                                    Ok(ChatChunk {
                                        delta: String::new(),
                                        done: true,
                                        tool_calls: if final_tool_calls.is_empty() {
                                            None
                                        } else {
                                            Some(final_tool_calls)
                                        },
                                        tool_results: None,
                                        reasoning: None,
                                        usage: None,
                                        subagent_events: None,
                                    }),
                                    None,
                                ))
                            }
                            Ok(Err(e)) => {
                                warn!(error = %e, "ACP stream error");
                                Some((
                                    Err(ChatError::Communication(format!("ACP error: {}", e))),
                                    None,
                                ))
                            }
                            Err(_) => {
                                warn!("ACP streaming task dropped (oneshot cancelled)");
                                Some((
                                    Err(ChatError::Communication(
                                        "ACP streaming task failed".to_string(),
                                    )),
                                    None,
                                ))
                            }
                        }
                    }
                }
            },
        ))
    }

    fn is_connected(&self) -> bool {
        self.session_id.is_some()
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

    fn get_commands(&self) -> &[AvailableCommand] {
        &self.commands
    }

    async fn clear_history(&mut self) {
        // Daemon manages history — ACP handle is stateless w.r.t. conversation
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

impl Drop for AcpAgentHandle {
    fn drop(&mut self) {
        let client_arc = Arc::clone(&self.client);
        let agent = self.agent_name.clone();
        // try_current() returns None if tokio runtime is gone (shutdown, sync context).
        // In that case CrucibleAcpClient drops synchronously — child process gets
        // SIGKILL when its stdin/stdout handles close.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Some(client) = client_arc.lock().await.take() {
                    drop(client);
                    debug!(agent = %agent, "ACP agent cleaned up on drop");
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
    let agent_name = agent_config
        .agent_name
        .as_deref()
        .unwrap_or("acp");

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
        ("claude", "npx", &["@zed-industries/claude-code-acp"]),
        ("gemini", "gemini", &[]),
        ("codex", "npx", &["@zed-industries/codex-acp"]),
        ("cursor", "cursor-acp", &[]),
    ];

    let (mut command, mut args) = known
        .iter()
        .find(|(name, _, _)| *name == agent_name)
        .map(|(_, cmd, ag)| (cmd.to_string(), ag.iter().map(|s| s.to_string()).collect()))
        .unwrap_or_else(|| {
            (agent_name.to_string(), Vec::new())
        });

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
    use std::collections::HashMap;

    fn test_session_agent(agent_name: &str) -> SessionAgent {
        SessionAgent {
            agent_type: "acp".to_string(),
            agent_name: Some(agent_name.to_string()),
            provider_key: None,
            provider: "acp".to_string(),
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
        assert_eq!(args, vec!["@zed-industries/claude-code-acp"]);
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
        assert_eq!(env[0], ("OPENCODE_MODEL".to_string(), "ollama/llama3.2".to_string()));
    }

    #[test]
    fn test_profile_overrides_command() {
        let config = test_session_agent("opencode");
        let mut acp_config = AcpConfig::default();

        let mut profile = crucible_config::AgentProfile::default();
        profile.command = Some("/usr/local/bin/opencode".to_string());
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
        let mut acp_config = AcpConfig::default();
        acp_config.streaming_timeout_minutes = 30;

        let config =
            build_client_config(&agent, Path::new("/tmp"), Some(&acp_config)).unwrap();
        assert_eq!(config.timeout_ms, Some(180_000));
    }
}
