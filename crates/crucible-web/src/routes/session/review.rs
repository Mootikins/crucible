//! `/api/session/{id}/review/…` — the attributed-diff review surface.
//!
//! The browser never speaks raw JSON-RPC, so these seven routes are the only
//! way `ChangesPanel`, the file viewer's gutter, and `ToolCard` reach the
//! daemon's `review.*` methods. They are registered inside
//! [`super::session_routes_with`] rather than as their own group, and that is
//! load-bearing: bearer auth, the host guard, the CORS allowlist, the body
//! limit, and the CSP/`nosniff`/`Referrer-Policy` headers are all applied to
//! the session router as a whole. A per-route layer here would be the seam
//! along which the review surface drifts out from under them.
//!
//! Responses are forwarded verbatim. The daemon's result objects gain keys
//! (`degraded`, `gate`) faster than a mirrored struct in this crate could
//! track, and a struct that drops a key the frontend already reads is a
//! silently missing feature rather than a compile error. Their shape is
//! pinned by `tests/route_contract_tests/review.rs`.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use crucible_core::session::ReviewScope;
use crucible_daemon::rpc_client::ReviewCommentRequest;
use serde::Deserialize;

/// `GET /review/hunks?scope=` — which hunks to list.
///
/// Typed, unlike `state` on [`SetStateRequest`], and for the opposite reason:
/// this is not a copy of the daemon's vocabulary but the daemon's own type,
/// shared through `crucible-core`, so there is nothing here to drift. A scope
/// the type does not know is refused as a bad query before the daemon is
/// asked; absent means the whole session.
#[derive(Debug, Default, Deserialize)]
pub(super) struct ListHunksQuery {
    #[serde(default)]
    scope: Option<ReviewScope>,
}

/// `POST /review/state` — accept, reject, or requeue one hunk.
#[derive(Debug, Deserialize)]
pub(super) struct SetStateRequest {
    hunk_id: String,
    /// Forwarded unvalidated: the daemon owns the state vocabulary and answers
    /// `INVALID_PARAMS` for anything outside it. A copy of the enum here could
    /// only ever refuse a state the daemon had newly learned.
    state: String,
}

/// `POST /review/states` — one decision over several hunks, in order.
///
/// The ids reach the daemon in the order the caller gave them, because the
/// daemon applies them in that order and a reject reverts files as it goes.
/// `state` is forwarded unvalidated for the same reason as on
/// [`SetStateRequest`].
#[derive(Debug, Deserialize)]
pub(super) struct SetStatesRequest {
    hunk_ids: Vec<String>,
    state: String,
}

/// `POST /review/comment` — anchor a comment to a line range.
///
/// Read into a typed body rather than forwarded as raw JSON, so the session
/// under review can only ever be the one in the path: a `session_id` in the
/// body is an unknown field here. The handler copies the fields into a
/// `ReviewCommentRequest` with the path's session id.
#[derive(Debug, Deserialize)]
pub(super) struct CommentRequest {
    /// Absolute, or relative to the session's tracked root.
    path: String,
    /// 1-based.
    line_start: u32,
    /// 1-based, exclusive. Absent means `line_start + 1`, which the daemon
    /// applies — hence `skip_serializing_if`. Sending an explicit `null`
    /// defeats the daemon's `optional_param!` default and is not the same
    /// request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line_end: Option<u32>,
    body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    author: Option<String>,
}

/// `GET /api/session/{id}/review/hunks?scope=session|turn`
pub(super) async fn list_hunks(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListHunksQuery>,
) -> Result<Json<serde_json::Value>, WebError> {
    let hunks = state
        .daemon
        .review_list_hunks(&id, query.scope)
        .await
        .daemon_err()?;
    Ok(Json(hunks))
}

/// `POST /api/session/{id}/review/rebase`
///
/// The release for a root the daemon can no longer account for. A degraded root
/// contributes no hunks, so nothing in the queue can clear it and every write
/// under it stays held — this is the only shipped way out, which is why it is
/// a sixth route rather than something the panel infers.
///
/// The `{}` body is load-bearing for the same reason as on
/// `…/comment/{id}/resolve`: it is what carries `Content-Type:
/// application/json` and forces a preflight the CORS allowlist refuses.
pub(super) async fn rebase(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.review_rebase(&id).await.daemon_err()?;
    Ok(Json(result))
}

/// `POST /api/session/{id}/review/state`
pub(super) async fn set_state(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetStateRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .review_set_state(&id, &req.hunk_id, &req.state)
        .await
        .daemon_err()?;
    Ok(Json(result))
}

/// `POST /api/session/{id}/review/states`
///
/// The daemon answers the ids it applied and the ids it refused, each with a
/// reason. A refused hunk is part of the answer, not an error status: the
/// client shows which ones and keeps the rest, so this route forwards the
/// object whole.
pub(super) async fn set_states(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetStatesRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .review_set_states(&id, &req.hunk_ids, &req.state)
        .await
        .daemon_err()?;
    Ok(Json(result))
}

/// `POST /api/session/{id}/review/undo-reject`
///
/// Takes back the most recent reject, single or bulk, as one action. The
/// daemon owns the stack, so the request names no hunk: the session in the
/// path is the whole input. The `{}` body is load-bearing for the same reason
/// as on `…/rebase` and `…/comment/{id}/resolve`: it carries `Content-Type:
/// application/json` and forces the preflight the CORS allowlist refuses.
pub(super) async fn undo_reject(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state.daemon.review_undo_reject(&id).await.daemon_err()?;
    Ok(Json(result))
}

/// `POST /api/session/{id}/review/comment`
pub(super) async fn comment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CommentRequest>,
) -> Result<Json<serde_json::Value>, WebError> {
    let request = ReviewCommentRequest {
        session_id: id,
        path: req.path,
        body: req.body,
        line_start: req.line_start,
        line_end: req.line_end,
        root: req.root,
        author: req.author,
    };
    let result = state.daemon.review_comment(request).await.daemon_err()?;
    Ok(Json(result))
}

/// `POST /api/session/{id}/review/comment/{comment_id}/resolve`
///
/// The client sends a `{}` body this handler never reads, and that is
/// deliberate — see the note on `resolveReviewComment` in `review-api.ts`.
/// The empty object forces `Content-Type: application/json`, which puts the
/// request outside the CORS simple-request set and makes the browser preflight
/// it against an allowlist that refuses cross-origin callers. It is the second
/// layer behind the `SameSite=Strict` auth cookie, not a redundant one: an
/// "optimisation" that drops the body drops the header, and every review write
/// becomes something a foreign page can fire blind.
pub(super) async fn resolve_comment(
    State(state): State<AppState>,
    Path((id, comment_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, WebError> {
    let result = state
        .daemon
        .review_resolve_comment(&id, &comment_id)
        .await
        .daemon_err()?;
    Ok(Json(result))
}
