//! Daemon-side implementation of [`DaemonSessionApi`] for Lua plugins.
//!
//! Bridges `cru.sessions.*` Lua calls to the daemon's `SessionManager`,
//! `AgentManager`, and event broadcast infrastructure.

use crate::agent_manager::AgentManager;
use crate::protocol::SessionEventMessage;
use crate::session_manager::SessionManager;
use crate::session_storage::{FileSessionStorage, SessionStorage};
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
            let st: SessionType = session_type
                .parse()
                .map_err(|_| format!("Invalid session type: {}", session_type))?;
            let kiln_path = kiln
                .map(PathBuf::from)
                .unwrap_or_else(crucible_core::config::crucible_home);
            let connected: Vec<PathBuf> = connected_kilns.into_iter().map(PathBuf::from).collect();
            let session = sm
                .create_session(st, kiln_path, workspace.map(PathBuf::from), connected, None)
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

    fn get_session(&self, session_id: String) -> BoxFut<Option<serde_json::Value>> {
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

    fn configure_agent(&self, session_id: String, agent_config: serde_json::Value) -> BoxFut<()> {
        bridge_async!(self.agent_manager, |am| async move {
            let agent: crucible_core::session::SessionAgent = serde_json::from_value(agent_config)
                .map_err(|e| format!("Invalid agent config: {}", e))?;
            am.configure_agent(&session_id, agent)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn send_message(&self, session_id: String, content: String) -> BoxFut<String> {
        bridge_async!(
            self.agent_manager,
            self.event_tx,
            |am, event_tx| async move {
                am.send_message(&session_id, content, &event_tx, true, None)
                    .await
                    .map_err(|e| e.to_string())
            }
        )
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
        bridge_async!(
            self.session_manager,
            self.agent_manager,
            |sm, am| async move {
                sm.end_session(&session_id)
                    .await
                    .map_err(|e| e.to_string())?;
                am.cleanup_session(&session_id);
                Ok(())
            }
        )
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

    fn load_messages(
        &self,
        session_id: String,
        role_filter: Option<String>,
        limit: Option<usize>,
    ) -> BoxFut<Vec<serde_json::Value>> {
        bridge_async!(self.session_manager, |sm| async move {
            if let Some(ref role) = role_filter {
                if !matches!(role.as_str(), "user" | "assistant" | "system") {
                    return Err(format!(
                        "Invalid role filter '{}': must be 'user', 'assistant', or 'system'",
                        role
                    ));
                }
            }

            let session = sm
                .get_session(&session_id)
                .ok_or_else(|| format!("Session not found: {}", session_id))?;
            let session_dir = FileSessionStorage::sessions_base(&session.kiln).join(&session_id);
            // NOTE: Loads entire session event log. For very long sessions, consider
            // adding a streaming/backwards-reading approach with index files.
            let events = crate::observe::load_events(&session_dir)
                .await
                .map_err(|e| e.to_string())?;

            let mut messages: Vec<serde_json::Value> = events
                .iter()
                .filter_map(|event| match event {
                    crate::observe::LogEvent::User { content, .. } => {
                        if role_filter.as_deref().is_some_and(|r| r != "user") {
                            return None;
                        }
                        Some(serde_json::json!({
                            "role": "user",
                            "content": content,
                            "timestamp": event.timestamp().to_rfc3339(),
                        }))
                    }
                    crate::observe::LogEvent::Assistant { content, .. } => {
                        if role_filter.as_deref().is_some_and(|r| r != "assistant") {
                            return None;
                        }
                        Some(serde_json::json!({
                            "role": "assistant",
                            "content": content,
                            "timestamp": event.timestamp().to_rfc3339(),
                        }))
                    }
                    crate::observe::LogEvent::System { content, .. } => {
                        if role_filter.as_deref().is_some_and(|r| r != "system") {
                            return None;
                        }
                        Some(serde_json::json!({
                            "role": "system",
                            "content": content,
                            "timestamp": event.timestamp().to_rfc3339(),
                        }))
                    }
                    _ => None,
                })
                .collect();

            if let Some(n) = limit {
                let start = messages.len().saturating_sub(n);
                messages = messages.split_off(start);
            }

            Ok(messages)
        })
    }

    /// Fork a session by copying messages up to an optional limit.
    ///
    /// NOTE: Bridge fork does not copy agent configuration (no AgentManager access).
    /// Callers should configure the forked session's agent separately.
    /// The RPC handler version (handle_session_fork) does copy agent config.
    fn fork_session(&self, session_id: String, up_to: Option<u64>) -> BoxFut<serde_json::Value> {
        bridge_async!(self.session_manager, |sm| async move {
            let parent = sm
                .get_session(&session_id)
                .ok_or_else(|| format!("Session not found: {}", session_id))?;

            let child = sm
                .create_session(
                    parent.session_type,
                    parent.kiln.clone(),
                    Some(parent.workspace.clone()),
                    parent.connected_kilns.clone(),
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;

            let parent_dir = FileSessionStorage::sessions_base(&parent.kiln).join(&session_id);
            let events = crate::observe::load_events(&parent_dir)
                .await
                .unwrap_or_default();

            let storage = FileSessionStorage::new();
            let mut count = 0u64;
            for event in &events {
                if let Some(limit) = up_to {
                    if count >= limit {
                        break;
                    }
                }
                match event {
                    crate::observe::LogEvent::User { .. }
                    | crate::observe::LogEvent::Assistant { .. }
                    | crate::observe::LogEvent::System { .. } => {
                        let json = serde_json::to_string(event).map_err(|e| e.to_string())?;
                        storage
                            .append_event(&child, &json)
                            .await
                            .map_err(|e| e.to_string())?;
                        count += 1;
                    }
                    _ => {}
                }
            }

            Ok(serde_json::json!({
                "id": child.id,
                "parent_id": session_id,
                "messages_copied": count,
            }))
        })
    }

    fn inject_context(&self, session_id: String, role: String, content: String) -> BoxFut<()> {
        let sm = self.session_manager.clone();
        let event_tx = self.event_tx.clone();
        Box::pin(async move {
            crate::server::session::inject_context_impl(
                &sm,
                &event_tx,
                &session_id,
                &role,
                &content,
            )
            .await
        })
    }

    fn collect_subagents(
        &self,
        job_ids: Vec<String>,
        timeout_secs: Option<f64>,
    ) -> BoxFut<Vec<serde_json::Value>> {
        let am = self.agent_manager.clone();
        Box::pin(async move {
            let timeout = std::time::Duration::from_secs_f64(timeout_secs.unwrap_or(120.0));
            let results = am
                .background_manager()
                .collect_jobs(&job_ids, timeout)
                .await;
            Ok(results)
        })
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
                .send_message(&session_id, content, &event_tx, true, None)
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

                let flush_text = |buf: &mut String,
                                  tx: &tokio::sync::mpsc::UnboundedSender<ResponsePart>|
                 -> bool {
                    if !buf.is_empty() {
                        tx.send(ResponsePart::Text {
                            content: std::mem::take(buf),
                        })
                        .is_ok()
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
                                    if let Some(c) =
                                        event.data.get("content").and_then(|v| v.as_str())
                                    {
                                        text_buf.push_str(c);
                                    }
                                }
                                "tool_call" => {
                                    if !flush_text(&mut text_buf, &part_tx) {
                                        return;
                                    }
                                    let tool = event
                                        .data
                                        .get("tool")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                        .to_string();
                                    let args_brief =
                                        truncate_json_preview(event.data.get("args"), 500);
                                    emit!(ResponsePart::ToolCall { tool, args_brief });
                                }
                                "tool_result" => {
                                    if !flush_text(&mut text_buf, &part_tx) {
                                        return;
                                    }
                                    let tool = event
                                        .data
                                        .get("tool")
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
                                    if !flush_text(&mut text_buf, &part_tx) {
                                        return;
                                    }
                                    if let Some(content) = event
                                        .data
                                        .get("content")
                                        .and_then(|v| v.as_str())
                                        .filter(|s| !s.is_empty())
                                    {
                                        emit!(ResponsePart::Thinking {
                                            content: content.to_string(),
                                        });
                                    }
                                }
                                "interaction_requested" => {
                                    if !flush_text(&mut text_buf, &part_tx) {
                                        return;
                                    }
                                    // Reset deadline — user needs time to respond to the prompt
                                    deadline = tokio::time::Instant::now() + timeout;
                                    let request_id = event
                                        .data
                                        .get("request_id")
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
                                "message_complete" | "response_complete" | "response_done"
                                | "ended" => {
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

    fn cache_stats(&self, session_id: String) -> BoxFut<serde_json::Value> {
        bridge_async!(self.agent_manager, |am| async move {
            let stats = am.get_cache_stats(&session_id);
            Ok(serde_json::json!({
                "session_id": session_id,
                "hits": stats.hits,
                "misses": stats.misses,
                "read_tokens": stats.read_tokens,
                "creation_tokens": stats.creation_tokens,
                "prompt_tokens": stats.prompt_tokens,
                "completion_tokens": stats.completion_tokens,
                "hit_rate": stats.hit_rate(),
            }))
        })
    }

    fn context_usage(&self, session_id: String) -> BoxFut<serde_json::Value> {
        bridge_async!(self.agent_manager, |am| async move {
            am.get_context_usage(&session_id).map_err(|e| e.to_string())
        })
    }

    fn compact(&self, session_id: String) -> BoxFut<()> {
        bridge_async!(self.session_manager, |sm| async move {
            sm.request_compaction(&session_id)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
    }

    fn remove_messages(
        &self,
        session_id: String,
        range: serde_json::Value,
    ) -> BoxFut<usize> {
        bridge_async!(self.agent_manager, |am| async move {
            let parsed = parse_range(&range)?;
            am.remove_messages(&session_id, parsed)
                .await
                .map_err(|e| e.to_string())
        })
    }
}

/// Decode a JSON range descriptor into a [`Range`] value.
///
/// Accepted shapes:
/// * `{ "type": "all" }`
/// * `{ "type": "last" | "first", "n": N }`
/// * `{ "type": "indices", "start": S, "end": E }` (half-open `[S, E)`)
fn parse_range(
    v: &serde_json::Value,
) -> Result<crucible_core::traits::context_ops::Range, String> {
    use crucible_core::traits::context_ops::Range;
    let obj = v
        .as_object()
        .ok_or_else(|| "range must be an object".to_string())?;
    let ty = obj.get("type").and_then(|x| x.as_str()).unwrap_or("");
    match ty {
        "all" => Ok(Range::All),
        "last" => {
            let n = obj
                .get("n")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| "range.n required for type='last'".to_string())?
                as usize;
            Ok(Range::Last(n))
        }
        "first" => {
            let n = obj
                .get("n")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| "range.n required for type='first'".to_string())?
                as usize;
            Ok(Range::First(n))
        }
        "indices" => {
            let start = obj
                .get("start")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| "range.start required for type='indices'".to_string())?
                as usize;
            let end = obj
                .get("end")
                .and_then(|x| x.as_u64())
                .ok_or_else(|| "range.end required for type='indices'".to_string())?
                as usize;
            Ok(Range::Indices(start..end))
        }
        other => Err(format!("unknown range type '{other}'")),
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
            (
                name.to_string(),
                truncate_json_preview(action.and_then(|a| a.get("args")), 200),
            )
        }
        _ => (
            "unknown".to_string(),
            "unrecognized action type".to_string(),
        ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_manager::{AgentManager, AgentManagerParams};
    use crate::background_manager::BackgroundJobManager;
    use crate::kiln_manager::KilnManager;
    use crate::session_manager::SessionManager;
    use crate::session_storage::FileSessionStorage;
    use crate::tools::workspace::WorkspaceTools;
    use crucible_core::config::BackendType;
    use crucible_core::session::{OutputValidation, SessionAgent, SessionType};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn build_test_agent_manager(session_manager: Arc<SessionManager>) -> Arc<AgentManager> {
        let (event_tx, _) = broadcast::channel(16);
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx));
        Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager: Arc::new(KilnManager::new()),
            session_manager,
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(PathBuf::from("/tmp"))),
        }))
    }

    fn make_test_agent(context_budget: Option<usize>) -> SessionAgent {
        SessionAgent {
            agent_type: "internal".to_string(),
            agent_name: None,
            provider_key: Some("ollama".to_string()),
            provider: BackendType::Ollama,
            model: "llama3.2".to_string(),
            system_prompt: "You are helpful.".to_string(),
            temperature: Some(0.7),
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
            context_budget,
            context_strategy: Default::default(),
            context_window: None,
            output_validation: OutputValidation::default(),
            validation_retries: 3,
            autocompact_threshold: None,
        }
    }

    #[test]
    fn test_daemon_session_bridge_construction() {
        // Create minimal dependencies for testing
        let (event_tx, _) = broadcast::channel(100);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = Arc::new(SessionManager::new());
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager,
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(PathBuf::from("/tmp"))),
        }));

        // Construct bridge
        let bridge = DaemonSessionBridge::new(
            session_manager.clone(),
            agent_manager.clone(),
            event_tx.clone(),
        );

        // Verify bridge was created (no panic)
        assert!(std::mem::size_of_val(&bridge) > 0);
    }

    #[test]
    fn test_daemon_session_bridge_delegates_to_managers() {
        // Verify Arc cloning works (bridge holds Arc references)
        let (event_tx, _) = broadcast::channel(100);
        let kiln_manager = Arc::new(KilnManager::new());
        let session_manager = Arc::new(SessionManager::new());
        let background_manager = Arc::new(BackgroundJobManager::new(event_tx.clone()));
        let agent_manager = Arc::new(AgentManager::new(AgentManagerParams {
            kiln_manager,
            session_manager: session_manager.clone(),
            background_manager,
            mcp_gateway: None,
            llm_config: None,
            acp_config: None,
            permission_config: None,
            plugin_loader: None,
            workspace_tools: Arc::new(WorkspaceTools::new(PathBuf::from("/tmp"))),
        }));

        let sm_strong_count = Arc::strong_count(&session_manager);
        let am_strong_count = Arc::strong_count(&agent_manager);

        let _bridge = DaemonSessionBridge::new(
            session_manager.clone(),
            agent_manager.clone(),
            event_tx.clone(),
        );

        // Verify Arc references are held (strong count increased)
        assert_eq!(Arc::strong_count(&session_manager), sm_strong_count + 1);
        assert_eq!(Arc::strong_count(&agent_manager), am_strong_count + 1);
    }

    #[test]
    fn parse_range_accepts_known_types() {
        use crucible_core::traits::context_ops::Range;
        assert!(matches!(
            parse_range(&serde_json::json!({"type": "all"})).unwrap(),
            Range::All
        ));
        assert!(matches!(
            parse_range(&serde_json::json!({"type": "last", "n": 3})).unwrap(),
            Range::Last(3)
        ));
        assert!(matches!(
            parse_range(&serde_json::json!({"type": "first", "n": 2})).unwrap(),
            Range::First(2)
        ));
        match parse_range(&serde_json::json!({"type": "indices", "start": 1, "end": 4})).unwrap() {
            Range::Indices(r) => assert_eq!(r, 1..4),
            _ => panic!("expected Indices"),
        }
    }

    #[test]
    fn parse_range_rejects_unknown_type() {
        let err = parse_range(&serde_json::json!({"type": "bogus"})).unwrap_err();
        assert!(err.contains("unknown range type"), "got: {err}");
    }

    #[test]
    fn parse_range_requires_n_for_last_and_first() {
        let err = parse_range(&serde_json::json!({"type": "last"})).unwrap_err();
        assert!(err.contains("range.n required"), "got: {err}");
        let err = parse_range(&serde_json::json!({"type": "first"})).unwrap_err();
        assert!(err.contains("range.n required"), "got: {err}");
    }

    #[test]
    fn parse_range_requires_start_end_for_indices() {
        let err = parse_range(&serde_json::json!({"type": "indices", "start": 0})).unwrap_err();
        assert!(err.contains("range.end required"), "got: {err}");
    }

    #[tokio::test]
    async fn context_usage_returns_expected_shape() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = build_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, make_test_agent(Some(10_000)))
            .await
            .unwrap();

        let (event_tx, _) = broadcast::channel(16);
        let bridge =
            DaemonSessionBridge::new(session_manager.clone(), agent_manager.clone(), event_tx);

        let usage = bridge.context_usage(session.id.clone()).await.unwrap();
        let obj = usage.as_object().expect("expected object");
        assert!(obj.contains_key("messages"));
        assert!(obj.contains_key("prompt_tokens"));
        assert!(obj.contains_key("budget"));
        assert!(obj.contains_key("percent"));
        assert_eq!(obj.get("budget").and_then(|v| v.as_u64()), Some(10_000));
        // No turn has run, so no tree, no tokens.
        assert_eq!(obj.get("messages").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(obj.get("prompt_tokens").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(obj.get("percent").and_then(|v| v.as_f64()), Some(0.0));
    }

    #[tokio::test]
    async fn compact_transitions_session_to_compacting() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = build_test_agent_manager(session_manager.clone());
        let (event_tx, _) = broadcast::channel(16);
        let bridge =
            DaemonSessionBridge::new(session_manager.clone(), agent_manager.clone(), event_tx);

        bridge.compact(session.id.clone()).await.unwrap();
        let after = session_manager.get_session(&session.id).unwrap();
        assert_eq!(
            format!("{}", after.state),
            "compacting",
            "session should be in compacting state after compact()"
        );
    }

    #[tokio::test]
    async fn remove_messages_last_n_rewinds_tree() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = build_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, make_test_agent(None))
            .await
            .unwrap();

        // Seed the tree with three non-root nodes: User → Agent → User.
        let tree = agent_manager.get_or_create_session_tree(&session.id);
        {
            let mut t = tree.lock().await;
            let root = t.root();
            let u1 = t.add_child_and_advance(
                root,
                crucible_core::turn::NodeContent::User { text: "u1".into() },
            );
            let a1 = t.add_child_and_advance(
                u1,
                crucible_core::turn::NodeContent::Agent { text: "a1".into() },
            );
            t.add_child_and_advance(
                a1,
                crucible_core::turn::NodeContent::User { text: "u2".into() },
            );
            // sanity: path has root + 3 nodes
            let cur = t.current();
            assert_eq!(t.path_to_here(cur).len(), 4);
        }

        let (event_tx, _) = broadcast::channel(16);
        let bridge = DaemonSessionBridge::new(session_manager.clone(), agent_manager, event_tx);

        let removed = bridge
            .remove_messages(session.id.clone(), serde_json::json!({"type": "last", "n": 2}))
            .await
            .unwrap();
        assert_eq!(removed, 2);

        let tree = bridge
            .agent_manager
            .get_session_tree(&session.id)
            .expect("tree should exist");
        let t = tree.lock().await;
        assert_eq!(
            t.path_to_here(t.current()).len(),
            2,
            "expected root + 1 surviving node"
        );
    }

    #[tokio::test]
    async fn remove_messages_indices_truncates_from_start() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = build_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, make_test_agent(None))
            .await
            .unwrap();

        // Seed the tree with three non-root nodes.
        let tree = agent_manager.get_or_create_session_tree(&session.id);
        {
            let mut t = tree.lock().await;
            let root = t.root();
            let u1 = t.add_child_and_advance(
                root,
                crucible_core::turn::NodeContent::User { text: "u1".into() },
            );
            let a1 = t.add_child_and_advance(
                u1,
                crucible_core::turn::NodeContent::Agent { text: "a1".into() },
            );
            t.add_child_and_advance(
                a1,
                crucible_core::turn::NodeContent::User { text: "u2".into() },
            );
        }

        let (event_tx, _) = broadcast::channel(16);
        let bridge = DaemonSessionBridge::new(session_manager.clone(), agent_manager, event_tx);

        let removed = bridge
            .remove_messages(
                session.id.clone(),
                serde_json::json!({"type": "indices", "start": 1, "end": 3}),
            )
            .await
            .unwrap();
        assert_eq!(removed, 2);
    }

    #[tokio::test]
    async fn remove_messages_invalid_range_type_errors() {
        let tmp = TempDir::new().unwrap();
        let storage = Arc::new(FileSessionStorage::new());
        let session_manager = Arc::new(SessionManager::with_storage(storage));
        let session = session_manager
            .create_session(
                SessionType::Chat,
                tmp.path().to_path_buf(),
                None,
                vec![],
                None,
            )
            .await
            .unwrap();

        let agent_manager = build_test_agent_manager(session_manager.clone());
        agent_manager
            .configure_agent(&session.id, make_test_agent(None))
            .await
            .unwrap();

        let (event_tx, _) = broadcast::channel(16);
        let bridge = DaemonSessionBridge::new(session_manager, agent_manager, event_tx);

        let err = bridge
            .remove_messages(session.id, serde_json::json!({"type": "bogus"}))
            .await
            .unwrap_err();
        assert!(err.contains("unknown range type"), "got: {err}");
    }
}
