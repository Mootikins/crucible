//! Review RPCs preserve the daemon's result shape.
//!
//! Both are writes, so neither may replay. A lost response to a review write
//! must surface an ambiguous outcome, never a second comment. The real-socket
//! regressions in daemon_retry_tests exercise response loss and reconnection.

use super::daemon::ReconnectingDaemon;
use crucible_daemon::rpc_client::ReviewCommentRequest;

impl ReconnectingDaemon {
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
