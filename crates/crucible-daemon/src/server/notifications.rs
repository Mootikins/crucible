//! `notification.list` and `notification.dismiss`: the read and the drop
//! side of the daemon's notification ring. The write side is
//! `cru.log.notify` in a VM; there is no RPC to add one.

use super::*;
use crate::notifications::NotificationHub;
use crate::rpc_client::{NotificationDismissRequest, NotificationListRequest};
use crate::rpc_helpers::typed_params;
use crucible_core::config::KilnName;

pub(crate) async fn handle_notification_list(req: Request, hub: &Arc<NotificationHub>) -> Response {
    let params = match typed_params::<NotificationListRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let mut kilns = Vec::with_capacity(params.kilns.len());
    for name in &params.kilns {
        match KilnName::parse(name) {
            Ok(kiln) => kilns.push(kiln),
            Err(e) => {
                return Response::error(req.id, INVALID_PARAMS, format!("kiln '{name}': {e}"))
            }
        }
    }
    let workspace = params.workspace.as_deref().map(Path::new);
    let notifications = hub.list(workspace, &kilns, params.all);
    Response::success(
        req.id,
        serde_json::json!({ "notifications": notifications }),
    )
}

pub(crate) async fn handle_notification_dismiss(
    req: Request,
    hub: &Arc<NotificationHub>,
) -> Response {
    let params = match typed_params::<NotificationDismissRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let dismissed = hub.dismiss(&params.id);
    Response::success(req.id, serde_json::json!({ "dismissed": dismissed }))
}
