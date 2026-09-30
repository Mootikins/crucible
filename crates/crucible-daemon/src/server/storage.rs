use super::*;
use crucible_core::protocol::requests::NotImplementedReply;

/// Answer with `value` as JSON, or report the serialisation failure.
///
/// The reply type here holds only strings, so the error arm is unreachable
/// in practice. It exists because an `expect` here would take the daemon
/// down over a reply nobody can act on.
fn reply<T: serde::Serialize>(id: Option<crate::protocol::RequestId>, value: T) -> Response {
    match serde_json::to_value(value) {
        Ok(value) => Response::success(id, value),
        Err(e) => internal_error(id, anyhow::anyhow!(e)),
    }
}

pub(crate) async fn handle_storage_verify(req: Request) -> Response {
    reply(
        req.id,
        NotImplementedReply {
            status: "not_implemented".to_string(),
            message: "Storage verification is not yet implemented. Use `cru process --force` to rebuild storage.".to_string(),
        },
    )
}

pub(crate) async fn handle_storage_cleanup(req: Request) -> Response {
    reply(
        req.id,
        NotImplementedReply {
            status: "not_implemented".to_string(),
            message: "Storage cleanup is not yet implemented.".to_string(),
        },
    )
}

pub(crate) async fn handle_storage_backup(req: Request) -> Response {
    reply(
        req.id,
        NotImplementedReply {
            status: "not_implemented".to_string(),
            message: "Storage backup is not yet implemented. Copy the .crucible directory directly for backup.".to_string(),
        },
    )
}

pub(crate) async fn handle_storage_restore(req: Request) -> Response {
    reply(
        req.id,
        NotImplementedReply {
            status: "not_implemented".to_string(),
            message: "Storage restore is not yet implemented.".to_string(),
        },
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Session RPC handlers
// ─────────────────────────────────────────────────────────────────────────────
