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
/// - Forwards daemon-injected context (Precognition, Lua `transform_context`)
///   into the ACP prompt via `acp_prompt_text`, so external agents see the
///   knowledge graph the same way internal agents do
/// - Does NOT use `unsafe` lifetime transmutation
/// - Routes through daemon's event system for multi-client consistency
pub struct AcpAgentHandle {
    client: Arc<Mutex<Option<CrucibleAcpClient>>>,
    _mcp_host: Option<InProcessMcpHost>,
    agent_name: String,
    mode_id: String,
    mode_state: SessionModeState,
    /// Model state advertised by the agent at connect (`unstable_session_model`).
    /// `None` if the agent doesn't expose models — drives the `model_switching`
    /// capability and `current_model`/`fetch_available_models`.
    model_state: Option<agent_client_protocol::SessionModelState>,
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
    pub delegation_spawner: Option<Arc<dyn crate::delegation::DelegationSpawner>>,
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
            delegation_spawner,
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
        let delegation_context = match (
            parent_session_id,
            background_spawner.clone(),
            delegation_spawner.clone(),
        ) {
            (Some(session_id), Some(bg), Some(deleg)) => Some(DelegationContext {
                background_spawner: bg,
                delegation_spawner: deleg,
                session_id: session_id.to_string(),
                targets: delegation_config
                    .and_then(|c| c.allowed_targets.clone())
                    .unwrap_or_default(),
                enabled: delegation_config.map(|c| c.enabled).unwrap_or(false),
                depth: 0,
                result_max_bytes: delegation_config
                    .map(|c| c.result_max_bytes)
                    .unwrap_or(51200),
                timeout_secs: delegation_config.map(|c| c.timeout_secs).unwrap_or(300),
                data_classification: kiln_path
                    .and_then(|kiln| {
                        crate::trust_resolution::resolve_kiln_classification(workspace, kiln)
                    })
                    .unwrap_or(DataClassification::Public),
            }),
            _ => None,
        };

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
        let model_state = session.models().cloned();

        Ok(Self {
            client: Arc::new(Mutex::new(Some(client))),
            _mcp_host: mcp_host,
            agent_name,
            mode_id,
            mode_state: default_internal_modes(),
            model_state,
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
        let Some(model_state) = self.model_state.as_ref() else {
            return Err(ChatError::NotSupported(
                "this ACP agent does not advertise selectable models".into(),
            ));
        };

        // Reject ids the agent didn't advertise — set_model on an unknown id
        // is an error on the agent side, so fail fast with a clear message.
        if !model_state
            .available_models
            .iter()
            .any(|m| m.model_id.0.as_ref() == model_id)
        {
            return Err(ChatError::ModeChange(format!(
                "model '{}' is not in the agent's advertised model list",
                model_id
            )));
        }

        let Some(session_id) = self.session_id.clone() else {
            return Err(ChatError::NotSupported("ACP agent not connected".into()));
        };

        {
            let mut guard = self.client.lock().await;
            let client = guard.as_mut().ok_or_else(|| {
                ChatError::AgentUnavailable("ACP client unavailable (busy streaming)".into())
            })?;
            client
                .set_session_model(&session_id, model_id)
                .await
                .map_err(|e| {
                    ChatError::ModeChange(format!("ACP agent rejected model '{model_id}': {e}"))
                })?;
        }

        // Reflect the new current model locally so `current_model` is accurate
        // without a round-trip.
        if let Some(model_state) = self.model_state.as_mut() {
            model_state.current_model_id = model_id.to_string().into();
        }
        info!(model = %model_id, "Switched ACP agent model");
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        self.model_state
            .as_ref()
            .map(|m| m.current_model_id.0.as_ref())
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        self.model_state
            .as_ref()
            .map(|m| {
                m.available_models
                    .iter()
                    .map(|info| info.model_id.0.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_modes(&self) -> Option<&SessionModeState> {
        Some(&self.mode_state)
    }

    async fn cancel(&self) -> ChatResult<()> {
        // Cancellation is driven by the daemon dropping the turn stream, which
        // the ACP client detects (callback returns false) and answers by
        // sending `session/cancel` to the agent. This handle method is not on
        // that path — the daemon never calls it — so it is a no-op.
        debug!("Cancel requested for ACP agent (handled via stream drop)");
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
            // Only when the agent advertised a model list at connect.
            model_switching: self.model_state.is_some(),
            usage_reporting: true,
            // Cancelling the turn drops the daemon's stream; the ACP client
            // reacts by sending `session/cancel`, stopping the agent
            // server-side (see acp/client/streaming.rs).
            cancellation: true,
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

        // Forward daemon-injected context (Precognition, Lua transform_context)
        // alongside the user content. ACP agents own their history, so we only
        // send the new turn's content plus any injected System-role blocks.
        let message = acp_prompt_text(&ctx.content, &ctx.messages);

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
                    StreamingChunk::ToolStart { name, id, arguments, diffs } => {
                        info!(
                            tool = %name,
                            tool_id = %id,
                            diff_count = diffs.len(),
                            "ACP tool call started"
                        );
                        tool_names_by_id.insert(id.clone(), name.clone());
                        announced_ids.insert(id.clone());
                        yield TurnEvent::ToolCall {
                            id,
                            name,
                            args: arguments.unwrap_or(serde_json::Value::Null),
                            diffs,
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
                    StreamingChunk::ToolDiffUpdate { call_id, diffs } => {
                        debug!(
                            tool_id = %call_id,
                            diff_count = diffs.len(),
                            "ACP late diff update"
                        );
                        yield TurnEvent::ToolCallDiffUpdate {
                            id: call_id,
                            diffs,
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
                    // the streaming callback hadn't announced. `tc.diffs` was
                    // populated by `record_tool_call` from the ACP frames'
                    // `ToolCallContent::Diff` entries (initial + later
                    // `ToolCallUpdate` frames merged via `upsert_tool_info`).
                    for tc in acp_tool_calls {
                        let id = tc.id.clone().unwrap_or_default();
                        if announced_ids.contains(&id) {
                            continue;
                        }
                        yield TurnEvent::ToolCall {
                            id,
                            name: tc.title,
                            args: tc.arguments.unwrap_or(serde_json::Value::Null),
                            diffs: tc.diffs,
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
        // See AgentHandle::cancel — real cancellation happens when the turn
        // stream is dropped and the ACP client sends `session/cancel`.
        Ok(())
    }

    async fn switch_model(
        &mut self,
        model_id: &str,
    ) -> Result<(), crucible_core::turn::NotSupported> {
        // Delegate to the AgentHandle impl (the daemon's RPC path). The Agent
        // trait can only signal `NotSupported`, so a runtime wire failure is
        // surfaced through that variant; the AgentHandle path carries detail.
        AgentHandle::switch_model(self, model_id)
            .await
            .map_err(|_| crucible_core::turn::NotSupported::new("switch_model"))
    }
}

impl Drop for AcpAgentHandle {
    fn drop(&mut self) {
        let client_arc = Arc::clone(&self.client);
        let agent = self.agent_name.clone();
        let session_id = self.session_id.clone();
        // try_current() returns None if tokio runtime is gone (shutdown, sync context).
        // In that case CrucibleAcpClient drops synchronously — the retained child
        // is SIGKILLed via kill_on_drop (pipe close alone only sends EOF).
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

/// Build the prompt text sent to an ACP agent for one turn.
///
/// ACP agents own their conversation history, so we send only the new user
/// content — never the daemon's flattened history (that would duplicate what
/// the agent already holds). The exception is daemon-injected context:
/// Precognition and Lua `transform_context` handlers prepend System-role
/// blocks to `ctx.messages` (see `apply_transform_context_handlers`). Those
/// represent knowledge the external agent has no other way to see, so we
/// forward them ahead of the user content. Precognition only fires on the
/// first user message, so this does not bloat the agent's context every turn.
fn acp_prompt_text(
    content: &str,
    messages: &[crucible_core::traits::context_ops::ContextMessage],
) -> String {
    use crucible_core::traits::llm::MessageRole;

    let injected: Vec<&str> = messages
        .iter()
        .filter(|m| m.role == MessageRole::System)
        .map(|m| m.content.as_str())
        .filter(|s| !s.is_empty())
        .collect();

    if injected.is_empty() {
        return content.to_string();
    }

    let mut out = injected.join("\n\n");
    out.push_str("\n\n");
    out.push_str(content);
    out
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
            mode: None,
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
            autocompact_threshold: None,
            tool_policy: None,
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

    // -- ACP prompt building: daemon-injected context push -------------------
    //
    // ACP agents own their conversation history, so `turn()` sends only the
    // new user content — never the flattened history. But daemon-side context
    // injection (Precognition, Lua `transform_context`) is prepended to
    // `ctx.messages` as System-role blocks the agent would otherwise never
    // see. `acp_prompt_text` is the seam that forwards that injected context.

    use crucible_core::traits::context_ops::ContextMessage;

    #[test]
    fn injected_system_context_is_prepended_to_user_content() {
        let precog = ContextMessage::system("KNOWLEDGE:\n- foo relates to bar")
            .with_tag(crate::agent_manager::precognition::PRECOGNITION_TAG);
        let user = ContextMessage::user("What is foo?");

        let prompt = acp_prompt_text("What is foo?", &[precog, user]);

        assert!(
            prompt.contains("KNOWLEDGE:\n- foo relates to bar"),
            "injected precognition context must reach the ACP prompt, got: {prompt:?}"
        );
        assert!(prompt.contains("What is foo?"));
        // Injected context precedes the user's question.
        assert!(
            prompt.find("KNOWLEDGE").unwrap() < prompt.find("What is foo?").unwrap(),
            "injected context must come before the user content"
        );
    }

    #[test]
    fn no_injected_context_leaves_user_content_unchanged() {
        let user = ContextMessage::user("just chatting");
        // History (User/Assistant) must NOT be resent — the ACP agent owns it.
        let prior = ContextMessage::assistant("earlier reply");
        let prompt = acp_prompt_text("just chatting", &[prior, user]);
        assert_eq!(prompt, "just chatting");
    }

    #[test]
    fn multiple_system_blocks_are_all_injected_in_order() {
        let a = ContextMessage::system("BLOCK_A");
        let b = ContextMessage::system("BLOCK_B");
        let prompt = acp_prompt_text("hi", &[a, b]);
        let ia = prompt.find("BLOCK_A").unwrap();
        let ib = prompt.find("BLOCK_B").unwrap();
        let iu = prompt.find("hi").unwrap();
        assert!(
            ia < ib && ib < iu,
            "blocks injected in order before content: {prompt:?}"
        );
    }

    #[test]
    fn empty_messages_falls_back_to_raw_content() {
        assert_eq!(acp_prompt_text("solo", &[]), "solo");
    }
}
