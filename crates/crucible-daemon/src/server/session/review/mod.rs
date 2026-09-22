//! The `review.*` RPC surface: the comment aliases of a session record.
//!
//! Two operations — comment, resolve — over the session-scoped ledger
//! `AgentManager` owns. The session record diffset (`diff.get`) replaces the
//! old hunk listing. The engine lives in `crate::review`; this
//! module is the boundary that turns [`ReviewError`] into JSON-RPC codes.
//! Every operation that changes what the panel shows emits `review_changed`.
//!
//! The same functions back the Lua bridge (`cru.session.review_*`), which
//! also reads the attributed hunks through [`list_hunks`], so the
//! logic lives in free functions here rather than inside the handlers; §6
//! needs a delegating agent to be able to review a sub-session, and a
//! handler-only implementation would have to be written twice.

use super::super::*;
use crate::rpc_client::{ReviewCommentRequest, ReviewResolveCommentRequest};
use crate::rpc_helpers::typed_params;

use std::path::Path;

use crucible_core::session::{
    Comment, CommentAuthor, CommentSide, ComposedHunk, LineRange, RootBase,
};

use crate::review::{paths, record_diffset, ReviewError, ReviewResult};
use crate::server::diff_comments::{record_comment, resolve_in, CommentSpec};
use crate::tools::containment::reject_non_normal;
use crucible_core::session::SessionId;

// ── Operations (shared with the Lua bridge) ─────────────────────────────────

/// Make sure the session's ledger is in memory before an operation reads it.
///
/// A review RPC can be the *first* thing that touches a session after a daemon
/// restart — the panel opens on a resumed session long before it sends a
/// message — and without this the record would come back empty until the user
/// happened to run a turn, which reads as "the agent changed nothing".
///
/// Silent when the session is not in the manager's memory either: there is no
/// kiln to find a journal under, and inventing one would be guessing. The
/// caller's own "no ledger is an empty record" rule then applies.
pub(crate) async fn ensure_loaded(am: &AgentManager, sm: &SessionManager, session_id: &str) {
    ensure_record_loaded(&am.review, sm, session_id).await;
}

/// [`ensure_loaded`] for a caller that holds the ledgers and not the agent
/// manager: the `diff.*` handlers of the session record.
pub(crate) async fn ensure_record_loaded(
    review: &crate::review::ReviewLedgers,
    sm: &SessionManager,
    session_id: &str,
) {
    if review.is_open(session_id) {
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
    // session's `Integrity` before it returns, so the record comes back
    // degraded, with a reason, rather than as an empty success.
    if let Err(e) = review.restore_from_journal(session_id, &path).await {
        warn!(
            session_id,
            path = %path.display(),
            error = %e,
            "review journal could not be restored; this session's roots are degraded"
        );
    }
}

/// The session's composed diff, attributed to its tool calls.
///
/// A session that has never run a turn has no ledger, and "no changes" is the
/// honest answer for it rather than an error — the record is empty, not
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
    let session =
        SessionId::parse(session_id).map_err(|e| ReviewError::InvalidSession(e.to_string()))?;
    // The alias of `diff.comment`: the range counts lines on the current
    // side of the session record, which is the file on disk.
    let spec = CommentSpec {
        path: &relative,
        side: CommentSide::Current,
        range: line_range,
        body,
        author,
    };
    record_comment(&am.review, event_tx, &session, &base.root, &spec).await
}

/// Mark a comment of the session record resolved: the alias of
/// `diff.resolve_comment`.
///
/// A session with no ledger answers [`ReviewError::NoLedger`], as the
/// `review.*` methods did before the comments moved to the diffset store.
pub(crate) async fn resolve_comment(
    am: &AgentManager,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    session_id: &str,
    comment_id: &str,
) -> ReviewResult<()> {
    if am.review.ledger(session_id).is_none() {
        return Err(ReviewError::NoLedger(session_id.to_string()));
    }
    let diffset = record_diffset(session_id)?;
    let session =
        SessionId::parse(session_id).map_err(|e| ReviewError::InvalidSession(e.to_string()))?;
    resolve_in(&am.review, event_tx, &diffset, Some(&session), comment_id)
}

// ── Handlers ────────────────────────────────────────────────────────────────

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

    let Some(author) = parse_wire::<CommentAuthor>(author_str) else {
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
/// the daemon is broken when the correct action is to read again.
fn review_error_to_response(req_id: Option<RequestId>, err: ReviewError) -> Response {
    match err {
        ReviewError::NoLedger(_)
        | ReviewError::NoTrackableRoots(_)
        | ReviewError::UnknownComment(_)
        | ReviewError::InvalidSession(_)
        | ReviewError::InvalidComment(_)
        | ReviewError::NotAGitRepo { .. }
        | ReviewError::AmbiguousPath { .. }
        | ReviewError::PathEscapesRoot { .. } => {
            Response::error(req_id, INVALID_PARAMS, err.to_string())
        }
        // A journal the daemon cannot read is a daemon-side fault, and the
        // caller must not read it as "no changes": that is the data loss the
        // journal exists to prevent, reported as success.
        // `WrongBackend` joins them: a snapshot read through the wrong seam
        // is the daemon's routing fault, and the caller can do nothing with
        // an INVALID_PARAMS about a request it made correctly.
        e @ (ReviewError::Git(_)
        | ReviewError::Io(_)
        | ReviewError::Journal { .. }
        | ReviewError::WrongBackend { .. }) => internal_error(req_id, e),
    }
}

pub(crate) fn emit_review_changed(
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

/// Parse a wire string through the type's own serde derive.
fn parse_wire<T: serde::de::DeserializeOwned>(raw: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(raw.to_owned())).ok()
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
/// retry with an explicit root.
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
            reject_non_normal(relative)
                .is_ok()
                .then(|| (*b, relative.to_string_lossy().into_owned()))
        })
        .max_by_key(|(base, _)| base.root.as_os_str().len())
        .ok_or_else(|| ReviewError::PathEscapesRoot {
            path: path.to_path_buf(),
        })
}

#[cfg(test)]
mod tests;
