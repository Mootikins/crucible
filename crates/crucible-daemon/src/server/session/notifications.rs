use super::super::*;
use crate::require_param;

pub(crate) async fn handle_session_add_notification(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let notification_obj = require_param!(req, "notification", as_object);

    let notification = match serde_json::from_value::<crucible_core::types::Notification>(
        serde_json::Value::Object(notification_obj.clone()),
    ) {
        Ok(n) => n,
        Err(e) => return Response::error(req.id, -32602, format!("Invalid notification: {}", e)),
    };

    match am
        .add_notification(session_id, notification, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "success": true,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_list_notifications(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);

    match am.list_notifications(session_id).await {
        Ok(notifications) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "notifications": notifications,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) async fn handle_session_dismiss_notification(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let notification_id = require_param!(req, "notification_id", as_str);

    match am
        .dismiss_notification(session_id, notification_id, Some(event_tx))
        .await
    {
        Ok(success) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "notification_id": notification_id,
                "success": success,
            }),
        ),
        Err(crate::agent_manager::AgentError::SessionNotFound(id)) => {
            session_not_found(req.id, &id)
        }
        Err(e) => internal_error(req.id, e),
    }
}
