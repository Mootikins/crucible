//! `/api/session/{id}/review/…` — the attributed-diff review surface.
//!
//! The browser never speaks raw JSON-RPC, so these three routes are the only
//! way `ChangesPanel`, the file viewer's gutter, and `ToolCard` reach the
//! daemon's `review.*` methods. They are registered inside
//! [`super::session_routes_with`] rather than as their own group, and that is
//! load-bearing: bearer auth, the host guard, the CORS allowlist, the body
//! limit, and the CSP/`nosniff`/`Referrer-Policy` headers are all applied to
//! the session router as a whole. A per-route layer here would be the seam
//! along which the review surface drifts out from under them.
//!
//! **Each reply is named here, and the names are what the browser reads.** The
//! routes forwarded `serde_json::Value` until task A6, so the OpenAPI document
//! could say nothing about them and `review-api.ts` described the seven shapes
//! by hand — where it got the degraded roots and three `session_id` keys
//! wrong. A named struct is what puts the fields in the document and in
//! the generated TypeScript.
//!
//! The cost the old note warned about is real: a struct drops a key the daemon
//! added, and a new feature goes silently missing rather than failing to
//! compile. `review_shape_tests.rs` answers it. Each row round-trips the
//! *core* type the daemon serialises — `ComposedHunk`, `Comment`,
//! `RootStatus`, `Integrity` — and demands the same JSON back, so
//! a field added in `crucible-core` fails a test here instead of disappearing
//! on the way to the browser.

use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use crucible_core::session::ReviewScope;
use crucible_daemon::rpc_client::ReviewCommentRequest;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::daemon_shape;

// =========================================================================
// Requests
// =========================================================================

/// `GET /review/hunks?scope=` — which hunks to list.
///
/// Typed, because this is a mirror of the daemon's own type, which
/// [`ReviewScopeRow`] converts into. A scope the type does not
/// know is refused as a bad query before the daemon is asked; absent means the
/// whole session.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListHunksQuery {
    /// The hunks to list. Absent means the whole session.
    #[serde(default)]
    scope: Option<ReviewScopeRow>,
}

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

/// Which hunks a listing covers, as `crucible_core::session::ReviewScope`
/// spells it on the wire.
///
/// Web-owned because a schema is what puts the two values in the document, and
/// `crucible-core` takes no `utoipa` dependency. [`From`] converts it to the
/// core type, so a variant added there fails to compile here rather than
/// reaching the daemon as a scope this crate silently narrowed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReviewScopeRow {
    /// Every hunk in the session.
    #[default]
    Session,
    /// Only the hunks the current turn produced.
    Turn,
}

impl From<ReviewScopeRow> for ReviewScope {
    fn from(row: ReviewScopeRow) -> Self {
        match row {
            ReviewScopeRow::Session => ReviewScope::Session,
            ReviewScopeRow::Turn => ReviewScope::Turn,
        }
    }
}

/// One user decision about one hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum ReviewStateRow {
    Unreviewed,
    Accepted,
    Rejected,
}

/// A half-open range of 1-based line numbers: `end` is one past the last line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct LineRangeRow {
    /// First line, 1-based, inclusive.
    start: u32,
    /// One past the last line, 1-based, exclusive.
    end: u32,
}

/// One hunk of the composed diff, the unit a decision applies to.
///
/// There is no `external` field, here or on the wire: a hunk is external when
/// `tool_call_ids` is empty, which is what `isExternal` reads in the browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewHunkRow {
    /// Content-derived, and the only handle a decision names.
    id: String,
    /// The repository top level this hunk belongs to.
    root: String,
    /// The path, relative to `root`.
    path: String,
    /// The lines this hunk replaces, in base coordinates.
    base_range: LineRangeRow,
    /// The lines it occupies now, in worktree coordinates.
    current_range: LineRangeRow,
    /// The base-side text. Empty for a pure insertion.
    before_content: String,
    /// The current-side text. Empty for a pure deletion.
    after_content: String,
    /// The tool calls whose writes survive into this hunk, in ledger order.
    tool_call_ids: Vec<String>,
    state: ReviewStateRow,
    /// The agent applied a change the user had rejected. The state comes back
    /// `unreviewed`; this is the history that makes the grind visible.
    #[serde(default)]
    reapplied: bool,
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

/// Whether the ledger can still account for one tracked root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewRootRow {
    /// The repository top level.
    root: String,
    /// Why this root's attribution cannot be trusted. `null` is intact, and
    /// the string is shown to the user as the daemon wrote it. The key is
    /// always written, so `required` rather than optional.
    #[schema(required = true)]
    degraded: Option<String>,
}

/// What a skipped journal record costs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub(super) enum ReviewSkipKindRow {
    /// The session's base is untrustworthy, so every root is degraded —
    /// including ones the ledger may no longer know it tracked.
    Session,
    /// One root's attribution is incomplete. Only that root is degraded.
    Root { root: String },
    /// A lost decision or comment. The queue is poorer, not wrong.
    Informational,
}

/// One journal record the daemon could not read back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewSkipRow {
    record: ReviewSkipKindRow,
    /// The 1-based line in `review.jsonl`, so an operator can find it.
    line: u32,
    /// The parse failure, verbatim.
    reason: String,
}

/// What the journal could not be read back as.
///
/// Separate from the degraded roots because the worst losses are the ones with
/// no root to name: a journal that will not read at all leaves `degraded`
/// empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewIntegrityRow {
    skips: Vec<ReviewSkipRow>,
}

// =========================================================================
// The replies
// =========================================================================

/// What `GET /api/session/{id}/review/hunks` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewHunksResponse {
    session_id: String,
    /// The scope the answer describes, echoed. A client that switched scope
    /// while a listing was in flight reads it to drop the stale answer.
    scope: ReviewScopeRow,
    hunks: Vec<ReviewHunkRow>,
    comments: Vec<ReviewCommentRow>,
    /// Only the roots that are broken. An empty array is the common case.
    degraded: Vec<ReviewRootRow>,
    integrity: ReviewIntegrityRow,
}

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

/// `GET /api/session/{id}/review/hunks?scope=session|turn`
#[utoipa::path(
    get,
    path = "/api/session/{id}/review/hunks",
    params(("id" = String, Path, description = "The session under review"), ListHunksQuery),
    responses(
        (status = 200, body = ReviewHunksResponse),
        (status = 400, description = "The `scope` query named something that is neither `session` nor `turn`"),
        (status = 422, description = "The session has no reviewable root"),
        (status = 502, description = "The daemon could not read the journal"),
    )
)]
pub(super) async fn list_hunks(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListHunksQuery>,
) -> Result<Json<ReviewHunksResponse>, WebError> {
    let hunks = state
        .daemon
        .review_list_hunks(&id, query.scope.map(ReviewScope::from))
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(hunks, "review.list_hunks")?))
}

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
