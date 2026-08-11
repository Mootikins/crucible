//! Review RPC methods — the composed diff, its states, and its comments.
//!
//! Two deliberate shapes here, both of which look like laziness and are not.
//!
//! **Responses are `serde_json::Value`.** The review result objects are grown
//! by several concurrent lanes (`degraded` from persistence, `gate` from the
//! gate) and every one of them would otherwise land as a breaking edit to a
//! struct in this file. A passthrough forwards a key this client has never
//! heard of to a browser that has, which is the same rationale already written
//! for `session.list_modes`' sibling routes; the shape is pinned by the web
//! layer's contract tests instead of by a type nobody reads.
//!
//! **Writes go through [`DaemonClient::call`], never `call_with_retry`.**
//! `call_with_retry` retries on *timeout*, which is precisely the case where
//! the daemon may already have executed. A retried `review.revert_hunk`
//! reverts once and injects the rejection note into the conversation twice, or
//! answers `UnknownHunk` for a revert that in fact succeeded. At-most-once is
//! the only correct semantics for a write that mutates the worktree and the
//! transcript; only `review.list_hunks` is idempotent enough to retry.

use anyhow::Result;
use serde_json::{json, Value};

use super::DaemonClient;

impl DaemonClient {
    /// One attempt, no retry. See the module doc for why the four review
    /// writes must not ride `call_with_retry`.
    async fn call_once(&self, method: &str, params: Value) -> Result<Value> {
        self.call(method, params).await
    }

    /// `review.list_hunks` — the composed diff, its comments, and whatever
    /// status keys the daemon has grown since.
    ///
    /// The one review method that is safe to retry: it mutates nothing.
    pub async fn review_list_hunks(&self, session_id: &str) -> Result<Value> {
        self.call_with_retry("review.list_hunks", json!({ "session_id": session_id }))
            .await
    }

    /// `review.rebase` — accept the worktree as the new base and release the
    /// queue.
    ///
    /// The one release for a block no amount of reviewing can clear: a degraded
    /// root contributes no hunks, so there is nothing to accept. Without a
    /// client for it the fail-closed half of the design has no shipped escape
    /// and the only workaround is turning review gating off, which is what the
    /// fail-closed design exists to prevent.
    ///
    /// A write, and destructive to the queue — everything the old base
    /// described stops being in the composed diff — so it takes the
    /// at-most-once path with the other four.
    pub async fn review_rebase(&self, session_id: &str) -> Result<Value> {
        self.call_once("review.rebase", json!({ "session_id": session_id }))
            .await
    }

    /// `review.set_state` — accept, reject, or return a hunk to the queue.
    ///
    /// `state` is forwarded as the caller spelled it; the daemon owns the
    /// vocabulary and answers `INVALID_PARAMS` for anything outside it, which
    /// is a better error than one this client invents from a stale copy.
    pub async fn review_set_state(
        &self,
        session_id: &str,
        hunk_id: &str,
        state: &str,
    ) -> Result<Value> {
        self.call_once(
            "review.set_state",
            json!({ "session_id": session_id, "hunk_id": hunk_id, "state": state }),
        )
        .await
    }

    /// `review.comment` — anchor a comment to a line range.
    ///
    /// `comment` is a params object the caller already validated and
    /// serialized, because its optional fields (`line_end`, `root`, `author`)
    /// must be *absent* rather than `null` for the daemon's `optional_param!`
    /// defaults to apply — a discipline that belongs on the struct that
    /// deserialized the request, not on a second copy of it here.
    ///
    /// `session_id` is written last and therefore wins: the session under
    /// review is the one the caller named out of band, never one smuggled in
    /// the payload.
    pub async fn review_comment(&self, session_id: &str, comment: Value) -> Result<Value> {
        let mut params = comment;
        match params.as_object_mut() {
            Some(object) => {
                object.insert("session_id".to_string(), json!(session_id));
            }
            None => params = json!({ "session_id": session_id }),
        }
        self.call_once("review.comment", params).await
    }

    /// `review.resolve_comment` — mark a comment answered.
    pub async fn review_resolve_comment(
        &self,
        session_id: &str,
        comment_id: &str,
    ) -> Result<Value> {
        self.call_once(
            "review.resolve_comment",
            json!({ "session_id": session_id, "comment_id": comment_id }),
        )
        .await
    }
}
