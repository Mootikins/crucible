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
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

mod translate;

use translate::{acp_prompt_text, replay_unannounced_tool_calls, turn_stop_reason};

use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};

use crate::acp::client::{CrucibleAcpClient, PermissionRequestHandler};
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
    /// How to run the agent inside a plugin's sandbox, from the session's
    /// isolation claim. `None` when nothing claimed the session.
    pub sandbox_exec: Option<crucible_lua::SandboxExec>,
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
            sandbox_exec,
        } = params;

        let agent_name = agent_config
            .agent_name
            .clone()
            .unwrap_or_else(|| "acp".to_string());

        info!(agent = %agent_name, workspace = %workspace.display(), "Creating ACP agent handle");

        let client_config = crate::acp_launch::build_client_config(
            agent_config,
            workspace,
            acp_config,
            sandbox_exec.as_ref(),
        )?;
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
                        crate::trust_resolution::resolve_session_classification(workspace, kiln)
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
        use crucible_core::turn::{TurnError, TurnEvent};
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
            // Whether this turn announced *any* tool call. Deliberately not
            // `!announced_ids.is_empty()`: the replay below yields calls that
            // contribute no id (see `replay_unannounced_tool_calls`), so the
            // set answers "which ids are spoken for", not "did a batch exist".
            let mut announced_any = false;
            let mut tool_names_by_id: HashMap<String, String> = HashMap::new();
            // Whether the turn produced anything the user can see. Drives
            // `StopReason::Empty`, which is otherwise unreachable here.
            let mut produced_content = false;
            // Results whose `ToolStart` never arrived, held until the
            // post-stream replay can name them. See the `ToolEnd` arm.
            let mut orphaned_results: Vec<(String, Option<String>, Option<String>)> = Vec::new();

            while let Some(chunk) = chunk_rx.recv().await {
                match chunk {
                    StreamingChunk::Text(text) => {
                        debug!(chunk_type = "text", len = text.len(), "ACP streaming chunk");
                        produced_content = true;
                        yield TurnEvent::TextDelta(text);
                    }
                    StreamingChunk::Thinking(text) => {
                        debug!(chunk_type = "thinking", len = text.len(), "ACP streaming chunk");
                        produced_content = true;
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
                        announced_any = true;
                        yield TurnEvent::ToolCall {
                            id,
                            name,
                            args: arguments.unwrap_or(serde_json::Value::Null),
                            diffs,
                        };
                    }
                    StreamingChunk::ToolEnd { id, result, error } => {
                        let Some(name) = tool_names_by_id.remove(&id) else {
                            // No `ToolStart` for this id — an ACP
                            // `tool_call_update{status: completed}` whose
                            // `tool_call` never arrived. Naming it here would
                            // have to invent one, and the renderer keys results
                            // on the id of an announced call
                            // (`containers.rs::update_tool`), so a made-up name
                            // reaches the transcript and the web view while the
                            // TUI silently drops the result.
                            //
                            // Hold it instead: if that same update carried a
                            // title/raw_input/diff, the client recorded the call
                            // and the post-stream replay below announces it and
                            // supplies the real name.
                            debug!(
                                tool_id = %id,
                                "ACP tool result arrived before its call; deferring until named"
                            );
                            orphaned_results.push((id, result, error));
                            continue;
                        };
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
                Ok(Ok((_content, acp_tool_calls, response, usage))) => {
                    debug!(
                        tool_count = acp_tool_calls.len(),
                        has_usage = usage.is_some(),
                        "ACP stream completed"
                    );

                    // Emit any final tool calls the ACP client reported but
                    // the streaming callback hadn't announced.
                    let replayed = replay_unannounced_tool_calls(acp_tool_calls, &announced_ids);
                    announced_any |= !replayed.is_empty();
                    let mut replayed_names: HashMap<String, String> = HashMap::new();
                    for event in replayed {
                        if let TurnEvent::ToolCall { id, name, .. } = &event {
                            replayed_names.insert(id.clone(), name.clone());
                        }
                        yield event;
                    }

                    // Results deferred above, now that the replay has named
                    // what it could. A result whose call is still unknown is
                    // dropped: nothing downstream can render it (no card was
                    // ever created for that id) and the only alternative is to
                    // invent a name for a tool that was never announced.
                    for (id, result, error) in orphaned_results {
                        let Some(name) = replayed_names.remove(&id) else {
                            warn!(
                                tool_id = %id,
                                "ACP reported a tool result for a call it never announced; dropping"
                            );
                            continue;
                        };
                        info!(
                            tool = %name, tool_id = %id,
                            has_error = error.is_some(),
                            "ACP tool call completed (named by post-stream replay)"
                        );
                        yield TurnEvent::ToolResult {
                            id,
                            name,
                            result: serde_json::Value::String(result.unwrap_or_default()),
                            error,
                        };
                    }

                    // Close the batch. An ACP agent runs its own tool loop, so
                    // the whole turn is one batch — there is no boundary on the
                    // wire to split it at — and it closes once every call has
                    // been announced. That is *after* the replay above, not
                    // before it: `ToolBatchEnd` claims "no further tool calls in
                    // this batch", so emitting it first would split one logical
                    // batch in two and leave the second half never closed.
                    //
                    // The scheduler resets per-batch state on this event
                    // (`agent_manager/messaging/stream.rs:839`) — but today that
                    // is a no-op here rather than a reason for the ordering:
                    // every variable it touches (`in_tool_batch`,
                    // `capped_this_batch`, `batch_terminate_signals`) is still
                    // at its default on an `owns_history` turn, because the
                    // pass-through arm `continue`s before any of them is
                    // written. Emitting this is control-flow neutral today; it
                    // is emitted for contract consistency with
                    // `GenaiAgentHandle`, so a future consumer of the batch
                    // boundary is not silently wrong on delegated turns.
                    //
                    // A turn that called nothing announces nothing: an empty
                    // batch-end would claim a batch that never existed. The
                    // guard is honest in both directions now that an unnamed
                    // result is dropped rather than emitted: every `ToolResult`
                    // this turn yields belongs to a `ToolCall` it also yielded,
                    // so "a batch existed" and "a result was reported" can no
                    // longer disagree (divergence B4).
                    if announced_any {
                        yield TurnEvent::ToolBatchEnd;
                    }

                    if let Some(usage) = usage {
                        yield TurnEvent::Usage(usage);
                    }
                    yield TurnEvent::Done {
                        stop_reason: turn_stop_reason(
                            response.stop_reason,
                            produced_content || announced_any,
                        ),
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
