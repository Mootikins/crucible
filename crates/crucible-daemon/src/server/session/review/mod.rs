//! The session record: the ledger reads that the `diff.*` handlers and the
//! Lua bridge share.
//!
//! The session record diffset (`diff.get`) lists the files, and the
//! comments belong to the `diff.*` operations. This module keeps the two
//! reads that need a loaded ledger: [`ensure_loaded`], which restores a
//! resumed session from `review.jsonl`, and [`list_hunks`], the attributed
//! hunks that `cru.session.review_list_hunks` answers to the reflection
//! pass. [`emit_review_changed`] tells the clients of a session that its
//! record or its comments changed.

use super::super::*;

use crucible_core::session::ComposedHunk;

use crate::review::{ReviewError, ReviewResult};

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

pub(crate) fn emit_review_changed(event_tx: &crate::EventBus, session_id: &str, reason: &str) {
    if !event_tx.emit(SessionEventMessage::review_changed(session_id, reason)) {
        debug!(session_id, reason, "no subscribers for review_changed");
    }
}

#[cfg(test)]
mod tests;
