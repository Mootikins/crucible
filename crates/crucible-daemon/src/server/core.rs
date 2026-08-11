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

pub(super) async fn handle_client(
    stream: UnixStream,
    ctx: Arc<ServerContext>,
    mut event_rx: broadcast::Receiver<SessionEventMessage>,
) -> Result<()> {
    #[cfg(unix)]
    if !peer_accepted(&stream, ctx.authorized_uid) {
        return Ok(());
    }

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
                                    if write_line_or_close(
                                        &writer_clone,
                                        json.as_bytes(),
                                        client_id,
                                    )
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

        // `?` on purpose: a timed-out write must fall out of this loop so the
        // cleanup below runs. Returning early is what releases the event
        // subscription and cancels the forwarder task — the leak this closes.
        write_line_or_close(&writer, output.as_bytes(), client_id).await?;
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
            | "segment_complete"
            | "message_complete"
            | "tool_call"
            | "tool_result"
            | "model_switched"
            | "ended"
            // What context was injected is part of the turn's record, not just
            // a live notification: without it a resumed transcript cannot say
            // which notes the answer was grounded in, and re-deriving it later
            // would report today's search results as though they were the
            // ones actually used.
            | "precognition_complete"
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
    data_home: &std::path::Path,
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
    let home = data_home.to_path_buf();
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

/// Release review keep refs whose sessions are gone.
///
/// Rides the archive sweep rather than getting its own timer: both walk the
/// same kiln list and both are cheap, and a second half-hourly task spawning
/// `git` subprocesses in every tracked repository is not worth the tick.
///
/// Only the *orphans* go. A keep ref whose session still has a journal is
/// still protecting trees a live review depends on, however old the session
/// is — expiring on age would delete the base tree out from under a queue
/// somebody is halfway through.
pub(super) async fn sweep_review_refs(kiln_manager: &KilnManager, data_home: &Path) -> usize {
    // Every *registered* kiln, not just the open ones. `KilnManager::list`
    // returns live connections, so a kiln nobody has opened this run is
    // invisible — and a repository shared between an open kiln's session and a
    // closed kiln's session would report the closed one's ref as an orphan and
    // delete it, taking the base tree of a queue somebody is halfway through.
    // A kiln we cannot read is a kiln whose sessions we cannot vouch for, so
    // being wrong in this direction only costs an unreleased ref.
    let mut kilns: Vec<PathBuf> = crucible_core::config::CliAppConfig::load(None, None, None)
        .map(|config| {
            config
                .resolved_kilns()
                .into_values()
                .filter_map(|entry| match entry {
                    crucible_core::config::KilnEntry::Path(path) => Some(path),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    for (path, _, _) in kiln_manager.list().await {
        if !kilns.contains(&path) {
            kilns.push(path);
        }
    }
    let home = data_home.to_path_buf();
    if !kilns.contains(&home) {
        kilns.push(home);
    }
    crate::review::sweep_review_refs(&kilns).await
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
mod panic_boundary_tests {
    use futures::FutureExt;

    /// The property `handle_request` provides: a panicking handler becomes an
    /// error response, not a dropped connection.
    ///
    /// This exercises the same `AssertUnwindSafe` + `catch_unwind` composition
    /// the real path uses. Driving `handle_request` itself would need a whole
    /// live `ServerContext`; what can actually regress here is the catch
    /// composition, since the panic escapes the moment it is removed.
    #[tokio::test]
    async fn a_panicking_future_is_caught_rather_than_unwinding() {
        let caught = std::panic::AssertUnwindSafe(async {
            // The shape of the real one: a byte index inside a codepoint.
            let s = "an em dash — here";
            let _ = &s[..12];
            "unreachable"
        })
        .catch_unwind()
        .await;

        let panic = caught.expect_err("slicing mid-codepoint must panic");
        let detail = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("unknown panic");

        assert!(
            detail.contains("char boundary"),
            "the panic payload is recoverable for the error message: {detail:?}"
        );
    }

    /// A handler that does not panic must be entirely unaffected.
    #[tokio::test]
    async fn a_normal_future_passes_through_untouched() {
        let result = std::panic::AssertUnwindSafe(async { 42 })
            .catch_unwind()
            .await;
        assert_eq!(result.ok(), Some(42));
    }
}

#[cfg(test)]
mod client_write_timeout_tests {
    use super::*;

    /// A peer that stops draining must cost the daemon one closed connection,
    /// not a permanently wedged writer.
    ///
    /// Before the timeout, `write_all` on a full socket buffer blocked forever
    /// with the writer mutex held: the event forwarder and the request loop both
    /// stalled, `handle_client` never reached its cleanup, and the connection
    /// leaked a task plus a subscription entry for the daemon's lifetime.
    ///
    /// Deterministic and instant: `tokio::time::pause()` means the runtime
    /// auto-advances the clock once every task is blocked, so the timeout fires
    /// the moment `write_all` genuinely cannot make progress. No sleeps, and no
    /// 30s of wall time.
    #[tokio::test(start_paused = true)]
    async fn a_client_that_stops_reading_gets_its_connection_closed() {
        let (server_side, _client_side) = tokio::net::UnixStream::pair().expect("socketpair");
        let (_reader, writer) = server_side.into_split();
        let writer = Mutex::new(writer);

        // `_client_side` is never read from, so this fills the socket buffers
        // and then blocks — which is precisely the wedge being closed. Well
        // past any plausible SO_SNDBUF + SO_RCVBUF.
        let payload = vec![b'x'; 16 * 1024 * 1024];

        let result = write_line_or_close(&writer, &payload, ClientId::new()).await;

        assert!(
            result.is_err(),
            "a write the peer never accepts must fail so the caller closes the connection"
        );
    }

    /// The ordinary case is untouched: a peer that reads gets its bytes and the
    /// connection stays up.
    #[tokio::test(start_paused = true)]
    async fn a_client_that_reads_is_written_to_normally() {
        use tokio::io::AsyncReadExt;

        let (server_side, mut client_side) = tokio::net::UnixStream::pair().expect("socketpair");
        let (_reader, writer) = server_side.into_split();
        let writer = Mutex::new(writer);

        let reading = tokio::spawn(async move {
            let mut buf = [0_u8; 5];
            client_side.read_exact(&mut buf).await.expect("read");
            buf
        });

        write_line_or_close(&writer, b"hello", ClientId::new())
            .await
            .expect("a draining peer must be written to without error");

        assert_eq!(&reading.await.expect("reader task"), b"hello");
    }
}
