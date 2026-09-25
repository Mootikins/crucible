use super::super::*;
use crate::require_param;
use crate::rpc_client::{SessionDismissNotificationRequest, SessionIdRequest};
use crate::rpc_helpers::typed_params;

/// The refusal when the session is neither live nor in storage. A session
/// in storage only, after a restart, has notifications too.
async fn refusal(
    sessions: &crate::session_manager::SessionManager,
    session_id: &str,
    id: Option<RequestId>,
) -> Option<Response> {
    match sessions.read_session(session_id).await {
        Ok(Some(_)) => None,
        Ok(None) => Some(session_not_found(id, session_id)),
        Err(e) => Some(internal_error(id, e)),
    }
}

/// Store a notification in the hub, scoped to the session.
pub(crate) async fn handle_session_add_notification(
    req: Request,
    sessions: &Arc<crate::session_manager::SessionManager>,
    hub: &Arc<crate::notifications::NotificationHub>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let notification_obj = require_param!(req, "notification", as_object);

    let notification = match serde_json::from_value::<crucible_core::types::Notification>(
        serde_json::Value::Object(notification_obj.clone()),
    ) {
        Ok(n) => n,
        Err(e) => return Response::error(req.id, -32602, format!("Invalid notification: {}", e)),
    };

    if let Some(response) = refusal(sessions, session_id, req.id.clone()).await {
        return response;
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

/// The notifications of one session, from the hub.
pub(crate) async fn handle_session_list_notifications(
    req: Request,
    sessions: &Arc<crate::session_manager::SessionManager>,
    hub: &Arc<crate::notifications::NotificationHub>,
) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;
    if let Some(response) = refusal(sessions, session_id, req.id.clone()).await {
        return response;
    }
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "notifications": hub.list_for_session(session_id),
        }),
    )
}

/// Remove one notification of one session from the hub.
pub(crate) async fn handle_session_dismiss_notification(
    req: Request,
    sessions: &Arc<crate::session_manager::SessionManager>,
    hub: &Arc<crate::notifications::NotificationHub>,
) -> Response {
    let params = match typed_params::<SessionDismissNotificationRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let (session_id, notification_id) = (&params.session_id, &params.notification_id);
    if let Some(response) = refusal(sessions, session_id, req.id.clone()).await {
        return response;
    }
    Response::success(
        req.id,
        serde_json::json!({
            "session_id": session_id,
            "notification_id": notification_id,
            "success": hub.dismiss_for_session(session_id, notification_id),
        }),
    )
}
