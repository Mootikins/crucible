use super::*;
use chrono::{Duration as ChronoDuration, Utc};

pub(super) fn internal_error(req_id: Option<RequestId>, err: impl std::fmt::Display) -> Response {
    let msg = err.to_string();
    error!("Internal error: {}", msg);
    Response::error(req_id, INTERNAL_ERROR, format!("Internal error: {}", msg))
}

/// Log client error details and return a sanitized error message.
pub(super) fn invalid_state_error(
    req_id: Option<RequestId>,
    operation: &str,
    err: impl std::fmt::Display,
) -> Response {
    debug!("Invalid state for {}: {}", operation, err);
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Operation '{}' not allowed in current state", operation),
    )
}

pub(super) fn session_not_found(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Session not found: {}", session_id),
    )
}

pub(super) fn agent_not_configured(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("No agent configured for session: {}", session_id),
    )
}

pub(super) fn concurrent_request(req_id: Option<RequestId>, session_id: &str) -> Response {
    Response::error(
        req_id,
        INVALID_PARAMS,
        format!("Request already in progress for session: {}", session_id),
    )
}

pub(super) fn agent_error_to_response(req_id: Option<RequestId>, err: AgentError) -> Response {
    match err {
        AgentError::SessionNotFound(id) => session_not_found(req_id, &id),
        AgentError::NoAgentConfigured(id) => agent_not_configured(req_id, &id),
        AgentError::ConcurrentRequest(id) => concurrent_request(req_id, &id),
        e => internal_error(req_id, e),
    }
}

pub(super) async fn handle_client(
    stream: UnixStream,
    ctx: Arc<ServerContext>,
    mut event_rx: broadcast::Receiver<SessionEventMessage>,
) -> Result<()> {
    let client_id = ClientId::new();
    let (reader, writer) = stream.into_split();
    let writer: Arc<Mutex<OwnedWriteHalf>> = Arc::new(Mutex::new(writer));
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let writer_clone = writer.clone();
    let sub_manager = ctx.subscription_manager.clone();
    let event_cancel = CancellationToken::new();
    let event_cancel_clone = event_cancel.clone();
    let event_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = event_cancel_clone.cancelled() => break,
                result = event_rx.recv() => {
                    match result {
                        Ok(event) => {
                            if sub_manager.is_subscribed(client_id, &event.session_id) {
                                if let Ok(json) = event.to_json_line() {
                                    let mut w = writer_clone.lock().await;
                                    if w.write_all(json.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(
                                "Event forwarder lagged, dropped {} events for client {}", n, client_id
                            );
                            continue;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            break;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(req) => handle_request(req, client_id, &ctx).await,
            Err(e) => {
                warn!("Parse error: {}", e);
                Response::error(None, PARSE_ERROR, e.to_string())
            }
        };

        let mut output = serde_json::to_string(&response)?;
        output.push('\n');

        let mut w = writer.lock().await;
        w.write_all(output.as_bytes()).await?;
    }

    // Graceful shutdown of event forwarding
    event_cancel.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_task).await;
    ctx.subscription_manager.remove_client(client_id);

    Ok(())
}

pub(super) fn forward_to_recording(sm: &SessionManager, event: &SessionEventMessage) {
    if let Some(tx) = sm.get_recording_sender(&event.session_id) {
        if tx.try_send(event.clone()).is_err() {
            warn!(
                session_id = %event.session_id,
                "Recording channel full or closed, dropping event"
            );
        }
    }
}

pub(super) fn should_persist(event: &SessionEventMessage) -> bool {
    if event.msg_type != "event" {
        return false;
    }

    matches!(
        event.event.as_str(),
        "user_message"
            | "thinking"
            | "message_complete"
            | "tool_call"
            | "tool_result"
            | "model_switched"
            | "ended"
    )
}

pub(super) async fn persist_event(
    event: &SessionEventMessage,
    sm: &SessionManager,
    storage: &dyn SessionStorage,
) -> Result<()> {
    if !should_persist(event) {
        return Ok(());
    }
    let session = match sm.get_session(&event.session_id) {
        Some(s) => s,
        None => return Ok(()),
    };

    let json = serde_json::to_string(event)?;
    storage
        .append_event(&session, &json)
        .await
        .map_err(|e| anyhow::anyhow!("append_event failed: {}", e))?;

    match event.event.as_str() {
        "user_message" => {
            if let Some(content) = event.data.get("content").and_then(|v| v.as_str()) {
                storage
                    .append_markdown(&session, "User", content)
                    .await
                    .map_err(|e| anyhow::anyhow!("append_markdown(User) failed: {}", e))?;
            }
        }
        "message_complete" => {
            if let Some(content) = event.data.get("full_response").and_then(|v| v.as_str()) {
                storage
                    .append_markdown(&session, "Assistant", content)
                    .await
                    .map_err(|e| anyhow::anyhow!("append_markdown(Assistant) failed: {}", e))?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) async fn sweep_and_archive_stale_sessions(
    session_manager: &SessionManager,
    kiln_manager: &KilnManager,
    subscription_manager: &SubscriptionManager,
    agent_manager: &AgentManager,
    auto_archive_hours: u64,
) -> Result<usize> {
    let now = Utc::now();
    let stale_after = ChronoDuration::hours(auto_archive_hours as i64);
    let mut archived = 0;

    // Storage-aware listing: the in-memory map only holds live sessions,
    // but the stale ones are exactly the persisted, no-longer-loaded ones.
    // Mirror handle_session_list's kiln coverage (open kilns + crucible home).
    let mut kiln_paths: Vec<PathBuf> = kiln_manager
        .list()
        .await
        .into_iter()
        .map(|(path, _, _)| path)
        .collect();
    let home = crucible_core::config::crucible_home();
    if !kiln_paths.contains(&home) {
        kiln_paths.push(home);
    }

    // In-memory sessions first (covers sessions whose kiln isn't open),
    // then persisted sessions from each kiln's storage. Each candidate is
    // paired with the kiln directory to archive under: legacy meta.json
    // files can carry RELATIVE kiln paths ("./docs"), which resolve against
    // the daemon's cwd and miss — so for storage hits, trust the directory
    // we actually scanned, never the file's self-reported kiln.
    let mut candidates: Vec<(_, PathBuf)> = session_manager
        .list_sessions()
        .into_iter()
        .filter(|s| !s.archived)
        .map(|s| {
            let kiln = s.kiln.clone();
            (s, kiln)
        })
        .collect();
    let mut seen_ids: std::collections::HashSet<String> =
        candidates.iter().map(|(s, _)| s.id.clone()).collect();
    for kiln_path in &kiln_paths {
        for summary in session_manager
            .list_sessions_filtered_async(Some(kiln_path), None, None, None, false)
            .await
        {
            if seen_ids.insert(summary.id.clone()) {
                candidates.push((summary, kiln_path.clone()));
            }
        }
    }

    for (summary, archive_kiln) in candidates {
        // Never archive out from under a connected client, regardless of idleness.
        if !subscription_manager.get_subscribers(&summary.id).is_empty() {
            continue;
        }

        let last_activity = summary.last_activity.unwrap_or(summary.started_at);
        if now - last_activity < stale_after {
            continue;
        }

        // Re-check last_activity for in-memory sessions to avoid a TOCTOU race
        // where the session receives new activity between staleness check and archive.
        if let Some(fresh) = session_manager.get_session(&summary.id) {
            let fresh_last_activity = fresh.last_activity.unwrap_or(fresh.started_at);
            if now - fresh_last_activity < stale_after {
                continue;
            }
        }

        // One unreadable meta.json must not wedge the whole sweep.
        match session_manager
            .archive_session(&summary.id, &archive_kiln)
            .await
        {
            Ok(_) => {
                // Mirror the RPC end/delete/archive handlers: free the archived
                // session's agent state (cache, Lua, dispatchers, trees,
                // snapshots, pending requests). The sweep is SessionManager-only,
                // so without this the agent state orphaned for the daemon's life.
                agent_manager.cleanup_session(&summary.id);
                archived += 1;
            }
            Err(e) => warn!(
                session_id = %summary.id,
                error = %e,
                "Auto-archive sweep: failed to archive session"
            ),
        }
    }

    Ok(archived)
}

pub(super) async fn handle_request(
    req: Request,
    client_id: ClientId,
    ctx: &ServerContext,
) -> Response {
    ctx.dispatcher.dispatch(client_id, req).await
}
