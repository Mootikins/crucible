//! `/api/session/{id}/review/…` — the comment aliases of a session record.
//!
//! The browser never speaks raw JSON-RPC, so these two routes are the way it
//! reaches the daemon's `review.*` methods. The session record itself comes
//! from `GET /api/diff?session=`. They are registered inside
//! [`super::session_routes_with`] rather than as their own group, and that is
//! load-bearing: bearer auth, the host guard, the CORS allowlist, the body
//! limit, and the CSP/`nosniff`/`Referrer-Policy` headers are all applied to
//! the session router as a whole. A per-route layer here would be the seam
//! along which the review surface drifts out from under them.
//!
//! **Each reply is named here, and the names are what the browser reads.** The
//! routes forwarded `serde_json::Value` until task A6, so the OpenAPI document
//! could say nothing about them and `review-api.ts` described the shapes by
//! hand, and got some of them wrong. A named struct is what puts the fields in the document and in
//! the generated TypeScript.
//!
//! The cost the old note warned about is real: a struct drops a key the daemon
//! added, and a new feature goes silently missing rather than failing to
//! compile. `review_shape_tests.rs` answers it. Each row round-trips the
//! *core* type the daemon serialises — `Comment` — and demands the same JSON
//! back, so
//! a field added in `crucible-core` fails a test here instead of disappearing
//! on the way to the browser.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, State},
    Json,
};
use crucible_daemon::rpc_client::ReviewCommentRequest;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::daemon_shape;

// =========================================================================
// Requests
// =========================================================================

/// `POST /review/comment` — anchor a comment to a line range.
///
/// Read into a typed body rather than forwarded as raw JSON, so the session
/// under review can only ever be the one in the path: a `session_id` in the
/// body is an unknown field here. The handler copies the fields into a
/// `ReviewCommentRequest` with the path's session id.
#[derive(Debug, Deserialize, ToSchema)]
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

// =========================================================================
// The rows the daemon's review types serialise as
// =========================================================================

/// A half-open range of 1-based line numbers: `end` is one past the last line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct LineRangeRow {
    /// First line, 1-based, inclusive.
    start: u32,
    /// One past the last line, 1-based, exclusive.
    end: u32,
}

/// Who wrote a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentAuthorRow {
    Human,
    Agent,
}

/// What a comment is anchored in. The web mirror of
/// `crucible_core::session::CommentAnchor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub(super) enum CommentAnchorRow {
    /// The session base snapshot of a session record.
    Snapshot(String),
    /// The merge-base commit of a branch diff.
    Commit(String),
    /// One proposal id.
    Proposal(String),
}

/// The side of a diff that a comment range counts its lines on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommentSideRow {
    Base,
    Current,
}

impl From<CommentAuthorRow> for crucible_core::session::CommentAuthor {
    fn from(row: CommentAuthorRow) -> Self {
        match row {
            CommentAuthorRow::Human => Self::Human,
            CommentAuthorRow::Agent => Self::Agent,
        }
    }
}

impl From<CommentSideRow> for crucible_core::session::CommentSide {
    fn from(row: CommentSideRow) -> Self {
        match row {
            CommentSideRow::Base => Self::Base,
            CommentSideRow::Current => Self::Current,
        }
    }
}

/// One review comment, anchored to a line range rather than to a hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(crate) struct ReviewCommentRow {
    id: String,
    /// The diffset that owns the comment.
    diffset: String,
    /// The repository top level.
    root: String,
    /// The path, relative to `root`.
    path: String,
    /// What the diffset compares with when the comment was made.
    anchor: CommentAnchorRow,
    /// The side that `line_range` counts its lines on.
    side: CommentSideRow,
    line_range: LineRangeRow,
    /// The text of the range on `side` when the comment was made.
    quoted: String,
    body: String,
    author: CommentAuthorRow,
    resolved: bool,
    /// When the comment was written, RFC 3339.
    ///
    /// A string rather than a date type: the daemon's spelling reaches the
    /// browser unchanged, and a parse and a reformat here could only lose
    /// precision the daemon sent.
    #[schema(format = DateTime)]
    created_at: String,
}

// =========================================================================
// The replies
// =========================================================================

/// What `POST /api/session/{id}/review/comment` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewCommentResponse {
    session_id: String,
    /// The comment as it was stored, with the id and the time the daemon
    /// minted.
    comment: ReviewCommentRow,
}

/// What `POST /api/session/{id}/review/comment/{comment_id}/resolve` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewResolveCommentResponse {
    session_id: String,
    comment_id: String,
    resolved: bool,
}

// =========================================================================
// Handlers
// =========================================================================

/// `POST /api/session/{id}/review/comment`
#[utoipa::path(
    post,
    path = "/api/session/{id}/review/comment",
    params(("id" = String, Path, description = "The session under review")),
    request_body = CommentRequest,
    responses(
        (status = 200, body = ReviewCommentResponse),
        (status = 422, description = "The path names no tracked root, or the author is neither `human` nor `agent`"),
        (status = 502, description = "The daemon could not store the comment"),
    )
)]
pub(super) async fn comment(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CommentRequest>,
) -> Result<Json<ReviewCommentResponse>, WebError> {
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
    Ok(Json(daemon_shape(result, "review.comment")?))
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
#[utoipa::path(
    post,
    path = "/api/session/{id}/review/comment/{comment_id}/resolve",
    params(
        ("id" = String, Path, description = "The session under review"),
        ("comment_id" = String, Path, description = "The comment to mark answered"),
    ),
    responses(
        (status = 200, body = ReviewResolveCommentResponse),
        (status = 422, description = "The daemon knows no such comment"),
        (status = 502, description = "The daemon could not resolve the comment"),
    )
)]
pub(super) async fn resolve_comment(
    State(state): State<AppState>,
    Path((id, comment_id)): Path<(String, String)>,
) -> Result<Json<ReviewResolveCommentResponse>, WebError> {
    let result = state
        .daemon
        .review_resolve_comment(&id, &comment_id)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(result, "review.resolve_comment")?))
}

#[cfg(test)]
#[path = "review_shape_tests.rs"]
mod shape_tests;
