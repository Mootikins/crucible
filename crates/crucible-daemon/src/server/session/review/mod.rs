//! The `review.*` RPC surface: the composed-diff review queue.
//!
//! Five operations — list, decide, revert, comment, resolve — over the
//! session-scoped ledger `AgentManager` owns. The engine lives in
//! `crate::review`; this module is the boundary that turns [`ReviewError`]
//! into JSON-RPC codes and decides what a decision *emits*.
//!
//! Two boundary rules are load-bearing and are why the operations are not
//! thin passthroughs:
//!
//! * **Rejection is a conversation event.** An agent that is not told its
//!   edit was reverted re-applies the same edit on the next turn, and you are
//!   in a loop. Every rejection injects a `user`-role note naming the file and
//!   lines. Acceptance is silent — it costs tokens and teaches the model
//!   nothing.
//! * **Every operation that moves the composed diff emits `review_changed`.**
//!   Including acceptance, which is silent to the *agent* but not to the
//!   panel showing the queue.
//!
//! The same functions back the Lua bridge (`cru.sessions.review_*`), so the
//! logic lives in free functions here rather than inside the handlers; §6
//! needs a delegating agent to be able to review a sub-session, and a
//! handler-only implementation would have to be written twice.

use super::super::*;
use crate::rpc_client::{
    ReviewCommentRequest, ReviewResolveCommentRequest, ReviewSetStateRequest, SessionIdRequest,
};
use crate::rpc_helpers::typed_params;

use std::path::{Component, Path};

use crucible_core::session::{
    Comment, CommentAuthor, ComposedHunk, HunkId, LineRange, ReviewState, RootBase, RootStatus,
};

use crate::review::{paths, ReviewError, ReviewResult};

// ── Operations (shared with the Lua bridge) ─────────────────────────────────

/// Make sure the session's ledger is in memory before an operation reads it.
///
/// A review RPC can be the *first* thing that touches a session after a daemon
/// restart — the panel opens on a resumed session long before it sends a
/// message — and without this the queue would come back empty until the user
/// happened to run a turn, which reads as "the agent changed nothing".
///
/// Silent when the session is not in the manager's memory either: there is no
/// kiln to find a journal under, and inventing one would be guessing. The
/// caller's own "no ledger is an empty queue" rule then applies.
pub(crate) async fn ensure_loaded(am: &AgentManager, sm: &SessionManager, session_id: &str) {
    if am.review.is_open(session_id) {
        return;
    }
    let Some(session) = sm.get_session(session_id) else {
        return;
    };
    let path = session
        .storage_path(sm.sessions_root())
        .join("review.jsonl");
    if !path.exists() {
        return;
    }
    // Loud but not fatal: `restore_from_journal` records the loss on the
    // session's `Integrity` before it returns, so the queue comes back
    // degraded — held, with a reason, and releasable by `review.rebase` —
    // rather than as an empty success.
    if let Err(e) = am.review.restore_from_journal(session_id, &path).await {
        warn!(
            session_id,
            path = %path.display(),
            error = %e,
            "review journal could not be restored; this session's writes are held until a rebase"
        );
    }
}

/// The session's composed diff.
///
/// A session that has never run a turn has no ledger, and "nothing to review"
/// is the honest answer for it rather than an error — the queue is empty, not
/// broken.
pub(crate) async fn list_hunks(
    am: &AgentManager,
    session_id: &str,
) -> ReviewResult<Vec<ComposedHunk>> {
    match am.review.list_hunks(session_id).await {
        Err(ReviewError::NoLedger(_)) => Ok(Vec::new()),
        other => other,
    }
}

/// The composed diff plus the roots the ledger cannot vouch for.
///
/// The statuses are not decoration: a degraded root contributes no hunks, so a
/// client shown only the hunks would read a broken ledger as a clean queue —
/// while the gate is holding every write under that root.
pub(crate) async fn list_hunks_with_status(
    am: &AgentManager,
    session_id: &str,
) -> ReviewResult<(Vec<ComposedHunk>, Vec<RootStatus>)> {
    match am.review.list_hunks_with_status(session_id).await {
        Err(ReviewError::NoLedger(_)) => Ok((Vec::new(), Vec::new())),
        other => other,
    }
}

/// Move the session's base to the worktree as it is now.
///
/// The release for a block no amount of reviewing can clear — a gc'd base
/// tree, a moved root, a journal record that would not parse. Destructive to
/// the queue by definition, which is why it is a sixth explicit method and
/// never something the daemon does on its own.
pub(crate) async fn rebase(
    am: &AgentManager,
    sm: &SessionManager,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
) -> ReviewResult<Vec<RootStatus>> {
    let session = sm
        .get_session(session_id)
        .ok_or_else(|| ReviewError::NoLedger(session_id.to_string()))?;
    // The same root set the send path opens the ledger over, re-derived rather
    // than read from the ledger: the case this exists to recover from includes
    // a ledger that lost its base records and so has no roots left to name.
    let mut roots: Vec<PathBuf> = session.workspace.iter().cloned().collect();
    roots.extend(sm.kiln_paths(&session.kilns));

    // The storage directory rather than the journal path: a session that has
    // never run a turn has no journal registered, and a rebase whose records
    // reach no file would answer `Ok` while persisting nothing and claiming no
    // keep ref for the base it just captured.
    let statuses = am
        .review
        .rebase_session(
            session_id,
            &session.storage_path(sm.sessions_root()),
            &roots,
        )
        .await?;
    emit_review_changed(event_tx, session_id, "rebased");
    Ok(statuses)
}

/// Record a decision about a hunk.
///
/// [`ReviewState::Rejected`] routes through [`reject_hunk`] so a recorded
/// rejection always corresponds to a revert that actually happened and to a
/// note the agent will read.
pub(crate) async fn set_state(
    am: &AgentManager,
    sm: &SessionManager,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    hunk_id: &HunkId,
    state: ReviewState,
) -> ReviewResult<()> {
    if state == ReviewState::Rejected {
        return reject_hunk(am, sm, event_tx, session_id, hunk_id).await;
    }
    am.review.set_state(session_id, hunk_id, state).await?;
    emit_review_changed(event_tx, session_id, state_reason(state));
    Ok(())
}

/// Revert a hunk against `session_base`, mark it rejected, and tell the agent.
///
/// The hunk is read *before* the revert: afterwards its identity is gone from
/// the composed diff, and the note has to name the lines the user was looking
/// at, not whatever occupies them now.
pub(crate) async fn reject_hunk(
    am: &AgentManager,
    sm: &SessionManager,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    hunk_id: &HunkId,
) -> ReviewResult<()> {
    let hunk = am
        .review
        .list_hunks(session_id)
        .await?
        .into_iter()
        .find(|h| &h.id == hunk_id)
        .ok_or_else(|| ReviewError::UnknownHunk(hunk_id.clone()))?;

    am.review.revert_hunk(session_id, hunk_id).await?;

    // The revert already landed on disk. A failure to inject the note leaves
    // the worktree correct and the agent ignorant, which is the loop this
    // exists to prevent — loud, but never a failed RPC.
    if let Err(e) =
        super::inject_context_impl(sm, event_tx, session_id, "user", &rejection_notice(&hunk)).await
    {
        warn!(
            session_id,
            hunk = %hunk_id,
            error = %e,
            "hunk reverted but the rejection was not injected; the agent may re-apply it"
        );
    }
    emit_review_changed(event_tx, session_id, "rejected");
    Ok(())
}

/// Anchor a comment to a line range.
///
/// `path` may be absolute or relative to one of the session's tracked roots;
/// `root` disambiguates when a relative path is ambiguous. Returns the stored
/// comment so the caller learns the minted id.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn add_comment(
    am: &AgentManager,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    root: Option<&Path>,
    path: &Path,
    line_range: LineRange,
    body: &str,
    author: CommentAuthor,
) -> ReviewResult<Comment> {
    let ledger = am
        .review
        .ledger(session_id)
        .ok_or_else(|| ReviewError::NoLedger(session_id.to_string()))?;

    let (base, relative) = resolve_root(ledger.session_base(), root, path)?;

    let comment = Comment::new(
        base.root.clone(),
        relative,
        base.base_tree.clone(),
        line_range,
        body,
        author,
    );
    am.review.add_comment(session_id, comment.clone()).await;
    emit_review_changed(event_tx, session_id, "commented");
    Ok(comment)
}

pub(crate) async fn resolve_comment(
    am: &AgentManager,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    comment_id: &str,
) -> ReviewResult<()> {
    am.review.resolve_comment(session_id, comment_id).await?;
    emit_review_changed(event_tx, session_id, "comment_resolved");
    Ok(())
}

// ── Handlers ────────────────────────────────────────────────────────────────

/// `review.list_hunks` — the composed diff plus its comments.
///
/// Comments ride along rather than getting a sixth method: the panel needs
/// both to draw one row, and two round-trips over a worktree that moves under
/// them can disagree.
pub(crate) async fn handle_review_list_hunks(
    req: Request,
    am: &Arc<AgentManager>,
    sm: &Arc<SessionManager>,
) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;
    ensure_loaded(am, sm, session_id).await;

    match list_hunks_with_status(am, session_id).await {
        Ok((hunks, roots)) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "hunks": hunks,
                "comments": am.review.comments(session_id),
                // Only the roots that are actually broken. An empty array is
                // the common case and the one a client should not have to
                // filter for; a non-empty one means the gate is holding writes
                // that no amount of reviewing will release.
                "degraded": roots.iter().filter(|r| r.is_degraded()).collect::<Vec<_>>(),
                // What the journal could not be read back as. Separate from
                // `degraded` because the worst losses are exactly the ones
                // with no root to attach to: a journal that will not read at
                // all, or one whose base records are gone, leaves nothing that
                // can name a repository — so `degraded` comes back empty while
                // the gate holds every write in the session. Reported as
                // success with an empty queue, that is the data loss the
                // journal exists to prevent.
                "integrity": am.review.integrity(session_id),
                // What this session's turn is parked on, or `null`. The
                // `review_gate` event is the live signal, but events are
                // dropped rather than replayed across a reconnect — so a
                // client that opens or refreshes while a turn is already
                // blocked would otherwise show nothing and the agent would
                // read as hung. Always present, so a client can tell "not
                // blocked" from "this daemon does not report it".
                "gate": am.review.gate_block(session_id),
            }),
        ),
        Err(e) => review_error_to_response(req.id, e),
    }
}

/// `review.rebase` — accept the worktree as the new base and release the queue.
pub(crate) async fn handle_review_rebase(
    req: Request,
    am: &Arc<AgentManager>,
    sm: &Arc<SessionManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<SessionIdRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;
    ensure_loaded(am, sm, session_id).await;

    match rebase(am, sm, event_tx, session_id).await {
        Ok(roots) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "roots": roots,
            }),
        ),
        Err(e) => review_error_to_response(req.id, e),
    }
}

/// `review.set_state` — accept, reject, or return a hunk to the queue.
pub(crate) async fn handle_review_set_state(
    req: Request,
    am: &Arc<AgentManager>,
    sm: &Arc<SessionManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<ReviewSetStateRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;
    let state_str = &params.state;
    ensure_loaded(am, sm, session_id).await;

    let Some(state) = parse_state(state_str) else {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Invalid 'state': {state_str} (expected unreviewed, accepted or rejected)"),
        );
    };
    let hunk_id = HunkId::from(params.hunk_id.clone());

    match set_state(am, sm, event_tx, session_id, &hunk_id, state).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "hunk_id": hunk_id,
                "state": state,
            }),
        ),
        Err(e) => review_error_to_response(req.id, e),
    }
}

/// `review.comment` — anchor a comment to `line_start..line_end`.
pub(crate) async fn handle_review_comment(
    req: Request,
    am: &Arc<AgentManager>,
    sm: &Arc<SessionManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<ReviewCommentRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let session_id = &params.session_id;
    ensure_loaded(am, sm, session_id).await;
    let (path, body, line_start) = (&params.path, &params.body, params.line_start);
    // Half-open, so a one-line comment is `line_start .. line_start + 1`.
    // Defaulting to that is what a client anchoring to a single line means.
    let line_end = params.line_end.unwrap_or(line_start + 1);
    let root = params.root.as_deref().map(Path::new);
    let author_str = params.author.as_deref().unwrap_or("human");

    let Some(author) = parse_author(author_str) else {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Invalid 'author': {author_str} (expected human or agent)"),
        );
    };

    match add_comment(
        am,
        event_tx,
        session_id,
        root,
        Path::new(path),
        LineRange::new(line_start, line_end),
        body,
        author,
    )
    .await
    {
        Ok(comment) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "comment": comment,
            }),
        ),
        Err(e) => review_error_to_response(req.id, e),
    }
}

/// `review.resolve_comment` — mark a comment answered.
pub(crate) async fn handle_review_resolve_comment(
    req: Request,
    am: &Arc<AgentManager>,
    sm: &Arc<SessionManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
) -> Response {
    let params = match typed_params::<ReviewResolveCommentRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };
    let (session_id, comment_id) = (&params.session_id, &params.comment_id);
    ensure_loaded(am, sm, session_id).await;

    match resolve_comment(am, event_tx, session_id, comment_id).await {
        Ok(()) => Response::success(
            req.id,
            serde_json::json!({
                "session_id": session_id,
                "comment_id": comment_id,
                "resolved": true,
            }),
        ),
        Err(e) => review_error_to_response(req.id, e),
    }
}

// ── Boundary helpers ────────────────────────────────────────────────────────

/// Map a [`ReviewError`] to a JSON-RPC code.
///
/// Mapped variant by variant rather than through a catch-all: every variant
/// here except `Git`/`Io` is something the *caller* did or a race the caller
/// can retry out of, and answering INTERNAL_ERROR to a stale client tells it
/// the daemon is broken when the correct action is to re-list.
fn review_error_to_response(req_id: Option<RequestId>, err: ReviewError) -> Response {
    match err {
        ReviewError::NoLedger(_)
        | ReviewError::NoTrackableRoots(_)
        | ReviewError::UnknownHunk(_)
        | ReviewError::Stale { .. }
        | ReviewError::ExternalHunk(_)
        | ReviewError::UnknownComment(_)
        | ReviewError::NotAGitRepo { .. }
        | ReviewError::AmbiguousPath { .. }
        | ReviewError::PathEscapesRoot { .. } => {
            Response::error(req_id, INVALID_PARAMS, err.to_string())
        }
        // A journal the daemon cannot read is a daemon-side fault, and the
        // caller must not read it as "nothing to review": that is the data
        // loss the journal exists to prevent, reported as success.
        e @ (ReviewError::Git(_) | ReviewError::Io(_) | ReviewError::Journal { .. }) => {
            internal_error(req_id, e)
        }
    }
}

fn emit_review_changed(
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    reason: &str,
) {
    if !emit_event(
        event_tx,
        SessionEventMessage::review_changed(session_id, reason),
    ) {
        debug!(session_id, reason, "no subscribers for review_changed");
    }
}

fn state_reason(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Unreviewed => "unreviewed",
        ReviewState::Accepted => "accepted",
        ReviewState::Rejected => "rejected",
    }
}

fn parse_state(raw: &str) -> Option<ReviewState> {
    match raw {
        "unreviewed" => Some(ReviewState::Unreviewed),
        "accepted" => Some(ReviewState::Accepted),
        "rejected" => Some(ReviewState::Rejected),
        _ => None,
    }
}

fn parse_author(raw: &str) -> Option<CommentAuthor> {
    match raw {
        "human" => Some(CommentAuthor::Human),
        "agent" => Some(CommentAuthor::Agent),
        _ => None,
    }
}

/// What the agent is told when one of its edits is rejected.
///
/// Names the file and the lines *as the user saw them* and says the revert
/// already happened, so the model treats it as a fact about the worktree
/// rather than as an instruction it may decline.
fn rejection_notice(hunk: &ComposedHunk) -> String {
    format!(
        "user rejected the edit to {}; reverted.",
        hunk_location(hunk)
    )
}

fn hunk_location(hunk: &ComposedHunk) -> String {
    let range = hunk.current_range;
    // A pure deletion has an empty current range: there are no lines left to
    // point at, so name the seam it was removed from.
    if range.len() <= 1 {
        format!("{}:{}", hunk.path, range.start)
    } else {
        format!("{}:{}-{}", hunk.path, range.start, range.end - 1)
    }
}

/// Resolve a client-supplied path to `(repository root, root-relative path)`.
///
/// An absolute path picks the longest matching root, so a kiln nested inside
/// the workspace repo resolves to the kiln rather than to its container. A
/// relative path is only unambiguous when there is exactly one candidate root;
/// with several, `None` asks the caller to be explicit instead of guessing and
/// anchoring a comment in the wrong repository.
///
/// Every path here — the query, the requested root, the tracked roots — goes
/// through [`paths::resolve`] first. Roots are stored as `git rev-parse
/// --show-toplevel` printed them, physical, while a client naturally names the
/// workspace as it was registered; a workspace reached through a symlink is
/// then two spellings of one directory, and comparing them raw resolves
/// nothing.
///
/// **Fail-closed.** A result that is not *inside* the root it matched is
/// refused rather than stored: the relative path ends up on a [`Comment`],
/// which the editor-opening path later joins back onto the root, so a `..` in
/// it is stored-path confusion — and network-reachable once the web bridge
/// lands. Refusing costs one loud `NotAGitRepo` on one RPC that the caller can
/// retry with an explicit root, which is why the gate's opposite rule does not
/// apply here: the gate refusing hangs a turn no human can release.
///
/// Returns the `RootBase` rather than its path: the caller needs the base tree
/// too, and looking it up again forced an error for a branch the code itself
/// documented as unreachable.
fn resolve_root<'a>(
    bases: &'a [RootBase],
    root: Option<&Path>,
    path: &Path,
) -> ReviewResult<(&'a RootBase, String)> {
    let candidates: Vec<&RootBase> = match root {
        Some(requested) => {
            let resolved = paths::resolve(requested);
            vec![bases
                .iter()
                .find(|b| paths::resolve(&b.root) == resolved)
                .ok_or_else(|| ReviewError::NotAGitRepo {
                    path: requested.to_path_buf(),
                })?]
        }
        None => bases.iter().collect(),
    };

    let absolute = if path.is_absolute() {
        paths::resolve(path)
    } else {
        match candidates.as_slice() {
            [only] => paths::resolve(&only.root.join(path)),
            // Naming one of several roots for the caller would be a guess about
            // which file they meant.
            _ => {
                return Err(ReviewError::AmbiguousPath {
                    path: path.to_path_buf(),
                    roots: candidates.len(),
                })
            }
        }
    };

    candidates
        .iter()
        .filter_map(|b| {
            let relative = absolute.strip_prefix(paths::resolve(&b.root)).ok()?;
            // A `..` whose whole prefix is missing survives normalisation —
            // nothing can resolve it — and `strip_prefix` is component-wise, so
            // it would come back out as a relative path that still escapes.
            relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
                .then(|| (*b, relative.to_string_lossy().into_owned()))
        })
        .max_by_key(|(base, _)| base.root.as_os_str().len())
        .ok_or_else(|| ReviewError::PathEscapesRoot {
            path: path.to_path_buf(),
        })
}

#[cfg(test)]
mod tests;
