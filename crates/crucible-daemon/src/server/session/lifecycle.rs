use super::super::*;
use crate::{optional_param, require_param};

pub(crate) async fn handle_session_pause(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.pause_session(session_id).await {
        Ok(previous_state) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "previous_state": format!("{}", previous_state),
                "state": "paused",
            }),
        ),
        Err(e) => invalid_state_error(req.id, "pause", e),
    }
}

pub(crate) async fn handle_session_resume(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

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
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    // Optional pagination params
    let limit = optional_param!(req, "limit", as_u64).map(|n| n as usize);
    let offset = optional_param!(req, "offset", as_u64).map(|n| n as usize);

    // Resume session from storage
    let session = match sm.resume_session_from_storage(session_id, &kiln).await {
        Ok(s) => s,
        Err(e) => return invalid_state_error(req.id, "resume_from_storage", e),
    };

    // Load event history with pagination
    let history = match sm
        .load_session_events(session_id, &kiln, limit, offset)
        .await
    {
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
                    "kiln": session.kiln,
                    "history": [],
                    "total_events": 0,
                }),
            );
        }
    };

    // Get total event count for pagination
    let total = sm
        .count_session_events(session_id, &kiln)
        .await
        .unwrap_or(0);

    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session.id,
            "type": session.session_type.as_prefix(),
            "state": format!("{}", session.state),
            "kiln": session.kiln,
            "history": history,
            "total_events": total,
        }),
    )
}

pub(crate) async fn handle_session_end(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match sm.end_session(session_id).await {
        Ok(session) => {
            am.cleanup_session(session_id);
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "state": "ended",
                    "kiln": session.kiln,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "end", e),
    }
}

pub(crate) async fn handle_session_delete(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    match sm.delete_session(session_id, &kiln).await {
        Ok(()) => {
            am.cleanup_session(session_id);
            // Deleting the parent deletes its delegated children too —
            // an orphaned hidden child would be unreachable otherwise.
            for child_id in sm.child_session_ids(session_id, &kiln).await {
                if let Err(e) = sm.delete_session(&child_id, &kiln).await {
                    warn!(child_id = %child_id, error = %e, "Failed to delete child session");
                } else {
                    am.cleanup_session(&child_id);
                }
            }
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session_id,
                    "deleted": true,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "delete", e),
    }
}

pub(crate) async fn handle_session_archive(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    match sm.archive_session(session_id, &kiln).await {
        Ok(session) => {
            am.cleanup_session(session_id);
            // Children are lifecycle-subordinate: archiving the parent
            // archives its delegated children too (best-effort).
            for child_id in sm.child_session_ids(session_id, &kiln).await {
                if let Err(e) = sm.archive_session(&child_id, &kiln).await {
                    warn!(child_id = %child_id, error = %e, "Failed to archive child session");
                } else {
                    am.cleanup_session(&child_id);
                }
            }
            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "archived": session.archived,
                }),
            )
        }
        Err(e) => invalid_state_error(req.id, "archive", e),
    }
}

pub(crate) async fn handle_session_unarchive(
    req: Request,
    sm: &Arc<SessionManager>,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let kiln = PathBuf::from(require_param!(req, "kiln", as_str));

    match sm.unarchive_session(session_id, &kiln).await {
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
    let recording_path = require_param!(req, "recording_path", as_str);
    let speed = req
        .params
        .get("speed")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    let recording_path = PathBuf::from(recording_path);
    let replay_session_id = format!("replay-{}", uuid::Uuid::new_v4());

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
                    "session_id": replay_session_id,
                    "status": "replaying",
                    "speed": speed,
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_compact(req: Request, sm: &Arc<SessionManager>) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

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
