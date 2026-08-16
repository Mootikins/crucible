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

/// The uid this daemon runs as — the only uid allowed to speak JSON-RPC to it.
pub(super) fn daemon_uid() -> u32 {
    crucible_core::protocol::lifecycle::current_uid()
}

/// Reject a connection from any other local user before it can send a request.
///
/// Every RPC method is unauthenticated once the connection is up — they read the
/// kiln, run Lua, spawn agents. So the connection itself is the authentication
/// boundary, and the only credential that grants it is being the same user. Root
/// is refused too: root does not need our door, and blessing it would silently
/// admit any process that gained it.
///
/// Fails closed: if `SO_PEERCRED` cannot be read there is no credential to
/// check, and an unidentifiable peer is exactly what an attacker looks like.
///
/// `authorized_uid` is a parameter rather than `geteuid()` read inline so a test
/// can bind a real server that expects a uid other than its own and watch a
/// genuine connection get dropped — otherwise nothing in the suite would notice
/// this check being deleted.
#[cfg(unix)]
pub(super) fn peer_accepted(stream: &UnixStream, authorized_uid: u32) -> bool {
    match stream.peer_cred() {
        Ok(cred) if cred.uid() == authorized_uid => true,
        Ok(cred) => {
            warn!(
                peer_uid = cred.uid(),
                authorized_uid, "refused a client connection from another user"
            );
            false
        }
        Err(e) => {
            warn!(error = %e, "refused a client connection with unreadable peer credentials");
            false
        }
    }
}

/// How long a write to a client may block before the connection is considered
/// dead.
///
/// A client that has stopped draining fills the socket buffer, and `write_all`
/// then blocks forever: the writer mutex is held, the other writer blocks on it,
/// and `handle_client` never reaches its cleanup — so the connection leaks a
/// task and a subscription entry for the daemon's lifetime.
///
/// Timing out *mid*-`write_all` leaves a partial line on the wire, so the only
/// correct response is to close the connection. Logging and continuing would
/// hand the client truncated JSON, which is worse than no JSON.
const CLIENT_WRITE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Write one framed line to a client, or report that the connection is gone.
///
/// `Err` means "stop using this connection" for either reason — a real I/O
/// error, or a peer that has not accepted 30s of writes. Callers must not
/// distinguish them: both leave the stream unusable.
pub(super) async fn write_line_or_close(
    writer: &Mutex<OwnedWriteHalf>,
    bytes: &[u8],
    client_id: ClientId,
) -> Result<()> {
    let mut w = writer.lock().await;
    match tokio::time::timeout(CLIENT_WRITE_TIMEOUT, w.write_all(bytes)).await {
        Ok(result) => Ok(result?),
        Err(_elapsed) => {
            warn!(
                %client_id,
                timeout_secs = CLIENT_WRITE_TIMEOUT.as_secs(),
                "client stopped accepting writes; closing the connection"
            );
            Err(anyhow::anyhow!("client write timed out"))
        }
    }
}

/// Tell one client that its event stream has a hole in it.
///
/// Written straight to the socket rather than published on the broadcast: the
/// broadcast is what lagged, and a gap is a fact about this connection's cursor,
/// not about any session — publishing it would hand every *other* client a
/// marker for a gap it did not have.
///
/// Addressed to the wildcard because `Lagged(n)` reports a count and nothing
/// else: the daemon does not know which sessions the dropped events belonged to,
/// so naming one would be a guess. `n` is per connection, not per session.
///
/// Deliberately carries **no `seq`**. Every other event is stamped from its
/// session's counter (`event_emitter::stamp_event`) so a client can check
/// contiguity by arithmetic; this one is minted outside that sequence space, and
/// giving it a number from the `"*"` counter would insert a value into a stream
/// other clients never saw. Clients exempt `stream_gap` from the contiguity
/// check — it is the thing that *explains* a discontinuity.
fn stream_gap_event(dropped: u64) -> SessionEventMessage {
    SessionEventMessage::typed(
        crate::subscription::WILDCARD_SESSION,
        crucible_core::protocol::session_events::SystemPayload::StreamGap { dropped },
    )
}

/// Forward broadcast events to one client's socket until cancelled or wedged.
///
/// Extracted from `handle_client` so the `Lagged` path is reachable from a test:
/// overflowing a `broadcast::Receiver` is deterministic (send past capacity
/// before the first `recv`), but only if something other than a live daemon
/// connection can be handed the receiver.
async fn forward_events(
    mut event_rx: broadcast::Receiver<SessionEventMessage>,
    writer: Arc<Mutex<OwnedWriteHalf>>,
    sub_manager: Arc<SubscriptionManager>,
    client_id: ClientId,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            result = event_rx.recv() => {
                match result {
                    Ok(event) => {
                        // The wildcard is symmetric: a client subscribed to
                        // "*" receives everything, and an event addressed
                        // to "*" reaches everyone. Without the second half,
                        // a genuinely global event (a theme change) would
                        // silently reach nobody, because every client
                        // subscribes to its own session id.
                        let broadcast_to_all =
                            event.session_id == crate::subscription::WILDCARD_SESSION;
                        if broadcast_to_all
                            || sub_manager.is_subscribed(client_id, &event.session_id)
                        {
                            if let Ok(json) = event.to_json_line() {
                                // Breaks on a write timeout as well as an
                                // I/O error: a peer that has stopped
                                // draining would otherwise hold the writer
                                // mutex forever and wedge the request loop
                                // behind it.
                                if write_line_or_close(&writer, json.as_bytes(), client_id)
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "Event forwarder lagged, dropped {} events for client {}", n, client_id
                        );
                        // Tell the client. A hole it cannot see is worse than a
                        // hole it can: with the marker it can refetch and it
                        // stops trusting a transcript that is now wrong;
                        // without it the rendered conversation is quietly and
                        // permanently missing N events.
                        if let Ok(json) = stream_gap_event(n).to_json_line() {
                            if write_line_or_close(&writer, json.as_bytes(), client_id)
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

/// How many requests one connection may have in flight at once.
///
/// Bounded, not unbounded: a client that pipelines faster than the daemon can
/// serve should queue in its socket, not create tasks without limit. Spawning
/// per line with no bound trades head-of-line blocking for unbounded resource
/// growth, which is a worse deal.
///
/// 32 is well above any real client's concurrency — the TUI and the web server
/// each have a handful of calls outstanding at their busiest — and far below
/// anything that matters for memory.
const MAX_INFLIGHT_PER_CONNECTION: usize = 32;

/// Read requests from one connection, serve them concurrently, and write each
/// reply as it finishes.
///
/// The serial `read_line` → `handle_request().await` → `write_all` this replaces
/// meant one long **synchronous** handler delayed that client's every other
/// call: `kiln.open` with `process = true`, `process_batch`, `session.reindex`.
/// (Not `session.send_message` — it spawns the turn and returns, so cancel was
/// never queued behind a running turn, contrary to how this is usually
/// described.)
///
/// Three things make the spawn safe, and it would not be without all three:
/// - `handle_request` already contains each request's panic, so a spawned
///   handler cannot unwind the connection.
/// - Replies are no longer FIFO, so every client must correlate by id.
///   `DaemonClient` now does in both modes, and so does the integration-test
///   `RpcConn`; a client that does not will mis-deliver silently.
/// - The semaphore bounds task growth.
///
/// `serve` is a parameter rather than a direct `handle_request` call because the
/// property this function exists for — a parked request not blocking a later one
/// — is unobservable through a dispatcher whose handlers all return promptly. It
/// also puts the connection's framing and concurrency on one side of a line and
/// dispatch on the other.
///
/// `shutdown` is the one thing framing owes the daemon back: the `shutdown`
/// handler arms it and this loop fires it after the confirmation is written, so
/// the process teardown that follows cannot outrun the reply. See
/// [`DeferredShutdown`].
async fn serve_requests<F, Fut>(
    reader: tokio::net::unix::OwnedReadHalf,
    writer: Arc<Mutex<OwnedWriteHalf>>,
    client_id: ClientId,
    max_inflight: usize,
    shutdown: Arc<DeferredShutdown>,
    serve: F,
) -> Result<()>
where
    F: Fn(String) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Response> + Send + 'static,
{
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let inflight = Arc::new(tokio::sync::Semaphore::new(max_inflight));
    let mut handlers = tokio::task::JoinSet::new();

    // A failed write now happens inside a handler, not in this loop. Without
    // this token the connection would go on accepting requests whose replies can
    // never be delivered — and never reach the caller's cleanup, which is the
    // task-plus-subscription leak the write timeout exists to close.
    let closed = CancellationToken::new();

    let outcome = loop {
        line.clear();
        // `read_line` is not cancel-safe: losing the select drops a partly-read
        // line. That is fine here and only here, because the only branch that can
        // beat it is the one that abandons the connection outright — a request
        // whose reply can never be written is not one to finish reading.
        let read = tokio::select! {
            biased;
            _ = closed.cancelled() => break Err(anyhow::anyhow!("client write failed; closing")),
            r = reader.read_line(&mut line) => r,
        };
        match read {
            Ok(0) => break Ok(()),
            Ok(_) => {}
            Err(e) => break Err(e.into()),
        }

        // Acquired before the spawn, so a client that pipelines past the limit
        // waits here and its extra requests queue in the socket buffer rather
        // than as tasks.
        let permit = tokio::select! {
            biased;
            _ = closed.cancelled() => break Err(anyhow::anyhow!("client write failed; closing")),
            p = inflight.clone().acquire_owned() => match p {
                Ok(p) => p,
                // Nothing closes this semaphore; it lives and dies with the loop.
                Err(_) => break Err(anyhow::anyhow!("connection semaphore closed")),
            },
        };

        let owned = std::mem::take(&mut line);
        let serve = serve.clone();
        let writer = writer.clone();
        let closed_for_handler = closed.clone();
        let shutdown = shutdown.clone();
        handlers.spawn(async move {
            let _permit = permit;
            let response = serve(owned).await;
            let mut output = match serde_json::to_string(&response) {
                Ok(o) => o,
                Err(e) => {
                    error!(error = %e, "failed to serialize a response; dropping it");
                    return;
                }
            };
            output.push('\n');
            let written = write_line_or_close(&writer, output.as_bytes(), client_id).await;
            if written.is_err() {
                closed_for_handler.cancel();
            }
            // After the write *attempt*, never before it: the daemon can be gone
            // a scheduling slice later. A client that asked to shut down and then
            // stopped reading still gets its shutdown — only the ordering changes.
            shutdown.fire_if_armed();
        });
    };

    // Let in-flight handlers finish before the caller tears the connection down.
    // Dropping the `JoinSet` would abort them mid-handler, and a handler is
    // whatever the RPC does — a storage write, a git operation. The serial loop
    // this replaces always ran its one request to completion, so waiting keeps
    // that guarantee instead of trading it for a faster close.
    while let Some(joined) = handlers.join_next().await {
        if let Err(e) = joined {
            error!(error = %e, "request handler task failed");
        }
    }

    outcome
}

pub(super) async fn handle_client(
    stream: UnixStream,
    ctx: Arc<ServerContext>,
    event_rx: broadcast::Receiver<SessionEventMessage>,
) -> Result<()> {
    #[cfg(unix)]
    if !peer_accepted(&stream, ctx.authorized_uid) {
        return Ok(());
    }

    let client_id = ClientId::new();
    let (reader, writer) = stream.into_split();
    let writer: Arc<Mutex<OwnedWriteHalf>> = Arc::new(Mutex::new(writer));

    let event_cancel = CancellationToken::new();
    let event_task = tokio::spawn(forward_events(
        event_rx,
        writer.clone(),
        ctx.subscription_manager.clone(),
        client_id,
        event_cancel.clone(),
    ));

    let dispatch_ctx = ctx.clone();
    let result = serve_requests(
        reader,
        writer,
        client_id,
        MAX_INFLIGHT_PER_CONNECTION,
        ctx.shutdown.clone(),
        move |line: String| {
            let ctx = dispatch_ctx.clone();
            async move {
                match serde_json::from_str::<Request>(&line) {
                    Ok(req) => handle_request(req, client_id, &ctx).await,
                    Err(e) => {
                        warn!("Parse error: {}", e);
                        Response::error(None, PARSE_ERROR, e.to_string())
                    }
                }
            }
        },
    )
    .await;

    // Graceful shutdown of event forwarding
    event_cancel.cancel();
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), event_task).await;
    ctx.subscription_manager.remove_client(client_id);

    result
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

/// Does this event belong in `session.jsonl`?
///
/// The decision now lives on the typed payload
/// ([`SessionEventPayload::is_persisted`]), which matches exhaustively over each
/// group. This was a ninth hand-maintained vocabulary: adding a turn event
/// persisted nothing and nobody was told, which is how `segment_complete` was
/// nearly shipped unpersisted.
///
/// An event this build cannot decode is not persisted — the same answer the name
/// list gave for a name it did not list.
pub(super) fn should_persist(event: &SessionEventMessage) -> bool {
    if event.msg_type != "event" {
        return false;
    }
    event.payload().is_ok_and(|p| p.is_persisted())
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
    subscription_manager: &SubscriptionManager,
    agent_manager: &AgentManager,
    auto_archive_hours: u64,
) -> Result<usize> {
    let now = Utc::now();
    let stale_after = ChronoDuration::hours(auto_archive_hours as i64);
    let mut archived = 0;

    // Storage-aware listing: the in-memory map only holds live sessions, but
    // the stale ones are exactly the persisted, no-longer-loaded ones — and
    // `list_sessions_filtered_async` covers both from the one sessions root.
    let candidates = session_manager
        .list_sessions_filtered_async(KilnFilter::Any, None, None, None, false)
        .await;

    for summary in candidates {
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
        match session_manager.archive_session(&summary.id).await {
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

/// Release review keep refs whose sessions are gone.
///
/// Rides the archive sweep rather than getting its own timer: both walk the
/// same sessions root and both are cheap, and a second half-hourly task
/// spawning `git` subprocesses in every tracked repository is not worth the
/// tick.
///
/// Only the *orphans* go. A keep ref whose session still has a journal is
/// still protecting trees a live review depends on, however old the session
/// is — expiring on age would delete the base tree out from under a queue
/// somebody is halfway through.
pub(super) async fn sweep_review_refs(sessions_root: &Path) -> usize {
    // Every session in one scan. This used to have to enumerate registered
    // kilns as well as open ones, because a kiln nobody had opened this run was
    // invisible and its live journals would read as orphans — a shared
    // repository would then lose the base tree of a queue somebody was halfway
    // through. With one sessions root there is no such partial view.
    crate::review::sweep_review_refs(sessions_root).await
}

/// Dispatch one request, converting a panic into an error response.
///
/// A panicking handler used to unwind the connection task, so the client saw
/// its socket close mid-request with no explanation — `cru status` reported
/// "Connection closed by daemon" when a note containing an em dash panicked the
/// markdown parser. One malformed note took down the whole conversation.
///
/// The daemon serves many clients and holds session state, so the blast radius
/// of a panic must be one request. This does not make panics acceptable — they
/// are still bugs, and the payload is logged at error level with the request id
/// so they surface — it makes them survivable.
///
/// `AssertUnwindSafe` is the honest choice rather than a rubber stamp: the
/// dispatcher's state lives behind locks and channels that are already shared
/// across concurrent requests, so a half-finished handler leaves nothing
/// observably torn that a concurrent request could not already see. State that
/// genuinely cannot tolerate interruption belongs behind a transaction, not
/// behind unwind safety.
pub(super) async fn handle_request(
    req: Request,
    client_id: ClientId,
    ctx: &ServerContext,
) -> Response {
    use futures::FutureExt;

    let id = req.id.clone();
    let method = req.method.clone();

    match std::panic::AssertUnwindSafe(ctx.dispatcher.dispatch(client_id, req))
        .catch_unwind()
        .await
    {
        Ok(response) => response,
        Err(panic) => {
            let detail = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");

            error!(
                method = %method,
                panic = detail,
                "handler panicked; returning an error instead of dropping the connection"
            );

            Response::error(
                id,
                INTERNAL_ERROR,
                format!("internal error handling '{method}': {detail}"),
            )
        }
    }
}

#[cfg(test)]
mod tests;
