use super::super::*;
use crate::rpc_helpers::{session_id_field, typed_params};
use crate::session_lifecycle::{SessionLifecycle, StopCause, StopError, Stopped};
use crate::SessionError;
use crucible_core::protocol::requests::SessionReplayRequest;
use crucible_core::protocol::requests::{
    SessionHistoryRequest, SessionIdRequest, SessionResumeFromStorageRequest,
};
use crucible_core::session::{SessionId, SessionState};

pub(crate) async fn handle_session_pause(req: Request, lifecycle: &SessionLifecycle) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    match lifecycle.stop(session_id, StopCause::Pause).await {
        Ok(Stopped::Paused { previous }) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "previous_state": format!("{}", previous),
                "state": "paused",
            }),
        ),
        Ok(other) => unexpected_stop(req.id, "pause", other),
        Err(e) => stop_error(req.id, "pause", e),
    }
}

/// The answer to a stop that returned a result of another cause. The owner
/// maps each cause to one result, so this is a daemon bug, not a user error.
fn unexpected_stop(req_id: Option<RequestId>, operation: &str, stopped: Stopped) -> Response {
    internal_error(
        req_id,
        format!("session {operation} returned {stopped:?}, which is a result of another stop"),
    )
}

/// The answer to a refused stop.
fn stop_error(req_id: Option<RequestId>, operation: &str, err: StopError) -> Response {
    match err {
        StopError::TurnRunning(_) => Response::error(req_id, INVALID_PARAMS, err.to_string()),
        StopError::Session(e) => invalid_state_error(req_id, operation, e),
    }
}

pub(crate) async fn handle_session_resume(req: Request, sm: &Arc<SessionManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    // A session that this daemon does not hold (an earlier daemon recorded
    // it) resumes from storage. The client asks once, and the daemon decides
    // where the session is. The dispatcher runs the start checks after
    // either path.
    let resumed = match sm.resume_session(session_id).await {
        Err(SessionError::NotFound(_)) => resume_stored(sm, session_id).await,
        resumed => resumed,
    };
    match resumed {
        Ok(previous_state) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "previous_state": format!("{}", previous_state),
                "state": "active",
            }),
        ),
        Err(e) => invalid_state_error(req.id, "resume", e),
    }
}

/// Resume a session from its stored record, and answer its stored state.
async fn resume_stored(
    sm: &Arc<SessionManager>,
    session_id: &str,
) -> Result<SessionState, SessionError> {
    let id =
        SessionId::parse(session_id).map_err(|_| SessionError::NotFound(session_id.to_string()))?;
    let previous = sm
        .read_session(session_id)
        .await?
        .ok_or_else(|| SessionError::NotFound(session_id.to_string()))?
        .state;
    sm.resume_session_from_storage(&id).await?;
    Ok(previous)
}

pub(crate) async fn handle_session_resume_from_storage(
    req: Request,
    sm: &Arc<SessionManager>,
) -> Response {
    let params = match typed_params::<SessionResumeFromStorageRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &match session_id_field(&params.session_id, &req) {
        Ok(id) => id,
        Err(response) => return *response,
    };
    let (limit, offset) = (params.limit, params.offset);

    // Resume session from storage
    let session = match sm.resume_session_from_storage(session_id).await {
        Ok(s) => s,
        Err(e) => return invalid_state_error(req.id, "resume_from_storage", e),
    };

    history_reply(req.id, &session, sm, limit, offset).await
}

/// Read a session's transcript without making the session live.
///
/// `session.resume_from_storage` answers the same shape, but it sets the
/// session to `Active` and the dispatcher runs the start checks after it,
/// which can pull a container. A page that only shows the transcript must not
/// do that, so this reads memory or storage and changes nothing: the session
/// keeps its state, it does not enter memory, and no hook runs.
pub(crate) async fn handle_session_history(req: Request, sm: &Arc<SessionManager>) -> Response {
    let params = match typed_params::<SessionHistoryRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &match session_id_field(&params.session_id, &req) {
        Ok(id) => id,
        Err(response) => return *response,
    };
    let session = match sm.read_session(session_id).await {
        Ok(Some(session)) => session,
        Ok(None) => return session_not_found(req.id, session_id),
        Err(e) => return internal_error(req.id, e),
    };
    history_reply(req.id, &session, sm, params.limit, params.offset).await
}

/// The session and one page of its stored events.
async fn history_reply(
    req_id: Option<RequestId>,
    session: &crucible_core::session::Session,
    sm: &Arc<SessionManager>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Response {
    let history = match sm.load_session_events(&session.id, limit, offset).await {
        Ok(events) => events,
        Err(e) => {
            // The session is there but its history did not load: answer the
            // session with no history. Log internally but don't expose error
            // details to the client.
            warn!("Failed to load session history: {}", e);
            return Response::success(
                req_id,
                serde_json::json!({
                    "session_id": session.id,
                    "type": session.session_type.as_prefix(),
                    "state": format!("{}", session.state),
                    "kilns": session.kilns,
                    "history": [],
                    "total_events": 0,
                    "transcript": crucible_core::transcript::Transcript::default(),
                }),
            );
        }
    };

    // Get total event count for pagination
    let total = sm.count_session_events(&session.id).await.unwrap_or(0);
    // The fold of the whole log. A client draws it; the page of raw events
    // stays for a client that reads them.
    let transcript = match sm.load_transcript(&session.id).await {
        Ok(transcript) => transcript,
        Err(e) => {
            warn!("Failed to fold session history: {}", e);
            crucible_core::transcript::Transcript::default()
        }
    };

    Response::success(
        req_id,
        serde_json::json!({
            "session_id": session.id,
            "type": session.session_type.as_prefix(),
            "state": format!("{}", session.state),
            "kilns": session.kilns,
            "history": history,
            "total_events": total,
            "transcript": transcript,
        }),
    )
}

pub(crate) async fn handle_session_end(req: Request, lifecycle: &SessionLifecycle) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    match lifecycle.stop(session_id, StopCause::End).await {
        Ok(Stopped::Ended(session)) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "state": "ended",
                "kilns": session.kilns,
            }),
        ),
        Ok(other) => unexpected_stop(req.id, "end", other),
        Err(e) => stop_error(req.id, "end", e),
    }
}

/// Deleting the parent deletes its delegated children too: the stop owner
/// stops each child the same way.
pub(crate) async fn handle_session_delete(req: Request, lifecycle: &SessionLifecycle) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &match session_id_field(&params.session_id, &req) {
        Ok(id) => id,
        Err(response) => return *response,
    };

    match lifecycle.stop(session_id.as_str(), StopCause::Delete).await {
        Ok(Stopped::Deleted) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "deleted": true,
            }),
        ),
        Ok(other) => unexpected_stop(req.id, "delete", other),
        Err(e) => stop_error(req.id, "delete", e),
    }
}

/// Children are lifecycle-subordinate: archiving the parent archives its
/// delegated children too, through the same stop.
pub(crate) async fn handle_session_archive(req: Request, lifecycle: &SessionLifecycle) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &match session_id_field(&params.session_id, &req) {
        Ok(id) => id,
        Err(response) => return *response,
    };

    match lifecycle
        .stop(session_id.as_str(), StopCause::Archive)
        .await
    {
        Ok(Stopped::Archived(session)) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "archived": session.archived,
            }),
        ),
        Ok(other) => unexpected_stop(req.id, "archive", other),
        Err(e) => stop_error(req.id, "archive", e),
    }
}

pub(crate) async fn handle_session_unarchive(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
    events: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &match session_id_field(&params.session_id, &req) {
        Ok(id) => id,
        Err(response) => return *response,
    };

    match sm.unarchive_session(session_id).await {
        Ok(session) => {
            am.cleanup_session(session_id, events);
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "archived": session.archived,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "unarchive", e),
    }
}

pub(crate) async fn handle_session_replay(
    req: Request,
    sm: &Arc<SessionManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<SessionReplayRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let speed = params.speed;

    let recording_path = PathBuf::from(params.recording_path);
    // A UUID is hex and hyphens, so this is a valid id by construction; the
    // `expect` is a tripwire on the format string, not a runtime possibility.
    let replay_session_id =
        crucible_core::session::SessionId::parse(&format!("replay-{}", uuid::Uuid::new_v4()))
            .expect("a uuid-suffixed replay id is a single path component");

    match ReplaySession::new(
        recording_path,
        speed,
        event_tx.clone(),
        replay_session_id.clone(),
    ) {
        Ok(replay) => {
            sm.register_transient(replay.session().clone());
            let _handle = replay.start();

            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": replay_session_id.as_str(),
                    "status": "replaying",
                    "speed": speed,
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_compact(req: Request, sm: &Arc<SessionManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    match sm.request_compaction(session_id).await {
        Ok(session) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session.id,
                "state": format!("{}", session.state),
                "compaction_requested": true,
            }),
        ),
        Err(e) => invalid_state_error(req.id, "compact", e),
    }
}
