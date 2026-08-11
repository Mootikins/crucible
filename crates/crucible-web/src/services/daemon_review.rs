//! `review.*` RPCs, forwarded to the daemon.
//!
//! Split from `daemon.rs` for the 1500-line file budget, along the same seam
//! as `daemon_plugins`: these five calls serve the attributed-diff review
//! surface and none of them interpret what the daemon answered — the results
//! are `serde_json::Value` the whole way to the browser so a key the daemon
//! grows reaches a frontend that reads it without a rebuild here.
//!
//! # The four writes take `call_once`, and no test can prove it
//!
//! `call_with_reconnect` retries when `is_connection_error` matches — broken
//! pipe, connection reset — which are exactly the states in which the daemon
//! may already have executed the call. A replayed `review.revert_hunk` reverts
//! once and injects the rejection into the conversation *twice*, or answers
//! `UnknownHunk` for a revert that succeeded.
//!
//! A contract test used to claim it pinned this. It could not: the mock
//! answered with a JSON-RPC error reading "Request timeout", which matches no
//! connection-error pattern, so `call_with_reconnect` would have passed it just
//! as happily. Making it discriminate needs a connection-shaped failure *and* a
//! mock that survives `reconnect_if_stale` calling `connect_or_start_with_events`,
//! which the mock-socket harness cannot do. It was deleted rather than left as a
//! green light with no bulb. Only `review_list_hunks` may retry.

use super::daemon::ReconnectingDaemon;

impl ReconnectingDaemon {
    /// The composed diff plus its comments, forwarded as the daemon shaped it.
    ///
    /// Untyped on purpose: the result object grows keys (`degraded`, `gate`)
    /// on the daemon's schedule, and a struct here would drop every one of
    /// them on the floor until this crate was rebuilt to match. The only
    /// review call idempotent enough to retry.
    pub async fn review_list_hunks(&self, session_id: &str) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("review.list_hunks", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.review_list_hunks(&session_id).await })
        })
        .await
    }

    /// The release for a degraded root, which no amount of reviewing clears.
    /// A write, and destructive to the queue, so at-most-once like the rest.
    pub async fn review_rebase(&self, session_id: &str) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        self.call_once(move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.review_rebase(&session_id).await })
        })
        .await
    }

    pub async fn review_set_state(
        &self,
        session_id: &str,
        hunk_id: &str,
        state: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        let hunk_id = hunk_id.to_string();
        let state = state.to_string();
        self.call_once(move |daemon| {
            let session_id = session_id.clone();
            let hunk_id = hunk_id.clone();
            let state = state.clone();
            Box::pin(async move { daemon.review_set_state(&session_id, &hunk_id, &state).await })
        })
        .await
    }

    /// `comment` is a params object the route already validated; its optional
    /// fields must reach the daemon absent rather than null so its own
    /// defaults apply.
    pub async fn review_comment(
        &self,
        session_id: &str,
        comment: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        self.call_once(move |daemon| {
            let session_id = session_id.clone();
            let comment = comment.clone();
            Box::pin(async move { daemon.review_comment(&session_id, comment).await })
        })
        .await
    }

    pub async fn review_resolve_comment(
        &self,
        session_id: &str,
        comment_id: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let session_id = session_id.to_string();
        let comment_id = comment_id.to_string();
        self.call_once(move |daemon| {
            let session_id = session_id.clone();
            let comment_id = comment_id.clone();
            Box::pin(async move {
                daemon
                    .review_resolve_comment(&session_id, &comment_id)
                    .await
            })
        })
        .await
    }
}
