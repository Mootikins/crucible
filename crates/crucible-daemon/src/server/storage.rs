use super::*;
use crucible_core::protocol::requests::NotImplementedReply;

pub(crate) async fn handle_storage_verify(req: Request) -> Response {
    typed_success(
        req.id,
        NotImplementedReply {
            status: "not_implemented".to_string(),
            message: "Storage verification is not yet implemented. Use `cru process --force` to rebuild storage.".to_string(),
        },
    )
}

pub(crate) async fn handle_storage_cleanup(req: Request) -> Response {
    typed_success(
        req.id,
        NotImplementedReply {
            status: "not_implemented".to_string(),
            message: "Storage cleanup is not yet implemented.".to_string(),
        },
    )
}

pub(crate) async fn handle_storage_backup(req: Request) -> Response {
    typed_success(
        req.id,
        NotImplementedReply {
            status: "not_implemented".to_string(),
            message: "Storage backup is not yet implemented. Copy the .crucible directory directly for backup.".to_string(),
        },
    )
}

pub(crate) async fn handle_storage_restore(req: Request) -> Response {
    typed_success(
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
