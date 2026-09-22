//! The daemon side of one ACP agent connection.
//!
//! The `agent-client-protocol` SDK owns the JSON-RPC framing, the request
//! correlation and the inbound dispatch. This module owns what Crucible adds
//! on top: the agent process, the handshake, the permission bridge, and the
//! translation of a turn's session updates into [`StreamingChunk`]s.
//!
//! The SDK connection runs on its own task. The client holds a clone of its
//! [`ConnectionTo<Agent>`], so a request and a turn can run at the same time.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionNotification, SessionUpdate, ToolCallId,
};
use agent_client_protocol::{Agent, Client, ConnectTo, ConnectionTo, JsonRpcRequest};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::acp::session::ModelChoice;
use crate::acp::{ClientError, Result};

mod connection;
mod recording;
pub mod replay;
mod streaming;
mod tool_table;
mod tools;
mod types;

#[cfg(test)]
mod tests;

pub use recording::{Direction, FixtureHeader, FrameRecord, Recorder};
pub use types::ClientConfig;

pub type PermissionOutcomeFuture = Pin<Box<dyn Future<Output = RequestPermissionOutcome> + Send>>;
pub type PermissionRequestHandler =
    Arc<dyn Fn(RequestPermissionRequest) -> PermissionOutcomeFuture + Send + Sync>;

/// The deadline of one handshake request. The old client allowed five
/// minutes per read, and an agent that `npx` must first download can use
/// much of that on `initialize`.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(300);

/// The turn that runs now. The notification handler sends the turn's
/// updates here. A permission request races the handler with `cancel`.
struct Turn {
    updates: mpsc::UnboundedSender<SessionUpdate>,
    cancel: CancellationToken,
}

/// State that the notification handler writes and the client reads.
#[derive(Default)]
struct Shared {
    turn: Option<Turn>,
    /// The model choice from the latest `config_option_update`.
    model_update: Option<ModelChoice>,
    /// The tool name each `tool_call` frame of this turn announced, by id.
    ///
    /// An agent may ask permission for a tool it does not name in the
    /// request: codex-acp asks about an MCP call with `kind: "execute"` and
    /// nothing else, because the identity reached the client in the
    /// `tool_call` frame that came first. The permission policy keys on a
    /// name, so the client keeps what it saw and joins on `toolCallId`.
    ///
    /// Cleared at the start of each turn, which bounds it: a permission
    /// request always belongs to the turn that is running.
    tool_names: std::collections::HashMap<ToolCallId, String>,
}

/// One connection to one ACP agent.
pub struct CrucibleAcpClient {
    cx: ConnectionTo<Agent>,
    agent_name: String,
    config: ClientConfig,
    shared: Arc<Mutex<Shared>>,
    /// The capabilities from `initialize`. The default until the handshake.
    caps: AgentCapabilities,
    /// A drop of this sender ends the SDK connection.
    _stop: oneshot::Sender<()>,
    /// The agent process, when the client started one. A drop kills it.
    _child: Option<connection::AgentProcess>,
}

impl std::fmt::Debug for CrucibleAcpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrucibleAcpClient")
            .field("agent_name", &self.agent_name)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl CrucibleAcpClient {
    /// Connect over `transport`, with no agent process. Tests and replay use
    /// this.
    pub async fn connect(
        config: ClientConfig,
        transport: impl ConnectTo<Client> + 'static,
        agent_name: impl Into<String>,
        permission: Option<PermissionRequestHandler>,
    ) -> Result<Self> {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (cx_tx, cx_rx) = oneshot::channel();
        let (stop, stop_rx) = oneshot::channel::<()>();
        let agent_name = agent_name.into();

        let builder = Client
            .builder()
            .name(agent_name.clone())
            .on_receive_notification(
                {
                    let shared = Arc::clone(&shared);
                    async move |notification: SessionNotification, _cx| {
                        route_update(&shared, notification.update);
                        Ok(())
                    }
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                {
                    let shared = Arc::clone(&shared);
                    async move |mut request: RequestPermissionRequest,
                                responder,
                                cx: ConnectionTo<Agent>| {
                        let cancel = {
                            let shared = lock(&shared);
                            // An agent that asks about a tool it did not name
                            // is answered from the `tool_call` frame it sent
                            // for the same id. The permission policy keys on
                            // a name; supplying it here keeps that one join
                            // beside the frames it reads, not in the gate.
                            name_the_tool_call(&shared, &mut request);
                            shared
                                .turn
                                .as_ref()
                                .map(|turn| turn.cancel.clone())
                                .unwrap_or_default()
                        };
                        let permission = permission.clone();
                        // The dispatch loop waits for a handler. A user who
                        // takes a minute to answer must not stop the updates
                        // of the turn, so the answer waits on its own task.
                        cx.spawn(async move {
                            let outcome = tokio::select! {
                                outcome = ask(permission, request) => outcome,
                                () = cancel.cancelled() => RequestPermissionOutcome::Cancelled,
                            };
                            // A closed connection has nobody to answer. The
                            // task returns `Ok`, because an error stops the
                            // connection.
                            let _ = responder.respond(RequestPermissionResponse::new(outcome));
                            Ok(())
                        })
                    }
                },
                agent_client_protocol::on_receive_request!(),
            );

        let name = agent_name.clone();
        tokio::spawn(async move {
            let result = builder
                .connect_with(transport, async move |cx| {
                    let _ = cx_tx.send(cx);
                    let _ = stop_rx.await;
                    Ok(())
                })
                .await;
            if let Err(error) = result {
                tracing::debug!(agent = %name, %error, "ACP connection ended with an error");
            }
        });

        let cx = cx_rx.await.map_err(|_| {
            ClientError::Connection("the ACP connection ended before it started".into())
        })?;
        Ok(Self {
            cx,
            agent_name,
            config,
            shared,
            caps: AgentCapabilities::default(),
            _stop: stop,
            _child: None,
        })
    }

    /// Send one request and wait for its response.
    pub async fn request<R: JsonRpcRequest>(&self, request: R) -> Result<R::Response> {
        let method = request.method().to_string();
        self.cx
            .send_request(request)
            .block_task()
            .await
            .map_err(|error| request_error(&method, &error))
    }

    /// Send one handshake request. The outer error is the deadline; the
    /// inner one is the agent's answer, for a caller that reads its code.
    async fn handshake_call<R: JsonRpcRequest>(
        &self,
        request: R,
    ) -> Result<std::result::Result<R::Response, agent_client_protocol::Error>> {
        let method = request.method().to_string();
        tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            self.cx.send_request(request).block_task(),
        )
        .await
        .map_err(|_| {
            ClientError::Timeout(format!("{method} timed out after {HANDSHAKE_TIMEOUT:?}"))
        })
    }

    /// [`Self::request`] with the handshake deadline.
    async fn handshake_request<R: JsonRpcRequest>(&self, request: R) -> Result<R::Response> {
        let method = request.method().to_string();
        self.handshake_call(request)
            .await?
            .map_err(|error| request_error(&method, &error))
    }

    /// The model choice from the latest `config_option_update`. The value
    /// goes to the first caller only.
    pub fn take_model_update(&self) -> Option<ModelChoice> {
        lock(&self.shared).model_update.take()
    }

    /// Whether the agent takes a Streamable HTTP MCP server. `false` before
    /// the handshake.
    pub fn agent_supports_http_mcp(&self) -> bool {
        self.caps.mcp_capabilities.http
    }

    /// Whether the agent advertised `sessionCapabilities.close`. `false`
    /// before the handshake.
    pub fn agent_supports_session_close(&self) -> bool {
        self.caps.session_capabilities.close.is_some()
    }
}

/// Lock the shared state. A panic in a holder does not make the state
/// wrong, because each holder writes whole values.
fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The tool id and name a session update announces, when it names one.
///
/// `name` is the ACP field, stable since schema 1.9.1. An agent that sends
/// no `name` for an MCP call puts the identity in `rawInput` instead —
/// codex-acp sends `{server, tool, arguments}` — so the wire form of its
/// own title, `mcp.<server>.<tool>`, is rebuilt from those two fields.
///
/// `title` is never read as a name. It is prose for a person, and a name
/// taken from prose matches no rule.
fn announced_tool_name(update: &SessionUpdate) -> Option<(ToolCallId, String)> {
    match update {
        // The first frame of a tool call, which states what the tool is.
        SessionUpdate::ToolCall(call) => {
            let name = match &call.name {
                Some(name) => name.clone(),
                None => mcp_tool_name(call.raw_input.as_ref()?)?,
            };
            Some((call.tool_call_id.clone(), name))
        }
        // A refinement. An absent `name` means "unchanged", so only a name
        // this frame states is read: a name derived from a later frame's raw
        // input would be a guess that overwrites what the agent said.
        SessionUpdate::ToolCallUpdate(update) => {
            Some((update.tool_call_id.clone(), update.fields.name.clone()?))
        }
        _ => None,
    }
}

/// The `mcp.<server>.<tool>` name an MCP call's raw input carries.
fn mcp_tool_name(raw_input: &serde_json::Value) -> Option<String> {
    let server = raw_input.get("server")?.as_str()?;
    let tool = raw_input.get("tool")?.as_str()?;
    Some(format!("mcp.{server}.{tool}"))
}

/// Keep the tool names and the model choice. Send the other updates to the
/// turn that runs.
fn route_update(shared: &Mutex<Shared>, update: SessionUpdate) {
    let mut shared = lock(shared);

    // Recorded before the routing below, and whether or not a turn is
    // running: an agent that announces a tool call and then asks about it
    // must find the name it sent.
    if let Some((id, name)) = announced_tool_name(&update) {
        shared.tool_names.insert(id, name);
    }

    match update {
        // The one option that Crucible tracks is the model selector. The
        // handle reads the choice with `take_model_update` after the turn.
        SessionUpdate::ConfigOptionUpdate(update) => {
            if let Some(choice) = ModelChoice::from_config_options(&update.config_options) {
                tracing::info!(model = %choice.current, "ACP agent reported a model change");
                shared.model_update = Some(choice);
            }
        }
        update => match shared.turn.as_ref() {
            Some(turn) => {
                let _ = turn.updates.send(update);
            }
            None => tracing::debug!(?update, "Ignoring a session update outside a turn"),
        },
    }
}

/// Give a permission request the tool name its own `tool_call` frame
/// announced, when the request itself names no tool.
///
/// A request that names a tool keeps that name: the agent is the authority
/// on what it is about to run, and a later frame must not rewrite it.
fn name_the_tool_call(shared: &Shared, request: &mut RequestPermissionRequest) {
    if request.tool_call.fields.name.is_some() {
        return;
    }
    request.tool_call.fields.name = shared
        .tool_names
        .get(&request.tool_call.tool_call_id)
        .cloned();
}

async fn ask(
    permission: Option<PermissionRequestHandler>,
    request: RequestPermissionRequest,
) -> RequestPermissionOutcome {
    match permission {
        Some(handler) => handler(request).await,
        None => {
            tracing::warn!("No ACP permission handler configured; cancelling request");
            RequestPermissionOutcome::Cancelled
        }
    }
}

/// Whether a request failed because the connection ended, not because the
/// agent answered with an error.
///
/// A clean EOF gives the SDK's transport-closed error. A connection that
/// fails, for example on a write to a dead agent, drops the reply channel,
/// and the SDK then makes an internal error with the data "response to
/// `<method>` never received". The SDK gives no typed mark for that case,
/// so the text is the signal. The tests that kill an agent mid-turn pin it.
/// The text comes from `agent-client-protocol` 2.0.0. Check it again when
/// the SDK version changes.
fn connection_lost(error: &agent_client_protocol::Error) -> bool {
    let never_received = || {
        error
            .data
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .is_some_and(|data| {
                data.starts_with("response to `") && data.contains("` never received")
            })
    };
    agent_client_protocol::is_incoming_transport_closed(error)
        || (error.code == agent_client_protocol::ErrorCode::InternalError && never_received())
}

/// The client error for a failed request. A closed transport is a lost
/// connection. An agent error keeps the agent's text, because that text
/// tells the user the cause, for example a missing login.
fn request_error(method: &str, error: &agent_client_protocol::Error) -> ClientError {
    if connection_lost(error) {
        return ClientError::Connection(format!("the agent closed the connection during {method}"));
    }
    let error = serde_json::to_value(error).unwrap_or_default();
    ClientError::Session(format!(
        "{method} failed: {}",
        streaming::describe_rpc_error(&error)
    ))
}
