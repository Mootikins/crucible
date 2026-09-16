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
//! **Each reply is named here, and the names are what the browser reads.** The
//! routes forwarded `serde_json::Value` until task A6, so the OpenAPI document
//! could say nothing about them and `review-api.ts` described the seven shapes
//! by hand — where it got the gate, the degraded roots and three `session_id`
//! keys wrong. A named struct is what puts the fields in the document and in
//! the generated TypeScript.
//!
//! The cost the old note warned about is real: a struct drops a key the daemon
//! added, and a new feature goes silently missing rather than failing to
//! compile. `review_shape_tests.rs` answers it. Each row round-trips the
//! *core* type the daemon serialises — `ComposedHunk`, `Comment`,
//! `RootStatus`, `Integrity`, `GateBlock` — and demands the same JSON back, so
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
/// Typed, unlike `state` on [`SetStateRequest`], and for the opposite reason:
/// this is not a copy of the daemon's vocabulary but a mirror of the daemon's
/// own type, which [`ReviewScopeRow`] converts into. A scope the type does not
/// know is refused as a bad query before the daemon is asked; absent means the
/// whole session.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct ListHunksQuery {
    /// The hunks to list. Absent means the whole session.
    #[serde(default)]
    scope: Option<ReviewScopeRow>,
}

/// `POST /review/state` — accept, reject, or requeue one hunk.
#[derive(Debug, Deserialize, ToSchema)]
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
#[derive(Debug, Deserialize, ToSchema)]
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
pub(super) enum CommentAuthorRow {
    Human,
    Agent,
}

/// One review comment, anchored to a line range rather than to a hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewCommentRow {
    id: String,
    /// The repository top level.
    root: String,
    /// The path, relative to `root`.
    path: String,
    /// The tree the range is anchored in. A range that no longer projects
    /// forward from it is outdated.
    base_tree: String,
    line_range: LineRangeRow,
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
    /// The session's base is untrustworthy, so every root blocks — including
    /// ones the ledger may no longer know it tracked.
    Session,
    /// One root's attribution is incomplete. Only writes under it block.
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
/// empty while the gate holds every write in the session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewIntegrityRow {
    skips: Vec<ReviewSkipRow>,
}

/// A turn parked on the review gate, waiting for a human.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewGateRow {
    /// The tool call being held.
    tool: String,
    /// The first target still unreviewed — what a human must answer to release
    /// the turn.
    path: String,
}

/// One hunk a bulk decision refused, with the daemon's reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewFailureRow {
    hunk_id: String,
    reason: String,
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
    /// Only the roots that are broken. An empty array is the common case; a
    /// non-empty one means the gate holds writes that no reviewing releases.
    degraded: Vec<ReviewRootRow>,
    integrity: ReviewIntegrityRow,
    /// What this session's turn is parked on, or `null`.
    ///
    /// Always written, and `required` in the document for that reason: a
    /// client has to be able to tell "not blocked" from "this daemon does not
    /// report it", and an optional key collapses the two.
    #[schema(required = true)]
    gate: Option<ReviewGateRow>,
}

/// What `POST /api/session/{id}/review/rebase` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewRebaseResponse {
    session_id: String,
    /// Every tracked root and what the rebase could do for it. A root that
    /// still carries a reason was not recovered.
    roots: Vec<ReviewRootRow>,
}

/// What `POST /api/session/{id}/review/state` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewStateResponse {
    session_id: String,
    hunk_id: String,
    state: ReviewStateRow,
}

/// What `POST /api/session/{id}/review/states` answers.
///
/// A refused hunk is part of the answer rather than an error status: the ids
/// in `applied` are on disk whatever happened to the rest, and a client told
/// only "failed" would have to re-list to learn which were which.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewStatesResponse {
    session_id: String,
    state: ReviewStateRow,
    /// The ids that applied, in the order the request sent them.
    applied: Vec<String>,
    failed: Vec<ReviewFailureRow>,
}

/// What `POST /api/session/{id}/review/undo-reject` answers.
///
/// The same report as a bulk decision, minus the state: an undo restores
/// whatever the batch rejected. An empty stack answers two empty lists.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(super) struct ReviewUndoRejectResponse {
    session_id: String,
    applied: Vec<String>,
    failed: Vec<ReviewFailureRow>,
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
#[utoipa::path(
    post,
    path = "/api/session/{id}/review/rebase",
    params(("id" = String, Path, description = "The session to rebase")),
    responses(
        (status = 200, body = ReviewRebaseResponse),
        (status = 422, description = "The session has no ledger and no trackable root"),
        (status = 502, description = "The daemon could not recapture a root"),
    )
)]
pub(super) async fn rebase(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReviewRebaseResponse>, WebError> {
    let result = state.daemon.review_rebase(&id).await.daemon_err()?;
    Ok(Json(daemon_shape(result, "review.rebase")?))
}

/// `POST /api/session/{id}/review/state`
#[utoipa::path(
    post,
    path = "/api/session/{id}/review/state",
    params(("id" = String, Path, description = "The session under review")),
    request_body = SetStateRequest,
    responses(
        (status = 200, body = ReviewStateResponse),
        (status = 422, description = "The daemon knows no such hunk, or refuses the state"),
        (status = 502, description = "The daemon could not record the decision"),
    )
)]
pub(super) async fn set_state(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetStateRequest>,
) -> Result<Json<ReviewStateResponse>, WebError> {
    let result = state
        .daemon
        .review_set_state(&id, &req.hunk_id, &req.state)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(result, "review.set_state")?))
}

/// `POST /api/session/{id}/review/states`
///
/// The daemon answers the ids it applied and the ids it refused, each with a
/// reason. A refused hunk is part of the answer, not an error status: the
/// client shows which ones and keeps the rest, so this route forwards the
/// object whole.
#[utoipa::path(
    post,
    path = "/api/session/{id}/review/states",
    params(("id" = String, Path, description = "The session under review")),
    request_body = SetStatesRequest,
    responses(
        (status = 200, body = ReviewStatesResponse),
        (status = 422, description = "The daemon refuses the state"),
        (status = 502, description = "The daemon could not record the decisions"),
    )
)]
pub(super) async fn set_states(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SetStatesRequest>,
) -> Result<Json<ReviewStatesResponse>, WebError> {
    let result = state
        .daemon
        .review_set_states(&id, &req.hunk_ids, &req.state)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(result, "review.set_states")?))
}

/// `POST /api/session/{id}/review/undo-reject`
///
/// Takes back the most recent reject, single or bulk, as one action. The
/// daemon owns the stack, so the request names no hunk: the session in the
/// path is the whole input. The `{}` body is load-bearing for the same reason
/// as on `…/rebase` and `…/comment/{id}/resolve`: it carries `Content-Type:
/// application/json` and forces the preflight the CORS allowlist refuses.
#[utoipa::path(
    post,
    path = "/api/session/{id}/review/undo-reject",
    params(("id" = String, Path, description = "The session under review")),
    responses(
        (status = 200, body = ReviewUndoRejectResponse),
        (status = 422, description = "The daemon knows no such hunk any more"),
        (status = 502, description = "The daemon could not restore the rejects"),
    )
)]
pub(super) async fn undo_reject(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ReviewUndoRejectResponse>, WebError> {
    let result = state.daemon.review_undo_reject(&id).await.daemon_err()?;
    Ok(Json(daemon_shape(result, "review.undo_reject")?))
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
