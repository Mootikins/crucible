//! Review capture: the bracket that turns a tool call into a ledger interval.
//!
//! This module records what a call that ran actually wrote.

use super::super::{is_safe, StreamContext};
use std::sync::Arc;
use tracing::{debug, warn};

/// The tool call that this future runs, for a daemon write nested in it.
///
/// A task-local, so only work that the call itself awaits sees it: a write
/// from another task of the same session is not part of the call.
pub(crate) struct ToolCallScope {
    session: String,
    /// A review bracket of this call is open.
    bracketed: std::cell::Cell<bool>,
    /// The permission gate allowed this call.
    allowed: std::cell::Cell<bool>,
}

tokio::task_local! {
    static CURRENT_CALL: ToolCallScope;
}

/// Run `call`, one tool call of `session`, so that the daemon writes nested in
/// it can find its review bracket and its permission decision.
pub(crate) async fn within_tool_call<F: std::future::Future>(session: &str, call: F) -> F::Output {
    CURRENT_CALL
        .scope(
            ToolCallScope {
                session: session.to_string(),
                bracketed: std::cell::Cell::new(false),
                allowed: std::cell::Cell::new(false),
            },
            call,
        )
        .await
}

fn current(session: &str, read: impl FnOnce(&ToolCallScope) -> bool) -> bool {
    CURRENT_CALL
        .try_with(|scope| scope.session == session && read(scope))
        .unwrap_or(false)
}

/// Whether an open bracket of the current tool call of `session` already
/// records a nested write.
pub(crate) fn captures_session(session: &str) -> bool {
    current(session, |scope| scope.bracketed.get())
}

/// Whether the permission gate allowed the current tool call of `session`.
pub(crate) fn call_allowed_in(session: &str) -> bool {
    current(session, |scope| scope.allowed.get())
}

/// Record that the permission gate allowed the current tool call.
pub(crate) fn mark_call_allowed() {
    let _ = CURRENT_CALL.try_with(|scope| scope.allowed.set(true));
}

/// Attribute `write`, a daemon write for `session` made outside the session's
/// tool calls, to its own interval of the session's review ledger.
///
/// A write nested in a bracketed tool call belongs to that call's interval,
/// so it opens no second bracket: two open brackets on one root make both
/// contested. Capture is best-effort, as for a tool call: a root the daemon
/// cannot diff leaves the write unattributed, and a failed close is logged,
/// because the write itself already happened.
pub(crate) async fn attribute_write<T>(
    review: &Arc<crate::review::ReviewLedgers>,
    session: &str,
    storage: &std::path::Path,
    roots: &[std::path::PathBuf],
    event_tx: &crate::EventBus,
    label: &str,
    write: impl std::future::Future<Output = T>,
) -> T {
    if captures_session(session) {
        return write.await;
    }
    let bracket = match review.open_or_restore(session, storage, roots).await {
        Ok(()) => review.open_bracket(session).await,
        Err(e) => Err(e),
    };
    let bracket = bracket
        .inspect_err(|e| {
            debug!(session_id = %session, error = %e, "review bracket not opened; write left unattributed");
        })
        .ok();
    let answer = write.await;
    if let Some(bracket) = bracket {
        let id = format!("{label}-{}", uuid::Uuid::new_v4());
        if let Err(e) = review.close(session, bracket, &id, 0).await {
            warn!(session_id = %session, error = %e, "review bracket close failed; write left unattributed");
        }
        crate::server::session::review::emit_review_changed(event_tx, session, label);
    }
    answer
}

/// Whether a tool call needs a review capture bracket around it (§5).
///
/// Deliberately the inverse of [`is_safe`] rather than a positive list of
/// writers: an unknown MCP tool must be bracketed, because a missed bracket
/// costs a hunk attributed to nobody, while a needless bracket costs two
/// `write-tree` calls that dedupe to no interval and no card.
///
/// `delegate_session` is the one exclusion. A delegated child keeps its own
/// ledger over the same root, so bracketing the parent's call would overlap
/// every one of the child's brackets and mark them all contested — turning an
/// entire delegation into unattributed `external` change. The parent records a
/// child-ledger reference instead; attribution depth follows session depth.
///
/// The exclusion is only half of it: the child's intervals are then the only
/// attribution the delegated work has, so they are copied into the parent when
/// the child ends — `ReviewLedgers::harvest_and_clear`, off the parent link
/// registered from the child's first turn.
pub(crate) fn needs_review_bracket(tool_name: &str) -> bool {
    !is_safe(tool_name) && tool_name != "delegate_session"
}

/// The child session id a `delegate_session` call reported, read out of the
/// tool's own JSON result.
///
/// The id is not threaded through `DelegationRequest`, and the result is the
/// tool's documented output contract (`tools/mcp_server.rs`), so reading it
/// here links parent to child without a plumbing change across the delegation
/// service. A result we cannot parse yields `None` and the delegation simply
/// goes unlinked — the child's own ledger is unaffected either way.
pub(crate) fn delegated_child_id(tool_result: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(tool_result)
        .ok()?
        .get("child_session_id")?
        .as_str()
        .map(str::to_string)
}

impl StreamContext {
    /// The turn's review ledgers, or `None` when the manager wired none.
    /// They ride on `agent_stream_config` because the review gate needs the
    /// same handle; a second field here would be the same state twice.
    fn review(&self) -> Option<&Arc<crate::review::ReviewLedgers>> {
        self.agent_stream_config.review.as_ref()
    }

    /// Open a review capture bracket for a tool call that could write.
    ///
    /// `None` — meaning "don't attribute this call" — for a read-only tool, a
    /// session with no git-backed root, or a capture that failed. Capture is
    /// best-effort on purpose: a workspace the daemon cannot diff must not
    /// stop the agent from working in it.
    pub(super) async fn open_review_bracket(
        &self,
        tool_name: &str,
    ) -> Option<crate::review::CaptureHandle> {
        if !needs_review_bracket(tool_name) {
            return None;
        }
        let review = self.review()?;
        if !review.is_open(&self.session_id) {
            return None;
        }
        let handle = review
            .open_bracket(&self.session_id)
            .await
            .inspect_err(|e| {
                debug!(
                    session_id = %self.session_id,
                    tool = %tool_name,
                    error = %e,
                    "review capture bracket not opened; call left unattributed"
                );
            })
            .ok();
        // Here, where the bracket opens, so that every path that brackets a
        // call also lets the daemon writes nested in it join the bracket.
        let _ = CURRENT_CALL.try_with(|scope| {
            if scope.session == self.session_id {
                scope.bracketed.set(handle.is_some());
            }
        });
        handle
    }

    /// Move an open bracket's baseline to now.
    ///
    /// Called once the permission prompt has been answered, so that what the
    /// user did to the worktree while deciding is not measured as part of the
    /// call they were deciding about. A no-op when nothing was bracketed.
    pub(super) async fn rebase_review_bracket(
        &self,
        bracket: &mut Option<crate::review::CaptureHandle>,
    ) {
        let Some(handle) = bracket.as_mut() else {
            return;
        };
        // A bracket only exists when `review()` was `Some`, and the config is
        // immutable for the turn.
        let Some(review) = self.review() else { return };
        review.rebase(handle).await;
    }

    /// Close a bracket opened by [`Self::open_review_bracket`].
    ///
    /// Must run on every exit path of the tool call: an unclosed bracket
    /// keeps its root registered and makes every later bracket on that root
    /// look contested forever.
    pub(super) async fn close_review_bracket(
        &self,
        handle: crate::review::CaptureHandle,
        tool_call_id: &str,
    ) {
        // A bracket only exists when `review()` was `Some`, and the config is
        // immutable for the turn.
        let Some(review) = self.review() else { return };
        // Turn-granular by construction: the tree cursor only advances on
        // User and Agent nodes, so every call in a batch shares this id. It
        // is the interval's display coordinate; `tool_call_id` is its
        // identity.
        let node_id = self.conversation_tree.lock().await.current().index();
        if let Err(e) = review
            .close(&self.session_id, handle, tool_call_id, node_id)
            .await
        {
            debug!(
                session_id = %self.session_id,
                tool_call_id = %tool_call_id,
                error = %e,
                "review capture bracket close failed"
            );
        }
    }

    /// Record that a `delegate_session` call produced a child with its own
    /// ledger, so expanding the delegation card can expand into the child's
    /// tool calls.
    pub(super) async fn link_delegation_child(&self, tool_call_id: &str, tool_result: &str) {
        let Some(review) = self.review() else { return };
        let Some(child) = delegated_child_id(tool_result) else {
            return;
        };
        // Same turn coordinate `close_review_bracket` records, and read the
        // same way, so a delegation card and the tool cards around it agree
        // about which turn they belong to.
        let node_id = self.conversation_tree.lock().await.current().index();
        review
            .link_child(&self.session_id, tool_call_id, &child, Some(node_id))
            .await;
    }
}
