use super::super::*;
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::{NewNotification, NotificationKey, Scoped};

/// The session, live or in storage, or the refusal when it is neither. A
/// session in storage only, after a restart, has notifications too.
async fn stored(
    sessions: &crate::session_manager::SessionManager,
    session_id: &str,
    id: Option<RequestId>,
) -> Result<crucible_core::session::Session, Box<Response>> {
    match sessions.read_session(session_id).await {
        Ok(Some(session)) => Ok(session),
        Ok(None) => Err(Box::new(session_not_found(id, session_id))),
        Err(e) => Err(Box::new(internal_error(id, e))),
    }
}

/// Store a notification in the hub, scoped to the session.
pub(crate) async fn handle_session_add_notification(
    req: Request,
    sessions: &Arc<crate::session_manager::SessionManager>,
    hub: &Arc<crate::notifications::NotificationHub>,
) -> Response {
    let params = match typed_params::<Scoped<NewNotification>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let notification = params.body.notification;

    if let Err(response) = stored(sessions, session_id, req.id.clone()).await {
        return *response;
    }
    match hub.add_for_session(session_id, notification) {
        Ok(_) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "success": true,
            }),
        ),
        Err(e) => internal_error(req.id, e),
    }
}

/// Every notification the hub delivers to one session.
pub(crate) async fn handle_session_list_notifications(
    req: Request,
    sessions: &Arc<crate::session_manager::SessionManager>,
    hub: &Arc<crate::notifications::NotificationHub>,
) -> Response {
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session = match stored(sessions, &params.session_id, req.id.clone()).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": params.session_id,
            "notifications": hub.list_for_session(&session),
        }),
    )
}

/// Close one notification for one session. The hub drops a notification
/// of the session, and hides a shared one for this session only.
pub(crate) async fn handle_session_dismiss_notification(
    req: Request,
    sessions: &Arc<crate::session_manager::SessionManager>,
    hub: &Arc<crate::notifications::NotificationHub>,
) -> Response {
    let params = match typed_params::<Scoped<NotificationKey>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let (session_id, notification_id) = (&params.session_id, &params.body.notification_id);
    let session = match stored(sessions, session_id, req.id.clone()).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "notification_id": notification_id,
            "success": hub.dismiss_for_session(&session, notification_id),
        }),
    )
}
