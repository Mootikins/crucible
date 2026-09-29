use super::super::*;
use crate::rpc_helpers::typed_params;
use crucible_core::protocol::requests::{PluginApprovalChange, PluginRef, Scoped};
use crucible_core::session::PluginApproval;

pub(crate) async fn handle_session_set_plugin_approval(
    req: Request,
    am: &Arc<AgentManager>,
    event_tx: &crate::EventBus,
) -> Response {
    let params = match typed_params::<Scoped<PluginApprovalChange>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let plugin = params.body.plugin.as_str();
    let value = params.body.approval.as_str();
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
    let params = match typed_params::<Scoped<PluginRef>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = params.session_id.as_str();
    let plugin = params.body.plugin.as_str();
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
    let params = match typed_params::<Scoped<()>>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    match am.list_plugin_approvals(&params.session_id).await {
        Ok(approvals) => Response::success(req.id, serde_json::json!({"approvals": approvals})),
        Err(e) => agent_error_to_response(req.id, e),
    }
}
