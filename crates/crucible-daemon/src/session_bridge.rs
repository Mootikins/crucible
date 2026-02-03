//! Daemon-side implementation of [`DaemonSessionApi`] for Lua plugins.
//!
//! Bridges `cru.sessions.*` Lua calls to the daemon's `SessionManager`,
//! `AgentManager`, and event broadcast infrastructure.

use crate::agent_manager::AgentManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::SessionManager;
use crucible_core::session::SessionType;
use crucible_lua::DaemonSessionApi;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Boxed future type alias used by all [`DaemonSessionApi`] methods.
type BoxFut<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;

/// Implements [`DaemonSessionApi`] using the daemon's real managers.
pub struct DaemonSessionBridge {
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
}

impl DaemonSessionBridge {
    pub fn new(
        session_manager: Arc<SessionManager>,
        agent_manager: Arc<AgentManager>,
        event_tx: broadcast::Sender<SessionEventMessage>,
    ) -> Self {
        Self {
            session_manager,
            agent_manager,
            event_tx,
        }
    }
}

/// Reduces boilerplate for trait methods that clone manager Arc(s) and Box::pin an async block.
///
/// Usage: `bridge_async!(self.session_manager, |sm| async move { ... })`
///        `bridge_async!(self.agent_manager, self.event_tx, |am, tx| async move { ... })`
macro_rules! bridge_async {
    ($self:ident . $field:ident, |$binding:ident| $body:expr) => {{
        let $binding = $self.$field.clone();
        Box::pin($body)
    }};
    ($self:ident . $field1:ident, $self2:ident . $field2:ident, |$b1:ident, $b2:ident| $body:expr) => {{
        let $b1 = $self.$field1.clone();
        let $b2 = $self2.$field2.clone();
        Box::pin($body)
    }};
}

impl DaemonSessionApi for DaemonSessionBridge {
    fn create_session(
        &self,
        session_type: String,
        kiln: String,
        workspace: Option<String>,
    ) -> BoxFut<serde_json::Value> {
        bridge_async!(self.session_manager, |sm| async move {
            let st = match session_type.as_str() {
                "chat" => SessionType::Chat,
                "agent" => SessionType::Agent,
                "workflow" => SessionType::Workflow,
                other => return Err(format!("Invalid session type: {}", other)),
            };
            let session = sm
                .create_session(st, PathBuf::from(&kiln), workspace.map(PathBuf::from), Vec::new())
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "id": session.id,
                "session_type": session.session_type.as_prefix(),
                "kiln": session.kiln,
                "state": format!("{}", session.state),
            }))
        })
    }

    fn get_session(
        &self,
        session_id: String,
    ) -> BoxFut<Option<serde_json::Value>> {
        bridge_async!(self.session_manager, |sm| async move {
            Ok(sm.get_session(&session_id).map(|s| {
                serde_json::json!({
                    "id": s.id,
                    "session_type": s.session_type.as_prefix(),
                    "kiln": s.kiln,
                    "state": format!("{}", s.state),
                    "title": s.title,
                })
            }))
        })
    }

    fn list_sessions(&self) -> BoxFut<Vec<serde_json::Value>> {
        bridge_async!(self.session_manager, |sm| async move {
            Ok(sm
                .list_sessions()
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "session_type": s.session_type.as_prefix(),
                        "kiln": s.kiln,
                        "state": format!("{}", s.state),
                        "title": s.title,
                    })
                })
                .collect())
        })
    }

    fn configure_agent(
        &self,
        session_id: String,
        agent_config: serde_json::Value,
    ) -> BoxFut<()> {
        bridge_async!(self.agent_manager, |am| async move {
            let agent: crucible_core::session::SessionAgent =
                serde_json::from_value(agent_config)
                    .map_err(|e| format!("Invalid agent config: {}", e))?;
            am.configure_agent(&session_id, agent)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn send_message(
        &self,
        session_id: String,
        content: String,
    ) -> BoxFut<String> {
        bridge_async!(self.agent_manager, self.event_tx, |am, event_tx| async move {
            am.send_message(&session_id, content, &event_tx)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn cancel(&self, session_id: String) -> BoxFut<bool> {
        bridge_async!(self.agent_manager, |am| async move {
            Ok(am.cancel(&session_id).await)
        })
    }

    fn pause(&self, session_id: String) -> BoxFut<()> {
        bridge_async!(self.session_manager, |sm| async move {
            sm.pause_session(&session_id)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }

    fn resume(&self, session_id: String) -> BoxFut<()> {
        bridge_async!(self.session_manager, |sm| async move {
            sm.resume_session(&session_id)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }

    fn end_session(&self, session_id: String) -> BoxFut<()> {
        bridge_async!(self.session_manager, self.agent_manager, |sm, am| async move {
            sm.end_session(&session_id)
                .await
                .map_err(|e| e.to_string())?;
            am.cleanup_session(&session_id);
            Ok(())
        })
    }

    fn respond_to_permission(
        &self,
        session_id: String,
        request_id: String,
        response: serde_json::Value,
    ) -> BoxFut<()> {
        bridge_async!(self.agent_manager, |am| async move {
            let perm_response: crucible_core::interaction::PermResponse =
                serde_json::from_value(response)
                    .map_err(|e| format!("Invalid permission response: {}", e))?;
            am.respond_to_permission(&session_id, &request_id, perm_response)
                .map_err(|e| e.to_string())
        })
    }

    fn subscribe(
        &self,
        session_id: String,
    ) -> BoxFut<tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>> {
        bridge_async!(self.event_tx, |event_tx| async move {
            let mut broadcast_rx = event_tx.subscribe();
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

            tokio::spawn(async move {
                loop {
                    match broadcast_rx.recv().await {
                        Ok(event) if event.session_id == session_id => {
                            let json = serde_json::json!({
                                "type": event.event,
                                "session_id": event.session_id,
                                "data": event.data,
                            });
                            if tx.send(json).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            Ok(rx)
        })
    }

    fn unsubscribe(&self, _session_id: String) -> BoxFut<()> {
        // Unsubscribe is handled by dropping the receiver from subscribe().
        // The spawned task will detect the closed channel and exit.
        Box::pin(async { Ok(()) })
    }
}
