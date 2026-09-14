//! Review RPCs preserve the daemon's result shape.
//!
//! Only listing may replay. A lost response to a review write must surface
//! an ambiguous outcome, never a second revert or rejection. The real-socket
//! regressions in daemon_retry_tests exercise response loss and reconnection.

use super::daemon::ReconnectingDaemon;
use crucible_core::session::ReviewScope;
use crucible_daemon::rpc_client::ReviewCommentRequest;

impl ReconnectingDaemon {
    forward_rpc! {
        /// The composed diff plus its comments, forwarded as the daemon shaped it.
        ///
        /// Untyped on purpose: the result object grows keys (`degraded`, `gate`)
        /// on the daemon's schedule, and a struct here would drop every one of
        /// them on the floor until this crate was rebuilt to match. The only
        /// review call idempotent enough to retry.
        ///
        /// `scope` narrows the listing to the current turn when asked; `None`
        /// sends no scope and the daemon answers the whole session.
        Safe ReviewListHunks =>
        review_list_hunks(session_id: &str, scope: Option<ReviewScope>)
        -> serde_json::Value = review_list_hunks(&session_id, scope);
    }

    forward_rpc! {
        /// The release for a degraded root, which no amount of reviewing clears.
        /// A write, and destructive to the queue, so at-most-once like the rest.
        Once ReviewRebase =>
        review_rebase(session_id: &str)
        -> serde_json::Value = review_rebase(&session_id);
    }

    forward_rpc! {
        Once ReviewSetState =>
        review_set_state(session_id: &str, hunk_id: &str, state: &str)
        -> serde_json::Value = review_set_state(&session_id, &hunk_id, &state);
    }

    forward_rpc! {
        /// One decision over several hunks, in the order given. A reject reverts
        /// several files, so a replay after a broken pipe would revert once and
        /// inject the rejection twice: at-most-once, like the single decision.
        Once ReviewSetStates =>
        review_set_states(session_id: &str, hunk_ids: &[String], state: &str)
        -> serde_json::Value = review_set_states(&session_id, &hunk_ids, &state);
    }

    forward_rpc! {
        /// Pop the most recent reject. A replay would pop a second batch the
        /// user never asked to restore: at-most-once.
        Once ReviewUndoReject =>
        review_undo_reject(session_id: &str)
        -> serde_json::Value = review_undo_reject(&session_id);
    }

    forward_rpc! {
        /// The route builds the request, so its `session_id` is the one from the
        /// URL path. The optional fields reach the daemon absent rather than null,
        /// so the daemon's own defaults apply.
        Once ReviewComment =>
        review_comment(request: ReviewCommentRequest)
        -> serde_json::Value = review_comment(request);
    }

    forward_rpc! {
        Once ReviewResolveComment =>
        review_resolve_comment(session_id: &str, comment_id: &str)
        -> serde_json::Value = review_resolve_comment(&session_id, &comment_id);
    }
}
