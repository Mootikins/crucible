use super::super::*;
use crate::require_param;
use crate::rpc_client::{SessionDismissNotificationRequest, SessionIdRequest};
use crate::rpc_helpers::typed_params;

/// Store a notification in the hub, scoped to the session.
pub(crate) async fn handle_session_add_notification(
    req: Request,
    am: &Arc<AgentManager>,
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

    if am.get_session(session_id).is_err() {
        return session_not_found(req.id, session_id);
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
    am: &Arc<AgentManager>,
    hub: &Arc<crate::notifications::NotificationHub>,
) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;
    if am.get_session(session_id).is_err() {
        return session_not_found(req.id, session_id);
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
    am: &Arc<AgentManager>,
    hub: &Arc<crate::notifications::NotificationHub>,
) -> Response {
    let params = match typed_params::<SessionDismissNotificationRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let (session_id, notification_id) = (&params.session_id, &params.notification_id);
    if am.get_session(session_id).is_err() {
        return session_not_found(req.id, session_id);
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
