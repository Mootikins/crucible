//! `CrucibleAcpAgent`: implements the ACP `Agent` trait by delegating to the
//! Crucible daemon over RPC.
//!
//! This is a thin protocol adapter — all agent logic (LLM calls, tools,
//! precognition, session persistence) runs daemon-side. The adapter creates an
//! ordinary daemon session per ACP session, forwards prompts via
//! `session.send_message`, and translates the daemon's event stream into ACP
//! `session/update` notifications. Permission requests round-trip to the host
//! via `session/request_permission`.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use agent_client_protocol::{
    Agent, AgentCapabilities, AuthenticateRequest, AuthenticateResponse, CancelNotification,
    Client, Error, InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PromptCapabilities, PromptRequest, PromptResponse,
    RequestPermissionRequest, Result as AcpResult, SessionId, SessionNotification, StopReason,
};
use crucible_core::config::CliAppConfig;
use crucible_core::interaction::{InteractionRequest, InteractionResponse};
use crucible_daemon::rpc_client::SessionCreateParams;
use crucible_daemon::{DaemonClient, SessionEvent};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};

use super::translate::{
    classify_event, interaction_tool_call, outcome_to_interaction_response, permission_options,
    TurnStep,
};

/// Shared event stream for one ACP session's daemon connection.
type EventStream = Rc<Mutex<mpsc::UnboundedReceiver<SessionEvent>>>;

/// A resolved session: its daemon client, daemon session id, and event stream.
type SessionRef = (Rc<DaemonClient>, String, EventStream);

/// Per-ACP-session state: a dedicated daemon connection, the daemon session id,
/// and the event stream for that connection.
struct SessionEntry {
    client: Rc<DaemonClient>,
    daemon_session_id: String,
    events: EventStream,
}

/// ACP agent backed by the Crucible daemon.
pub struct CrucibleAcpAgent {
    config: CliAppConfig,
    /// Set once, immediately after the connection is constructed. Used to send
    /// `session/update` notifications and `session/request_permission`.
    conn: OnceCell<Rc<agent_client_protocol::AgentSideConnection>>,
    sessions: RefCell<HashMap<String, SessionEntry>>,
}

impl CrucibleAcpAgent {
    pub fn new(config: CliAppConfig) -> Self {
        Self {
            config,
            conn: OnceCell::new(),
            sessions: RefCell::new(HashMap::new()),
        }
    }

    /// Inject the connection handle after `AgentSideConnection::new`.
    pub fn set_connection(&self, conn: Rc<agent_client_protocol::AgentSideConnection>) {
        let _ = self.conn.set(conn);
    }

    fn connection(&self) -> AcpResult<Rc<agent_client_protocol::AgentSideConnection>> {
        self.conn.get().cloned().ok_or_else(Error::internal_error)
    }

    /// Look up a session's daemon client, id, and event stream.
    fn lookup(&self, acp_session_id: &str) -> Option<SessionRef> {
        let sessions = self.sessions.borrow();
        let entry = sessions.get(acp_session_id)?;
        Some((
            entry.client.clone(),
            entry.daemon_session_id.clone(),
            entry.events.clone(),
        ))
    }

    /// Pump daemon events into ACP `session/update` notifications until the turn
    /// ends, returning the stop reason to report on `session/prompt`.
    async fn pump_turn(
        &self,
        acp_session_id: &SessionId,
        client: &Rc<DaemonClient>,
        daemon_session_id: &str,
        events: &EventStream,
    ) -> AcpResult<StopReason> {
        let conn = self.connection()?;
        let mut rx = events.lock().await;
        loop {
            let Some(event) = rx.recv().await else {
                // Daemon connection closed mid-turn.
                return Ok(StopReason::Cancelled);
            };
            if event.session_id != daemon_session_id {
                continue;
            }
            match classify_event(&event) {
                TurnStep::Update(update) => {
                    let notif = SessionNotification::new(acp_session_id.clone(), *update);
                    if let Err(e) = conn.session_notification(notif).await {
                        warn!(error = ?e, "failed to send session/update; ending turn");
                        return Ok(StopReason::EndTurn);
                    }
                }
                TurnStep::Interaction {
                    request_id,
                    request,
                } => {
                    self.handle_interaction(
                        acp_session_id,
                        client,
                        daemon_session_id,
                        &conn,
                        &request_id,
                        &request,
                    )
                    .await?;
                }
                TurnStep::Finished(reason) => return Ok(reason),
                TurnStep::Ignore => {}
            }
        }
    }

    async fn handle_interaction(
        &self,
        acp_session_id: &SessionId,
        client: &Rc<DaemonClient>,
        daemon_session_id: &str,
        conn: &Rc<agent_client_protocol::AgentSideConnection>,
        request_id: &str,
        request: &InteractionRequest,
    ) -> AcpResult<()> {
        // Only permission requests are surfaced to the host in v1. Other
        // interaction primitives (Ask, Edit, Panel, ...) have no ACP analogue,
        // so we decline them with a Cancelled response and log — a headless
        // editor host cannot answer a free-form question.
        let response = if matches!(request, InteractionRequest::Permission(_)) {
            let tool_call = interaction_tool_call(request_id, request);
            let req = RequestPermissionRequest::new(
                acp_session_id.clone(),
                tool_call,
                permission_options(),
            );
            match conn.request_permission(req).await {
                Ok(resp) => outcome_to_interaction_response(&resp.outcome, request),
                Err(e) => {
                    warn!(error = ?e, "request_permission failed; denying");
                    InteractionResponse::Cancelled
                }
            }
        } else {
            warn!(
                kind = request.kind(),
                "non-permission interaction has no ACP mapping; auto-declining"
            );
            InteractionResponse::Cancelled
        };

        client
            .session_interaction_respond(daemon_session_id, request_id, response)
            .await
            .map_err(|e| {
                warn!(error = %e, "failed to deliver interaction response to daemon");
                Error::internal_error()
            })
    }
}

#[async_trait::async_trait(?Send)]
impl Agent for CrucibleAcpAgent {
    async fn initialize(&self, args: InitializeRequest) -> AcpResult<InitializeResponse> {
        // Echo the client's requested protocol version (we support v1 shapes);
        // advertise text prompts and load_session support.
        let caps = AgentCapabilities::new()
            .load_session(true)
            .prompt_capabilities(PromptCapabilities::new());
        Ok(InitializeResponse::new(args.protocol_version).agent_capabilities(caps))
    }

    async fn authenticate(&self, _args: AuthenticateRequest) -> AcpResult<AuthenticateResponse> {
        // No auth methods advertised; nothing to do.
        Ok(AuthenticateResponse::default())
    }

    async fn new_session(&self, args: NewSessionRequest) -> AcpResult<NewSessionResponse> {
        let workspace = args.cwd.clone();
        info!(cwd = %workspace.display(), "acp new_session");

        let (client, event_rx) =
            DaemonClient::connect_or_start_with_events()
                .await
                .map_err(|e| {
                    warn!(error = %e, "failed to connect to daemon");
                    Error::internal_error()
                })?;

        // Subscribe-first (wildcard) so setup/turn events are never missed by a
        // create→subscribe race; the pump filters by daemon session id.
        client.session_subscribe(&["*"]).await.map_err(|e| {
            warn!(error = %e, "failed to subscribe to session events");
            Error::internal_error()
        })?;

        let create = client
            .session_create(SessionCreateParams {
                session_type: "chat".to_string(),
                kiln: self.config.session_storage_path(),
                workspace: Some(workspace),
                connect_kilns: vec![],
                recording_mode: None,
                recording_path: None,
                agent_type: Some("internal".to_string()),
            })
            .await
            .map_err(|e| {
                warn!(error = %e, "session.create failed");
                Error::internal_error()
            })?;

        let daemon_session_id = create["session_id"]
            .as_str()
            .ok_or_else(Error::internal_error)?
            .to_string();

        let agent = crate::factories::agent::build_internal_session_agent(&self.config);
        client
            .session_configure_agent(&daemon_session_id, &agent)
            .await
            .map_err(|e| {
                warn!(error = %e, "session.configure_agent failed");
                Error::internal_error()
            })?;

        info!(session = %daemon_session_id, "acp session ready");
        self.sessions.borrow_mut().insert(
            daemon_session_id.clone(),
            SessionEntry {
                client: Rc::new(client),
                daemon_session_id: daemon_session_id.clone(),
                events: Rc::new(Mutex::new(event_rx)),
            },
        );

        Ok(NewSessionResponse::new(daemon_session_id))
    }

    async fn prompt(&self, args: PromptRequest) -> AcpResult<PromptResponse> {
        let acp_session_id = args.session_id.clone();
        let key = acp_session_id.0.as_ref().to_string();
        let (client, daemon_session_id, events) =
            self.lookup(&key).ok_or_else(Error::invalid_params)?;

        let text = prompt_text(&args);
        if text.trim().is_empty() {
            return Err(Error::invalid_params());
        }

        debug!(session = %daemon_session_id, "acp prompt: sending message");
        client
            .session_send_message(&daemon_session_id, &text, true)
            .await
            .map_err(|e| {
                warn!(error = %e, "session.send_message failed");
                Error::internal_error()
            })?;

        let reason = self
            .pump_turn(&acp_session_id, &client, &daemon_session_id, &events)
            .await?;
        Ok(PromptResponse::new(reason))
    }

    async fn cancel(&self, args: CancelNotification) -> AcpResult<()> {
        let key = args.session_id.0.as_ref().to_string();
        if let Some((client, daemon_session_id, _)) = self.lookup(&key) {
            debug!(session = %daemon_session_id, "acp cancel");
            if let Err(e) = client.session_cancel(&daemon_session_id).await {
                warn!(error = %e, "session.cancel failed");
            }
        }
        Ok(())
    }

    async fn load_session(&self, args: LoadSessionRequest) -> AcpResult<LoadSessionResponse> {
        // Resume an existing daemon session and re-attach an event stream. We do
        // not replay history as session/update in v1 (the host keeps its own
        // transcript); the session becomes live for new prompts.
        let daemon_session_id = args.session_id.0.as_ref().to_string();
        info!(session = %daemon_session_id, "acp load_session");

        let (client, event_rx) = DaemonClient::connect_or_start_with_events()
            .await
            .map_err(|_| Error::internal_error())?;
        client
            .session_subscribe(&["*"])
            .await
            .map_err(|_| Error::internal_error())?;
        client
            .session_resume(&daemon_session_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "session.resume failed");
                Error::internal_error()
            })?;

        self.sessions.borrow_mut().insert(
            daemon_session_id.clone(),
            SessionEntry {
                client: Rc::new(client),
                daemon_session_id,
                events: Rc::new(Mutex::new(event_rx)),
            },
        );
        Ok(LoadSessionResponse::default())
    }
}

/// Concatenate the text content blocks of a prompt into a single string.
fn prompt_text(req: &PromptRequest) -> String {
    use agent_client_protocol::ContentBlock;
    let mut parts = Vec::new();
    for block in &req.prompt {
        match block {
            ContentBlock::Text(t) => parts.push(t.text.clone()),
            ContentBlock::ResourceLink(link) => parts.push(format!("[{}]", link.uri)),
            _ => {}
        }
    }
    parts.join("\n")
}
