//! ACP agent handle for daemon-managed external agents.
//!
//! Implements `AgentHandle` by wrapping `crucible-daemon (acp module)`'s protocol layer,
//! allowing the daemon to spawn and manage ACP agents (claude-code, opencode,
//! codex, etc.) with the same lifecycle as internal Rig agents.
//!
//! The daemon handles session persistence, event streaming, permission hooks,
//! and Lua handlers — ACP agents get all of these for free by routing through
//! this handle instead of being spawned directly by the CLI.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

mod translate;

use translate::{acp_prompt_text, turn_stop_reason};

use crate::empty_providers::{EmptyEmbeddingProvider, EmptyKnowledgeRepository};

use crate::acp::client::{CrucibleAcpClient, PermissionRequestHandler};
use crate::acp::session::ModelChoice;
use crate::acp::streaming::{channel_callback, StreamingChunk};
use crate::mcp_host::InProcessMcpHost;
use crate::tools::DelegationContext;
use crucible_core::background::BackgroundSpawner;
use crucible_core::config::{AcpConfig, DelegationConfig};
use crucible_core::enrichment::EmbeddingProvider;
use crucible_core::session::SessionAgent;
use crucible_core::traits::chat::{AgentHandle, ChatError, ChatResult, SessionKnobs};
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
    /// The model selector the agent advertised in `configOptions` at
    /// connect. `None` when the agent exposes no model selector; that turns
    /// off the `model_switching` capability, `current_model` and
    /// `fetch_available_models`.
    model: Option<ModelChoice>,
    /// Every config option the agent advertised, kept as it sent them. The
    /// model selector is projected onto `model` above; these are the rest,
    /// which a client renders and the daemon does not interpret.
    config_options: Vec<crucible_core::types::acp::schema::SessionConfigOption>,
    session_id: Option<String>,
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
    /// Where agent cards come from, for the `delegate_session` tool text.
    pub card_roots: &'a crate::agent_cards::CardRoots,
    pub acp_config: Option<&'a AcpConfig>,
    pub permission_handler: Option<PermissionRequestHandler>,
    /// How to run the agent inside a plugin's sandbox, from the session's
    /// isolation claim. `None` when nothing claimed the session.
    pub sandbox_exec: Option<crucible_lua::SandboxExec>,
    /// The session's filesystem containment, for the in-process MCP server this
    /// handle exposes to the external agent. Its note/search/kiln tools take
    /// model-supplied paths exactly as the internal dispatcher's do, so they
    /// answer to the same root set — a containment rule enforced on one agent
    /// type and not the other is the tool-family split all over again.
    pub containment: crate::tools::containment::RootSet,
    /// The agent session id an earlier handle persisted on the daemon
    /// session. `Some` makes the connect flow send `session/resume`, so the
    /// agent keeps its history across a daemon restart. `None` opens a
    /// fresh agent session.
    pub resume_acp_session_id: Option<String>,
    /// Where connect-time announcements go — today only the resume
    /// fallback note. `None` drops the announcement; the fallback itself
    /// still happens.
    pub event_tx: Option<tokio::sync::broadcast::Sender<crate::protocol::SessionEventMessage>>,
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
            card_roots,
            acp_config,
            permission_handler,
            sandbox_exec,
            containment,
            resume_acp_session_id,
            event_tx,
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
                result_max_bytes: delegation_config
                    .map(|c| c.result_max_bytes)
                    .unwrap_or(51200),
                timeout_secs: delegation_config.map(|c| c.timeout_secs).unwrap_or(300),
                card_roots: card_roots.clone(),
            }),
            _ => None,
        };

        let mcp_host = if let Some(kiln) = kiln_path {
            let repo = knowledge_repo.unwrap_or_else(|| Arc::new(EmptyKnowledgeRepository));
            let embed = embedding_provider.unwrap_or_else(|| Arc::new(EmptyEmbeddingProvider));

            match InProcessMcpHost::start(
                kiln.to_path_buf(),
                workspace.to_path_buf(),
                repo,
                embed,
                delegation_context,
                containment,
            )
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
        let resume_id = resume_acp_session_id.as_deref();
        let (session, mcp_host) = match client
            .connect_with_best_mcp_resuming(mcp_url.as_deref(), resume_id)
            .await
        {
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
                    .connect_with_best_mcp_resuming(None, resume_id)
                    .await
                    .map_err(|e| AcpHandleError::Connection(e.to_string()))?;
                client = retry_client;
                (session, None)
            }
            Err(e) => return Err(AcpHandleError::Connection(e.to_string())),
        };

        let session_id = session.id().to_string();
        info!(session_id = %session_id, "ACP agent connected");

        // The agent answered session/resume with -32601, so its side of the
        // conversation restarted from nothing. Say so in the event stream —
        // a silent fallback would look like an agent that remembers and
        // does not (plan W7, decision d).
        if session.resume() == crate::acp::session::ResumeDisposition::FellBackToNew {
            warn!(
                agent = %agent_name,
                requested = ?resume_acp_session_id,
                session_id = %session_id,
                "session/resume unsupported; started a fresh agent session"
            );
            if let (Some(tx), Some(daemon_session_id)) = (event_tx.as_ref(), parent_session_id) {
                let _ = tx.send(
                    crate::protocol::SessionEventMessage::new(
                        daemon_session_id,
                        "acp_resume_fallback",
                        serde_json::json!({
                            "agent": agent_name,
                            "requested_session_id": resume_acp_session_id,
                            "new_session_id": session_id,
                            "reason": "the agent does not support session/resume; \
                                       a new agent session started without the \
                                       previous agent-side history",
                        }),
                    )
                    .with_timestamp(),
                );
            }
        }

        // The modes belong to the agent. claude-agent-acp declares five and
        // codex-acp three, with ids Crucible does not share, so answering
        // with `default_internal_modes()` offered a front end modes the
        // agent would reject and named a current mode (`normal`) that no ACP
        // agent has. The internal set stands in only for an agent that
        // declares none — the mock's default profile, and any agent that
        // answers `session/set_mode` with `-32601`.
        let mode_state = session
            .modes()
            .cloned()
            .unwrap_or_else(default_internal_modes);
        let mode_id = mode_state.current_mode_id.0.to_string();
        let model = session.model().cloned();
        let config_options = session.config_options().to_vec();

        Ok(Self {
            client: Arc::new(Mutex::new(Some(client))),
            _mcp_host: mcp_host,
            agent_name,
            mode_id,
            mode_state,
            model,
            config_options,
            session_id: Some(session_id),
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

    /// The agent's own session id, for the daemon to persist. A handle
    /// built after a daemon restart resumes it (`session/resume`).
    fn acp_session_id(&self) -> Option<String> {
        self.session_id.clone()
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
        // The mode set carries its own current id, and `get_modes` hands the
        // whole set out. Updating only `mode_id` left the two accessors on
        // this handle disagreeing after every switch: `get_mode_id` said the
        // new mode and `get_modes().current_mode_id` still said whatever the
        // agent declared at the handshake.
        self.mode_state.current_mode_id =
            crucible_core::types::acp::schema::SessionModeId::new(mode_id);
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

/// The ACP agent runs its own model loop. The handle caches the three
/// knobs the ACP wire can carry; the rest return the empty answer.
///
/// `precognition` belongs to the session's `AgentConfig`: the daemon turn
/// loop reads it from the config before it calls the handle, and the ACP wire
/// has no field for it. A value stored here would reach nothing, so the
/// handle refuses the setter. `DaemonAgentHandle` answers it by RPC.
#[async_trait]
impl SessionKnobs for AcpAgentHandle {
    /// ACP carries no system prompt; the agent owns its own.
    fn get_system_prompt(&self) -> Option<String> {
        None
    }

    /// Switch the agent's model through `session/set_config_option`.
    ///
    /// The agent keeps its history; only the selector value changes. The
    /// reply lists every option with its current value, so the local
    /// selector is read from the reply when the agent includes it.
    fn agent_config_options(&self) -> &[crucible_core::types::acp::schema::SessionConfigOption] {
        &self.config_options
    }

    async fn set_agent_config_option(&mut self, id: &str, value: &str) -> ChatResult<()> {
        // Fail fast on an id the agent did not list, the way `switch_model`
        // does: the agent would refuse it with a less clear message.
        if !self.config_options.iter().any(|o| o.id.to_string() == id) {
            return Err(ChatError::NotSupported(format!(
                "this ACP agent advertises no '{id}' option"
            )));
        }

        let Some(session_id) = self.session_id.clone() else {
            return Err(ChatError::NotSupported("ACP agent not connected".into()));
        };

        let response = {
            let mut guard = self.client.lock().await;
            let client = guard.as_mut().ok_or_else(|| {
                ChatError::AgentUnavailable("ACP client unavailable (busy streaming)".into())
            })?;
            client
                .set_config_option(&session_id, id, value)
                .await
                .map_err(|e| {
                    ChatError::ModeChange(format!("ACP agent rejected '{id}' = '{value}': {e}"))
                })?
        };

        // The agent answers with its whole option list, which is the only
        // report of what the value became — an agent may clamp or normalise
        // what it was sent.
        self.config_options = response.config_options.clone();
        info!(option = %id, value = %value, "Set ACP agent config option");
        Ok(())
    }

    async fn switch_model(&mut self, model_id: &str) -> ChatResult<()> {
        let Some(model) = self.model.as_ref() else {
            return Err(ChatError::NotSupported(
                "this ACP agent does not advertise a model selector".into(),
            ));
        };

        // Fail fast on an id the agent did not list; the agent would refuse
        // it, with a less clear message.
        if !model.available.iter().any(|id| id == model_id) {
            return Err(ChatError::ModeChange(format!(
                "model '{model_id}' is not in the agent's advertised model list"
            )));
        }
        let config_id = model.config_id.clone();

        let Some(session_id) = self.session_id.clone() else {
            return Err(ChatError::NotSupported("ACP agent not connected".into()));
        };

        let response = {
            let mut guard = self.client.lock().await;
            let client = guard.as_mut().ok_or_else(|| {
                ChatError::AgentUnavailable("ACP client unavailable (busy streaming)".into())
            })?;
            client
                .set_config_option(&session_id, &config_id, model_id)
                .await
                .map_err(|e| {
                    ChatError::ModeChange(format!("ACP agent rejected model '{model_id}': {e}"))
                })?
        };

        match ModelChoice::from_config_options(&response.config_options) {
            Some(choice) => self.model = Some(choice),
            None => {
                if let Some(model) = self.model.as_mut() {
                    model.current = model_id.to_string();
                }
            }
        }
        info!(model = %model_id, "Switched ACP agent model");
        Ok(())
    }

    fn current_model(&self) -> Option<&str> {
        self.model.as_ref().map(|m| m.current.as_str())
    }

    async fn fetch_available_models(&mut self) -> Vec<String> {
        self.model
            .as_ref()
            .map(|m| m.available.clone())
            .unwrap_or_default()
    }

    async fn fetch_available_modes(&mut self) -> Vec<String> {
        Vec::new()
    }

    async fn set_context_strategy(
        &mut self,
        _strategy: crucible_core::session::ContextStrategy,
    ) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_context_strategy".into()))
    }

    fn get_context_strategy(&self) -> crucible_core::session::ContextStrategy {
        crucible_core::session::ContextStrategy::default()
    }

    async fn set_precognition(&mut self, _enabled: bool) -> ChatResult<()> {
        Err(ChatError::NotSupported("set_precognition".into()))
    }

    fn get_precognition(&self) -> bool {
        true
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
            // Only when the agent advertised a model selector at connect.
            model_switching: self.model.is_some(),
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
            use agent_client_protocol::schema::v1::{ContentBlock, PromptRequest, SessionId};

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
            // Capture the model choice a mid-turn `config_option_update`
            // parked on the client, so `current_model` reports the switch.
            let model_update = owned_client.take_model_update();

            {
                let mut guard = client_arc.lock().await;
                *guard = Some(owned_client);
            }

            let _ = result_tx
                .send(result.map(|(summary, response)| (summary, response, usage, model_update)));
        });

        let model_slot = &mut self.model;
        let body = stream! {
            // The client already decided everything the stream needs to know:
            // every `ToolEnd` follows a `ToolStart` for its id and carries the
            // name that start carried, and the summary at the end says what
            // the turn showed the user. So each chunk maps to one event
            // through a total `From`, and the handle keeps no state of its own.
            while let Some(chunk) = chunk_rx.recv().await {
                yield TurnEvent::from(chunk);
            }

            match result_rx.await {
                Ok(Ok((summary, response, usage, model_update))) => {
                    if let Some(choice) = model_update {
                        *model_slot = Some(choice);
                    }
                    debug!(
                        produced_content = summary.produced_content,
                        announced_any = summary.announced_any,
                        has_usage = usage.is_some(),
                        "ACP stream completed"
                    );

                    // Close the batch. An ACP agent runs its own tool loop, so
                    // the whole turn is one batch — there is no boundary on the
                    // wire to split it at — and it closes once the client has
                    // flushed every call, which happens before the response
                    // arrives here. `ToolBatchEnd` claims "no further tool
                    // calls in this batch", so it comes after every call.
                    //
                    // The scheduler resets per-batch state on this event
                    // (`agent_manager/messaging/stream.rs`), but that is a
                    // no-op on an `owns_history` turn today. It is emitted for
                    // contract consistency with `GenaiAgentHandle`, so a future
                    // consumer of the batch boundary is not silently wrong on
                    // delegated turns.
                    //
                    // A turn that called nothing announces nothing: an empty
                    // batch-end would claim a batch that never existed.
                    if summary.announced_any {
                        yield TurnEvent::ToolBatchEnd;
                    }

                    if let Some(usage) = usage {
                        yield TurnEvent::Usage(usage);
                    }
                    yield TurnEvent::Done {
                        stop_reason: turn_stop_reason(
                            response.stop_reason,
                            summary.produced_content || summary.announced_any,
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
        SessionKnobs::switch_model(self, model_id)
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
                if let Some(mut client) = client_arc.lock().await.take() {
                    // Say goodbye first: `session/close` lets the agent free
                    // the session (plan W7, decision d). The timeout keeps a
                    // hung agent from delaying its own SIGKILL, which the
                    // drop below performs. A refusal is only logged — the
                    // agent dies either way.
                    if let (Some(id), true) =
                        (session_id.as_deref(), client.agent_supports_session_close())
                    {
                        let close = client.close_session(id);
                        match tokio::time::timeout(std::time::Duration::from_secs(2), close).await {
                            Ok(Ok(())) => debug!(agent = %agent, "session/close acknowledged"),
                            Ok(Err(e)) => {
                                debug!(agent = %agent, error = %e, "session/close refused")
                            }
                            Err(_) => debug!(agent = %agent, "session/close timed out"),
                        }
                    }
                    drop(client);
                    info!(agent = %agent, session_id = ?session_id, "ACP session terminated");
                }
            });
        }
    }
}
