//! Daemon-side implementation of [`DaemonSessionApi`] for Lua plugins.
//!
//! Bridges `cru.sessions.*` Lua calls to the daemon's `SessionManager`,
//! `AgentManager`, and event broadcast infrastructure.

use crate::agent_manager::AgentManager;
use crate::protocol::SessionEventMessage;
use crate::rpc::RpcContext;
use crate::session_manager::SessionManager;
use crate::session_storage::{FileSessionStorage, SessionStorage};
use crucible_core::session::{CommentAuthor, HunkId, LineRange, ReviewState};
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
    /// The dispatcher's own context, so `create` runs the daemon's real create
    /// path rather than a second, thinner one that skips scope refusal, trust
    /// validation, agent resolution and the setup task.
    ctx: Arc<RpcContext>,
    session_manager: Arc<SessionManager>,
    agent_manager: Arc<AgentManager>,
    event_tx: broadcast::Sender<SessionEventMessage>,
}

impl DaemonSessionBridge {
    /// The three manager handles are taken off the context rather than passed
    /// beside it: they are the same `Arc`s, and two sources for one manager is
    /// how a bridge ends up talking to a different `SessionManager` than the
    /// create path it delegates to.
    pub fn new(ctx: Arc<RpcContext>) -> Self {
        Self {
            session_manager: ctx.sessions.clone(),
            agent_manager: ctx.agents.clone(),
            event_tx: ctx.event_tx.clone(),
            ctx,
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
    /// A plugin create is the same create an RPC client gets.
    ///
    /// The params object is deserialized with the same request type
    /// `handle_session_create` uses, so the two callers cannot drift on a field
    /// name, and `create_session_resolved` then does the whole job: scope
    /// refusal, trust validation against the kiln's classification, agent
    /// resolution (including agent cards), project registration, kiln open,
    /// recording and the setup task. The one step it does not do is
    /// `enforce_session_start` — see that method's doc for why a plugin-side
    /// create must not reach it yet.
    ///
    /// An omitted `kilns` stays omitted rather than being resolved to
    /// `crucible_home()` here: the fallback belongs to the daemon's own data
    /// root, and a path this bridge invented would be scope-checked as if the
    /// caller had asked for it — which fails whenever the data root is `$HOME`.
    fn create_session(&self, params: serde_json::Value) -> BoxFut<serde_json::Value> {
        bridge_async!(self.ctx, |ctx| async move {
            let request: crate::rpc_client::SessionCreateRequest =
                serde_json::from_value(params)
                    .map_err(|e| format!("Invalid create options: {e}"))?;
            let session = ctx
                .create_session_resolved(&request)
                .await
                .map_err(|e| e.to_string())?;
            Ok(serde_json::json!({
                "id": session.id,
                "session_type": session.session_type.as_prefix(),
                "kilns": session.kilns,
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
                    "kilns": s.kilns,
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
                        "kilns": s.kilns,
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

    /// Plugin turns are non-interactive.
    ///
    /// There is no Crucible principal behind a plugin turn — a Discord message
    /// carries a chat-room username, and permissions are keyed on
    /// `(session_id, permission_id)` alone, so whoever answers first answers
    /// for everyone. Passing `false` makes the permission engine convert `Ask`
    /// to `Deny` (`PermissionEngine::evaluate`) and the gate return a tool
    /// error before any prompt is emitted. A plugin that wants to drive
    /// permissions itself can still subscribe and use
    /// `cru.sessions.interaction_respond`.
    fn send_message(&self, session_id: String, content: String) -> BoxFut<String> {
        bridge_async!(
            self.agent_manager,
            self.event_tx,
            |am, event_tx| async move {
                am.send_message(&session_id, content, &event_tx, false, None)
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

    fn request_interaction(
        &self,
        session_id: String,
        request: serde_json::Value,
        timeout_secs: u64,
    ) -> BoxFut<serde_json::Value> {
        bridge_async!(
            self.agent_manager,
            self.event_tx,
            |am, event_tx| async move {
                // Decoded here rather than passed through as JSON so a
                // malformed request fails at the plugin's call site with the
                // serde path in the message, instead of reaching a client that
                // cannot render it and timing out 300 s later.
                let request: crucible_core::interaction::InteractionRequest =
                    serde_json::from_value(request)
                        .map_err(|e| format!("Invalid interaction request: {e}"))?;
                let response = am
                    .request_interaction(
                        &session_id,
                        request,
                        &event_tx,
                        std::time::Duration::from_secs(timeout_secs),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                serde_json::to_value(response)
                    .map_err(|e| format!("Could not serialize interaction response: {e}"))
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
            let session_dir = sm.session_dir(&session.id);
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
                    parent.kilns.clone(),
                    parent.workspace.clone(),
                    None,
                )
                .await
                .map_err(|e| e.to_string())?;

            let parent_dir = sm.session_dir(&parent.id);
            let events = crate::observe::load_events(&parent_dir)
                .await
                .unwrap_or_default();

            let storage = FileSessionStorage::new(sm.sessions_root().to_path_buf())
                .with_registry(sm.kiln_registry().clone());
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
            let results = am.collect_jobs(&job_ids, timeout).await;
            Ok(results)
        })
    }

    fn send_and_collect(
        &self,
        session_id: String,
        content: String,
        timeout_secs: Option<f64>,
        max_tool_result_len: Option<usize>,
        interactive: bool,
    ) -> BoxFut<tokio::sync::mpsc::UnboundedReceiver<ResponsePart>> {
        let am = self.agent_manager.clone();
        let event_tx = self.event_tx.clone();
        Box::pin(async move {
            let timeout = std::time::Duration::from_secs_f64(timeout_secs.unwrap_or(120.0));
            let max_result = max_tool_result_len.unwrap_or(500);

            // Subscribe to broadcast BEFORE sending so we don't miss early events
            let mut broadcast_rx = event_tx.subscribe();

            // `false` unless the caller asserted a single identified principal.
            // A plugin turn has no Crucible principal behind it by default:
            // permissions are keyed on `(session_id, permission_id)` alone, so
            // in any channel with more than one person the first reply answers
            // for everyone. A plugin may opt in where it knows the channel is
            // one named account — a DM — and only there.
            let _msg_id = am
                .send_message(&session_id, content, &event_tx, interactive, None)
                .await
                .map_err(|e| e.to_string())?;

            let (part_tx, part_rx) = tokio::sync::mpsc::unbounded_channel();

            tokio::spawn(async move {
                let mut text_buf = String::new();
                let deadline = tokio::time::Instant::now() + timeout;

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
            am.get_context_usage(&session_id)
                .await
                .map_err(|e| e.to_string())
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

    fn remove_messages(&self, session_id: String, range: serde_json::Value) -> BoxFut<usize> {
        bridge_async!(self.agent_manager, |am| async move {
            let parsed = parse_range(&range)?;
            am.remove_messages(&session_id, parsed)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn set_output_validation(&self, session_id: String, spec: String) -> BoxFut<()> {
        let agent_manager = Arc::clone(&self.agent_manager);
        let event_tx = self.event_tx.clone();
        Box::pin(async move {
            let parsed: crucible_core::session::OutputValidation = spec.parse()?;
            agent_manager
                .set_output_validation(&session_id, parsed, Some(&event_tx))
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn undo(&self, session_id: String, count: usize) -> BoxFut<usize> {
        let agent_manager = Arc::clone(&self.agent_manager);
        let event_tx = self.event_tx.clone();
        Box::pin(async move {
            let summaries = agent_manager
                .undo(&session_id, count, Some(&event_tx))
                .await
                .map_err(|e| e.to_string())?;
            Ok(summaries.len())
        })
    }

    fn can_undo(&self, session_id: String) -> BoxFut<bool> {
        bridge_async!(self.agent_manager, |am| async move {
            am.can_undo(&session_id).await.map_err(|e| e.to_string())
        })
    }

    fn undo_depth(&self, session_id: String) -> BoxFut<usize> {
        bridge_async!(self.agent_manager, |am| async move {
            am.undo_depth(&session_id).await.map_err(|e| e.to_string())
        })
    }

    fn undo_history(&self, session_id: String) -> BoxFut<Vec<serde_json::Value>> {
        bridge_async!(self.agent_manager, |am| async move {
            let summaries = am
                .undo_history(&session_id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(summaries
                .into_iter()
                .enumerate()
                .map(|(idx, s)| {
                    let mut v = serde_json::to_value(&s).unwrap_or(serde_json::Value::Null);
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("turn_index".to_string(), serde_json::json!(idx));
                    }
                    v
                })
                .collect())
        })
    }

    // The review methods delegate to the same free functions the `review.*`
    // RPC handlers call, so a plugin tool and the web panel cannot drift on
    // what "reject" does — in particular, both revert on disk and both tell
    // the reviewed session's agent.
    //
    // Every one of them opens with `ensure_loaded`, exactly as every handler
    // does, and for the reason `ensure_loaded` exists: a review call can be the
    // *first* thing that touches a session after a daemon restart. Without it a
    // delegating agent asking for a resumed session's hunks is answered `[]`
    // with no error — "the child changed nothing" — while a browser hitting the
    // same session through the REST route gets the queue restored from
    // `review.jsonl`. That is precisely the drift these shared functions exist
    // to prevent.

    fn review_list_hunks(&self, session_id: String) -> BoxFut<Vec<serde_json::Value>> {
        bridge_async!(
            self.agent_manager,
            self.session_manager,
            |am, sm| async move {
                crate::server::session::review::ensure_loaded(&am, &sm, &session_id).await;
                let hunks = crate::server::session::review::list_hunks(&am, &session_id)
                    .await
                    .map_err(|e| e.to_string())?;
                hunks
                    .iter()
                    .map(|h| serde_json::to_value(h).map_err(|e| e.to_string()))
                    .collect()
            }
        )
    }

    fn review_set_state(&self, session_id: String, hunk_id: String, state: String) -> BoxFut<()> {
        let agent_manager = Arc::clone(&self.agent_manager);
        let session_manager = Arc::clone(&self.session_manager);
        let event_tx = self.event_tx.clone();
        Box::pin(async move {
            let state: ReviewState = serde_json::from_value(serde_json::Value::String(state))
                .map_err(|_| "state must be one of: unreviewed, accepted, rejected".to_string())?;
            crate::server::session::review::ensure_loaded(
                &agent_manager,
                &session_manager,
                &session_id,
            )
            .await;
            crate::server::session::review::set_state(
                &agent_manager,
                &session_manager,
                &event_tx,
                &session_id,
                &HunkId::from(hunk_id),
                state,
            )
            .await
            .map_err(|e| e.to_string())
        })
    }

    fn review_comment(
        &self,
        session_id: String,
        spec: serde_json::Value,
    ) -> BoxFut<serde_json::Value> {
        let agent_manager = Arc::clone(&self.agent_manager);
        let session_manager = Arc::clone(&self.session_manager);
        let event_tx = self.event_tx.clone();
        Box::pin(async move {
            let spec = CommentSpec::parse(&spec)?;
            crate::server::session::review::ensure_loaded(
                &agent_manager,
                &session_manager,
                &session_id,
            )
            .await;
            let comment = crate::server::session::review::add_comment(
                &agent_manager,
                &event_tx,
                &session_id,
                spec.root.as_deref(),
                &spec.path,
                spec.line_range,
                &spec.body,
                spec.author,
            )
            .await
            .map_err(|e| e.to_string())?;
            serde_json::to_value(comment).map_err(|e| e.to_string())
        })
    }

    fn review_resolve_comment(&self, session_id: String, comment_id: String) -> BoxFut<()> {
        let agent_manager = Arc::clone(&self.agent_manager);
        let session_manager = Arc::clone(&self.session_manager);
        let event_tx = self.event_tx.clone();
        Box::pin(async move {
            crate::server::session::review::ensure_loaded(
                &agent_manager,
                &session_manager,
                &session_id,
            )
            .await;
            crate::server::session::review::resolve_comment(
                &agent_manager,
                &event_tx,
                &session_id,
                &comment_id,
            )
            .await
            .map_err(|e| e.to_string())
        })
    }
}

/// A `cru.sessions.review_comment` spec, validated once so the Lua binding and
/// the RPC handler reject the same inputs for the same reasons.
struct CommentSpec {
    root: Option<PathBuf>,
    path: PathBuf,
    line_range: LineRange,
    body: String,
    author: CommentAuthor,
}

impl CommentSpec {
    fn parse(spec: &serde_json::Value) -> Result<Self, String> {
        let obj = spec
            .as_object()
            .ok_or_else(|| "comment spec must be a table".to_string())?;
        let path = obj
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "comment spec requires 'path'".to_string())?;
        let body = obj
            .get("body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "comment spec requires 'body'".to_string())?;
        let line_start = obj
            .get("line_start")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "comment spec requires 'line_start'".to_string())?
            as u32;
        // Half-open: a spec naming only a start line means that one line.
        let line_end = obj
            .get("line_end")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(line_start + 1);
        // An agent commenting through a plugin tool is the §6 case, so
        // `author` defaults to `agent` here — the RPC handler, whose caller is
        // a human at a panel, defaults the other way.
        let author = match obj
            .get("author")
            .and_then(|v| v.as_str())
            .unwrap_or("agent")
        {
            "agent" => CommentAuthor::Agent,
            "human" => CommentAuthor::Human,
            other => return Err(format!("unknown comment author: {other}")),
        };

        Ok(Self {
            root: obj.get("root").and_then(|v| v.as_str()).map(PathBuf::from),
            path: PathBuf::from(path),
            line_range: LineRange::new(line_start, line_end),
            body: body.to_string(),
            author,
        })
    }
}

/// Decode a JSON range descriptor into a [`Range`] value.
///
/// Accepted shapes:
/// * `{ "type": "all" }`
/// * `{ "type": "last" | "first", "n": N }`
/// * `{ "type": "indices", "start": S, "end": E }` (half-open `[S, E)`)
fn parse_range(v: &serde_json::Value) -> Result<crucible_core::traits::context_ops::Range, String> {
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
mod tests;
