//! Daemon-side implementation of [`DaemonSessionApi`] for Lua plugins.
//!
//! Bridges `cru.sessions.*` Lua calls to the daemon's `SessionManager`,
//! `AgentManager`, and event broadcast infrastructure.

use crate::agent_manager::AgentManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::SessionManager;
use crucible_core::session::SessionType;
use crucible_lua::{DaemonSessionApi, ResponsePart};
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
        kiln: Option<String>,
        workspace: Option<String>,
        connected_kilns: Vec<String>,
    ) -> BoxFut<serde_json::Value> {
        bridge_async!(self.session_manager, |sm| async move {
            let st = match session_type.as_str() {
                "chat" => SessionType::Chat,
                "agent" => SessionType::Agent,
                "workflow" => SessionType::Workflow,
                other => return Err(format!("Invalid session type: {}", other)),
            };
            let kiln_path = kiln
                .map(PathBuf::from)
                .unwrap_or_else(crucible_config::crucible_home);
            let connected: Vec<PathBuf> = connected_kilns.into_iter().map(PathBuf::from).collect();
            let session = sm
                .create_session(st, kiln_path, workspace.map(PathBuf::from), connected)
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

            tracing::debug!(
                session_id = %session_id,
                "Lua subscribe: creating forwarder task"
            );

            let sid = session_id.clone();
            tokio::spawn(async move {
                tracing::debug!(session_id = %sid, "Forwarder task started");
                let mut forwarded = 0u64;
                loop {
                    match broadcast_rx.recv().await {
                        Ok(event) if event.session_id == sid => {
                            forwarded += 1;
                            let json = serde_json::json!({
                                "type": event.event,
                                "session_id": event.session_id,
                                "data": event.data,
                            });
                            if tx.send(json).is_err() {
                                tracing::debug!(
                                    session_id = %sid,
                                    forwarded,
                                    "Forwarder: mpsc receiver dropped"
                                );
                                break;
                            }
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                session_id = %sid,
                                lagged = n,
                                "Forwarder: broadcast lagged, lost events"
                            );
                            continue;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            tracing::debug!(
                                session_id = %sid,
                                forwarded,
                                "Forwarder: broadcast closed"
                            );
                            break;
                        }
                    }
                }
                tracing::debug!(
                    session_id = %sid,
                    forwarded,
                    "Forwarder task exiting"
                );
            });

            Ok(rx)
        })
    }

    fn unsubscribe(&self, _session_id: String) -> BoxFut<()> {
        // Unsubscribe is handled by dropping the receiver from subscribe().
        // The spawned task will detect the closed channel and exit.
        Box::pin(async { Ok(()) })
    }

    fn send_and_collect(
        &self,
        session_id: String,
        content: String,
        timeout_secs: Option<f64>,
        max_tool_result_len: Option<usize>,
    ) -> BoxFut<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>> {
        let am = self.agent_manager.clone();
        let event_tx = self.event_tx.clone();
        Box::pin(async move {
            let timeout = std::time::Duration::from_secs_f64(timeout_secs.unwrap_or(120.0));
            let max_result = max_tool_result_len.unwrap_or(500);

            // Subscribe to broadcast BEFORE sending so we don't miss early events
            let mut broadcast_rx = event_tx.subscribe();

            let _msg_id = am
                .send_message(&session_id, content, &event_tx)
                .await
                .map_err(|e| e.to_string())?;

            let (part_tx, part_rx) = tokio::sync::mpsc::unbounded_channel();

            tokio::spawn(async move {
                let mut text_buf = String::new();
                let mut deadline = tokio::time::Instant::now() + timeout;

                macro_rules! emit {
                    ($part:expr) => {
                        if part_tx.send($part).is_err() {
                            tracing::debug!(session_id = %session_id, "part receiver dropped, stopping");
                            return;
                        }
                    };
                }

                let flush_text = |buf: &mut String, tx: &tokio::sync::mpsc::UnboundedSender<ResponsePart>| -> bool {
                    if !buf.is_empty() {
                        tx.send(ResponsePart::Text { content: std::mem::take(buf) }).is_ok()
                    } else {
                        true
                    }
                };

                loop {
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        tracing::warn!(session_id = %session_id, "send_and_collect: timeout");
                        break;
                    }

                    match tokio::time::timeout(remaining, broadcast_rx.recv()).await {
                        Ok(Ok(event)) if event.session_id == session_id => {
                            match event.event.as_str() {
                                "text_delta" => {
                                    if let Some(c) = event.data.get("content").and_then(|v| v.as_str()) {
                                        text_buf.push_str(c);
                                    }
                                }
                                "tool_call" => {
                                    if !flush_text(&mut text_buf, &part_tx) { return; }
                                    let tool = event.data.get("tool")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                        .to_string();
                                    let args_brief = truncate_json_preview(
                                        event.data.get("args"),
                                        120,
                                    );
                                    emit!(ResponsePart::ToolCall { tool, args_brief });
                                }
                                "tool_result" => {
                                    if !flush_text(&mut text_buf, &part_tx) { return; }
                                    let tool = event.data.get("tool")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                        .to_string();
                                    let result_data = event.data.get("result");
                                    let error_str = result_data
                                        .and_then(|r| r.get("error"))
                                        .and_then(|v| v.as_str());
                                    let result_brief = match error_str {
                                        Some(e) => truncate_str(e, max_result),
                                        None => truncate_json_preview(result_data, max_result),
                                    };
                                    emit!(ResponsePart::ToolResult {
                                        tool,
                                        result_brief,
                                        is_error: error_str.is_some(),
                                    });
                                }
                                "thinking" => {
                                    if !flush_text(&mut text_buf, &part_tx) { return; }
                                    if let Some(content) = event.data.get("content")
                                        .and_then(|v| v.as_str())
                                        .filter(|s| !s.is_empty())
                                    {
                                        emit!(ResponsePart::Thinking {
                                            content: content.to_string(),
                                        });
                                    }
                                }
                                "interaction_requested" => {
                                    if !flush_text(&mut text_buf, &part_tx) { return; }
                                    // Reset deadline — user needs time to respond to the prompt
                                    deadline = tokio::time::Instant::now() + timeout;
                                    let request_id = event.data.get("request_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let (tool, description) = extract_permission_info(&event.data);
                                    if !request_id.is_empty() {
                                        emit!(ResponsePart::PermissionRequest {
                                            request_id,
                                            tool,
                                            description,
                                        });
                                    }
                                }
                                "message_complete" | "response_complete" | "response_done" | "ended" => {
                                    let _ = flush_text(&mut text_buf, &part_tx);
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Ok(Ok(_)) => {}
                        Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                            tracing::warn!(session_id = %session_id, lagged = n, "send_and_collect: lagged");
                        }
                        Ok(Err(broadcast::error::RecvError::Closed)) => {
                            let _ = flush_text(&mut text_buf, &part_tx);
                            break;
                        }
                        Err(_) => {
                            tracing::warn!(session_id = %session_id, "send_and_collect: timeout");
                            let _ = flush_text(&mut text_buf, &part_tx);
                            break;
                        }
                    }
                }
            });

            Ok(part_rx)
        })
    }
}

/// Extract tool name and human-readable description from a permission request's data payload.
fn extract_permission_info(data: &serde_json::Value) -> (String, String) {
    let action = data.get("request").and_then(|r| r.get("action"));
    let action_type = action
        .and_then(|a| a.get("type"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    fn collect_str_array<'a>(action: Option<&'a serde_json::Value>, key: &str) -> Vec<&'a str> {
        action
            .and_then(|a| a.get(key))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default()
    }

    match action_type {
        "bash" => {
            let description = collect_str_array(action, "tokens").join(" ");
            ("bash".to_string(), description)
        }
        "read" | "write" => {
            let description = collect_str_array(action, "segments").join("/");
            (action_type.to_string(), description)
        }
        "tool" => {
            let name = action
                .and_then(|a| a.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            (name.to_string(), truncate_json_preview(action.and_then(|a| a.get("args")), 200))
        }
        _ => ("unknown".to_string(), "unrecognized action type".to_string()),
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len.saturating_sub(3);
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}

fn truncate_json_preview(val: Option<&serde_json::Value>, max_len: usize) -> String {
    val.map(|v| truncate_str(&v.to_string(), max_len))
        .unwrap_or_default()
}
