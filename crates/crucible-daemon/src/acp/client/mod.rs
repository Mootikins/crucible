//! The daemon side of one ACP agent connection.
//!
//! The `agent-client-protocol` SDK owns the JSON-RPC framing, the request
//! correlation and the inbound dispatch. This module owns what Crucible adds
//! on top: the agent process, the handshake, the permission bridge, and the
//! translation of a turn's session updates into [`TurnEvent`]s.
//!
//! The SDK connection runs on its own task. The client holds a clone of its
//! [`ConnectionTo<Agent>`], so a request and a turn can run at the same time.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, PermissionOption, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SessionNotification, SessionUpdate,
};
use agent_client_protocol::{Agent, Client, ConnectTo, ConnectionTo, JsonRpcRequest};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::acp::session::ModelChoice;
use crate::acp::{ClientError, Result};
use crucible_core::turn::TurnEvent;
use crucible_core::types::{AgentKeys, CanonicalToolCall};

mod connection;
mod recording;
pub mod replay;
pub(crate) mod streaming;
mod tool_table;
mod tools;
mod types;

#[cfg(test)]
mod tests;

pub use recording::{Direction, FixtureHeader, FrameRecord, Recorder};
pub use types::ClientConfig;

pub type PermissionOutcomeFuture = Pin<Box<dyn Future<Output = RequestPermissionOutcome> + Send>>;
/// The permission path. It gets the canonical call that a
/// `session/request_permission` asks about, and the options of the agent.
/// It never gets the wire form of the call.
pub type PermissionRequestHandler =
    Arc<dyn Fn(CanonicalToolCall, Vec<PermissionOption>) -> PermissionOutcomeFuture + Send + Sync>;

/// The deadline of one handshake request. The old client allowed five
/// minutes per read, and an agent that `npx` must first download can use
/// much of that on `initialize`.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(300);

/// The turn that runs now. The notification handler applies each update of
/// the turn to `state` and sends the chunks to `out`. A permission request
/// joins the tool table of `state`, and races the handler with `cancel`.
///
/// The SDK runs the notification handler and the request handler in the
/// order of the wire. So a permission request finds every frame that the
/// agent sent before it.
struct Turn {
    out: mpsc::UnboundedSender<TurnEvent>,
    state: types::StreamingState,
    cancel: CancellationToken,
}

/// State that the notification handler writes and the client reads.
#[derive(Default)]
struct Shared {
    turn: Option<Turn>,
    /// The model choice from the latest `config_option_update`.
    model_update: Option<ModelChoice>,
    /// The key table of the agent, from its profile. It classifies each
    /// tool call.
    keys: Vec<AgentKeys>,
}

/// One connection to one ACP agent.
pub struct CrucibleAcpClient {
    cx: ConnectionTo<Agent>,
    agent_name: String,
    config: ClientConfig,
    shared: Arc<Mutex<Shared>>,
    /// One turn at a time. [`Self::prompt`] holds it until the agent ends
    /// the turn, so the next turn and the goodbye at drop wait for that end.
    turn_gate: tokio::sync::Mutex<()>,
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
        let shared = Arc::new(Mutex::new(Shared {
            keys: config.tools.clone(),
            ..Shared::default()
        }));
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
                    let agent_name = agent_name.clone();
                    async move |request: RequestPermissionRequest,
                                responder,
                                cx: ConnectionTo<Agent>| {
                        let (call, cancel) = {
                            let mut shared = lock(&shared);
                            let Shared { turn, keys, .. } = &mut *shared;
                            match turn {
                                Some(turn) => (
                                    turn.state
                                        .tool_calls
                                        .permission_call(&request.tool_call, keys),
                                    turn.cancel.clone(),
                                ),
                                // A request outside a turn has no frames to join.
                                None => (
                                    tool_table::ToolCallTable::for_agent(&agent_name)
                                        .permission_call(&request.tool_call, keys),
                                    CancellationToken::default(),
                                ),
                            }
                        };
                        let options = request.options;
                        let permission = permission.clone();
                        // The dispatch loop waits for a handler. A user who
                        // takes a minute to answer must not stop the updates
                        // of the turn, so the answer waits on its own task.
                        cx.spawn(async move {
                            // A cancel wins over an answer that is ready at
                            // the same time: the turn ended.
                            let outcome = tokio::select! {
                                biased;
                                () = cancel.cancelled() => RequestPermissionOutcome::Cancelled,
                                outcome = ask(permission, call, options) => outcome,
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
            turn_gate: tokio::sync::Mutex::default(),
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

/// Keep the model choice. Apply the other updates to the turn that runs.
fn route_update(shared: &Mutex<Shared>, update: SessionUpdate) {
    let mut shared = lock(shared);
    let Shared {
        turn,
        model_update,
        keys,
    } = &mut *shared;

    match update {
        // The one option that Crucible tracks is the model selector. The
        // handle reads the choice with `take_model_update` after the turn.
        SessionUpdate::ConfigOptionUpdate(update) => {
            if let Some(choice) = ModelChoice::from_config_options(&update.config_options) {
                tracing::info!(model = %choice.current, "ACP agent reported a model change");
                *model_update = Some(choice);
            }
        }
        update => match turn {
            Some(turn) => streaming::apply_update(update, &mut turn.state, &turn.out, keys),
            None => tracing::debug!(?update, "Ignoring a session update outside a turn"),
        },
    }
}

async fn ask(
    permission: Option<PermissionRequestHandler>,
    call: CanonicalToolCall,
    options: Vec<PermissionOption>,
) -> RequestPermissionOutcome {
    match permission {
        Some(handler) => handler(call, options).await,
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
/// The text comes from `agent-client-protocol` 2.2.0 (`jsonrpc.rs`, three
/// sites). Check it again when the SDK version changes.
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
