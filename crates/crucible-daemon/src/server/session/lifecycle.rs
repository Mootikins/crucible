use super::super::*;
use crate::rpc_client::{SessionIdRequest, SessionResumeFromStorageRequest};
use crate::rpc_helpers::{session_id_field, typed_params};
use crate::session_lifecycle::{SessionLifecycle, StopCause, StopError, Stopped};

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
        StopError::Session(e) => invalid_state_error(req_id, operation, e),
    }
}

pub(crate) async fn handle_session_resume(req: Request, sm: &Arc<SessionManager>) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;

    match sm.resume_session(session_id).await {
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

    // Load event history with pagination
    let history = match sm.load_session_events(session_id, limit, offset).await {
        Ok(events) => events,
        Err(e) => {
            // Session resumed but history load failed - return session without history
            // Log internally but don't expose error details to client
            warn!("Failed to load session history: {}", e);
            return Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "type": session.session_type.as_prefix(),
                    "state": format!("{}", session.state),
                    "kilns": session.kilns,
                    "history": [],
                    "total_events": 0,
                }),
            );
        }
    };

    // Get total event count for pagination
    let total = sm.count_session_events(session_id).await.unwrap_or(0);

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session.id,
            "type": session.session_type.as_prefix(),
            "state": format!("{}", session.state),
            "kilns": session.kilns,
            "history": history,
            "total_events": total,
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
            am.cleanup_session(session_id);
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
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<crate::rpc_client::SessionReplayRequest>(&req) {
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
