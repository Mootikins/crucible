//! Review RPC methods — the composed diff and its comments.
//!
//! Two deliberate shapes here, both of which look like laziness and are not.
//!
//! **Responses are `serde_json::Value`.** The review result objects are grown
//! by several concurrent lanes (`degraded` and `integrity` from persistence)
//! and every one of them would otherwise land as a breaking edit to a
//! struct in this file. A passthrough forwards a key this client has never
//! heard of to a browser that has, which is the same rationale already written
//! for `session.list_modes`' sibling routes; the shape is pinned by the web
//! layer's contract tests instead of by a type nobody reads.
//!
//! **Writes go through [`DaemonClient::call`], never `call_with_retry`.**
//! `call_with_retry` retries on *timeout*, which is precisely the case where
//! the daemon may already have executed. A retried `review.comment` stores
//! the comment twice. At-most-once is the only correct semantics for a write;
//! only `review.list_hunks` is idempotent enough to retry.

use anyhow::Result;
use serde_json::Value;

use super::DaemonClient;
use crucible_core::session::ReviewScope;

/// Request for `review.list_hunks`.
///
/// `scope` is the daemon's own type, shared through `crucible-core`, so the
/// web route, this client and the handler spell the two words from one
/// definition. Absent means [`ReviewScope::Session`], which is what every
/// client written before scopes existed sends.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewListHunksRequest {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ReviewScope>,
}

/// Request for `review.comment`.
///
/// This is what the handler reads, and what [`DaemonClient::review_comment`]
/// sends, so the field names have one home. The Lua bridge reads its spec
/// into this struct too.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewCommentRequest {
    pub session_id: String,
    pub path: String,
    pub body: String,
    pub line_start: u32,
    /// Half-open, so a one-line comment is `line_start .. line_start + 1`.
    /// Defaulting to that is what anchoring to a single line means.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// `human` (the default here) or `agent`. The Lua bridge defaults the
    /// other way — its caller is an agent, this one is a person at a panel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

/// Request for `review.resolve_comment`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewResolveCommentRequest {
    pub session_id: String,
    pub comment_id: String,
}

impl DaemonClient {
    /// One attempt, no retry. See the module doc for why the review writes
    /// must not ride `call_with_retry`.
    async fn call_once(&self, method: &str, params: Value) -> Result<Value> {
        self.call(method, params).await
    }

    /// `review.list_hunks` — the composed diff, its comments, and whatever
    /// status keys the daemon has grown since.
    ///
    /// The one review method that is safe to retry: it mutates nothing.
    ///
    /// `None` for `scope` sends no scope, which the daemon reads as the whole
    /// session.
    pub async fn review_list_hunks(
        &self,
        session_id: &str,
        scope: Option<ReviewScope>,
    ) -> Result<Value> {
        self.call_with_retry(
            "review.list_hunks",
            serde_json::to_value(ReviewListHunksRequest {
                session_id: session_id.to_string(),
                scope,
            })?,
        )
        .await
    }

    /// `review.comment` — anchor a comment to a line range.
    ///
    /// The caller names the session in `request.session_id`. The web route
    /// fills it from the URL path, so a `session_id` in a request body never
    /// reaches here.
    pub async fn review_comment(&self, request: ReviewCommentRequest) -> Result<Value> {
        self.call_once("review.comment", serde_json::to_value(request)?)
            .await
    }

    /// `review.resolve_comment` — mark a comment answered.
    pub async fn review_resolve_comment(
        &self,
        session_id: &str,
        comment_id: &str,
    ) -> Result<Value> {
        self.call_once(
            "review.resolve_comment",
            serde_json::to_value(ReviewResolveCommentRequest {
                session_id: session_id.to_string(),
                comment_id: comment_id.to_string(),
            })?,
        )
        .await
    }
}
