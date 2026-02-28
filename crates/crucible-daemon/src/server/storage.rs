use super::*;

pub(super) async fn handle_storage_verify(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "status": "not_implemented",
            "message": "Storage verification is not yet implemented. Use `cru process --force` to rebuild storage."
        }),
    )
}

pub(super) async fn handle_storage_cleanup(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "status": "not_implemented",
            "message": "Storage cleanup is not yet implemented."
        }),
    )
}

pub(super) async fn handle_storage_backup(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "status": "not_implemented",
            "message": "Storage backup is not yet implemented. Copy the .crucible directory directly for backup."
        }),
    )
}

pub(super) async fn handle_storage_restore(req: Request) -> Response {
    Response::success(
        req.id,
        serde_json::json!({
            "status": "not_implemented",
            "message": "Storage restore is not yet implemented."
        }),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Session RPC handlers
// ─────────────────────────────────────────────────────────────────────────────
