use super::super::*;
use crate::require_param;
use crucible_core::session::PluginApproval;

pub(crate) async fn handle_session_set_plugin_approval(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let plugin = require_param!(req, "plugin", as_str);
    let value = require_param!(req, "approval", as_str);
    let approval = match value {
        "inherit" => PluginApproval::Inherit,
        "ask" => PluginApproval::Ask,
        "stop" => PluginApproval::Stop,
        _ => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                "approval must be inherit, ask or stop",
            )
        }
    };
    match am
        .set_plugin_approval(session_id, plugin, approval, Some(event_tx))
        .await
    {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({"plugin": plugin, "approval": value}),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_get_plugin_approval(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    let plugin = require_param!(req, "plugin", as_str);
    match am.get_plugin_approval(session_id, plugin).await {
        Ok(approval) => Response::success(
            req.id,
            serde_json::json!({"plugin": plugin, "approval": approval.as_str()}),
        ),
        Err(e) => agent_error_to_response(req.id, e),
    }
}

pub(crate) async fn handle_session_list_plugin_approvals(
    req: Request,
    am: &Arc<AgentManager>,
) -> Response {
    let session_id = require_param!(req, "session_id", as_str);
    match am.list_plugin_approvals(session_id).await {
        Ok(approvals) => Response::success(req.id, serde_json::json!({"approvals": approvals})),
        Err(e) => agent_error_to_response(req.id, e),
    }
}
