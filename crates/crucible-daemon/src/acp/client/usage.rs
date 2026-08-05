//! Extract token usage and context-window size from raw ACP JSON.
//!
//! The ACP spec defines a `usage` field on `PromptResponse` and a
//! `usage_update` session update (see the `unstable_session_usage` feature in
//! `agent-client-protocol-schema`), but both are gated behind an unstable
//! feature flag in the upstream Rust types. Rather than coupling to that flag,
//! we extract the fields directly from the deserialized JSON so we capture the
//! data whether or not the upstream crate exposes them as typed items.
//!
//! For `usage_update` this is not merely convenient, it is load-bearing.
//! `SessionUpdate` is an internally tagged enum, so with the feature off the
//! `UsageUpdate` variant does not exist and `sessionUpdate: "usage_update"` is
//! an *unknown variant*: the whole `SessionNotification` fails to deserialize
//! and the frame dies at the `Failed to parse SessionNotification` warning in
//! `streaming.rs`, before any `match` on the update runs. Enabling the feature
//! would only move the problem — the next unstable update type an agent sends
//! would kill its notification the same way.
//!
//! Wire shape (Claude Code 2.1.114, captured 2026-04-19):
//!
//! ```json
//! {
//!   "stopReason": "end_turn",
//!   "usage": {
//!     "inputTokens": 3,
//!     "outputTokens": 7,
//!     "cachedReadTokens": 0,
//!     "cachedWriteTokens": 22696,
//!     "totalTokens": 22706
//!   }
//! }
//! ```
//!
//! Field names match the ACP unstable spec: camelCase, all u64 except
//! cached/thought tokens which are optional.

use crucible_core::traits::llm::TokenUsage;
use serde_json::Value;

/// Pull a token-usage record out of a parsed JSON-RPC response result.
///
/// Looks for a `usage` object on `result` (or on `result.update` for
/// notification-shaped payloads). Returns `None` if absent or malformed.
///
/// Tolerates missing fields by zero-filling — agents may report only some of
/// the categories. If neither input nor output tokens are present, returns
/// `None` (treats it as "no usage data" rather than "all zeros").
pub fn extract_usage(result: &Value) -> Option<TokenUsage> {
    // Look for `usage` directly on result or one level deeper. Agents differ
    // in where they place it; we accept either.
    let usage = result
        .get("usage")
        .or_else(|| result.get("update").and_then(|u| u.get("usage")))?;

    let input = usage
        .get("inputTokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(Value::as_u64);
    let output = usage
        .get("outputTokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(Value::as_u64);

    if input.is_none() && output.is_none() {
        return None;
    }

    let cached_read = usage
        .get("cachedReadTokens")
        .or_else(|| usage.get("cached_read_tokens"))
        .and_then(Value::as_u64);
    let cached_write = usage
        .get("cachedWriteTokens")
        .or_else(|| usage.get("cached_write_tokens"))
        .and_then(Value::as_u64);
    let total = usage
        .get("totalTokens")
        .or_else(|| usage.get("total_tokens"))
        .and_then(Value::as_u64);

    let prompt_tokens = saturating_u32(input.unwrap_or(0));
    let completion_tokens = saturating_u32(output.unwrap_or(0));
    let total_tokens =
        saturating_u32(total.unwrap_or_else(|| input.unwrap_or(0) + output.unwrap_or(0)));

    Some(TokenUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        cache_read_tokens: cached_read.map(saturating_u32),
        cache_creation_tokens: cached_write.map(saturating_u32),
    })
}

fn saturating_u32(v: u64) -> u32 {
    v.min(u32::MAX as u64) as u32
}

/// Pull `(used, size)` out of a `session/update` params payload, if it is a
/// `usage_update`.
///
/// Wire shape (claude 2.1.114 and opencode, captured in
/// `tests/fixtures/acp/recorded/*/basic-chat.jsonl`):
///
/// ```json
/// {
///   "sessionId": "…",
///   "update": {
///     "sessionUpdate": "usage_update",
///     "used": 22700,
///     "size": 1000000,
///     "cost": { "amount": 0.14204, "currency": "USD" }
///   }
/// }
/// ```
///
/// `cost` is deliberately dropped: nothing in Crucible displays or aggregates
/// a monetary figure, and inventing a consumer for it is not this function's
/// job.
///
/// Both counts are required. A frame carrying only one of them describes
/// neither an occupancy nor a window, and a zero-filled stand-in would be
/// indistinguishable from a real reading downstream.
pub fn extract_context_window(params: &Value) -> Option<(u64, u64)> {
    let update = params.get("update")?;
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("usage_update") {
        return None;
    }

    Some((
        update.get("used").and_then(Value::as_u64)?,
        update.get("size").and_then(Value::as_u64)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_claude_shape() {
        // Real fixture from claude 2.1.114
        let result = json!({
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 3,
                "outputTokens": 7,
                "cachedReadTokens": 0,
                "cachedWriteTokens": 22696,
                "totalTokens": 22706
            }
        });
        let usage = extract_usage(&result).expect("usage present");
        assert_eq!(usage.prompt_tokens, 3);
        assert_eq!(usage.completion_tokens, 7);
        assert_eq!(usage.total_tokens, 22706);
        assert_eq!(usage.cache_read_tokens, Some(0));
        assert_eq!(usage.cache_creation_tokens, Some(22696));
    }

    #[test]
    fn returns_none_when_absent() {
        let result = json!({ "stopReason": "end_turn" });
        assert!(extract_usage(&result).is_none());
    }

    #[test]
    fn returns_none_when_only_zero_total_no_token_fields() {
        // Empty usage object → no signal
        let result = json!({ "usage": {} });
        assert!(extract_usage(&result).is_none());
    }

    #[test]
    fn handles_snake_case_variant() {
        // Some agents may emit snake_case
        let result = json!({
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "total_tokens": 150
            }
        });
        let usage = extract_usage(&result).expect("usage present");
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
        assert!(usage.cache_read_tokens.is_none());
    }

    #[test]
    fn computes_total_when_only_input_output_present() {
        let result = json!({
            "usage": { "inputTokens": 7, "outputTokens": 3 }
        });
        let usage = extract_usage(&result).expect("usage present");
        assert_eq!(usage.total_tokens, 10);
    }

    /// The reason `extract_context_window` reads raw JSON instead of matching
    /// a `SessionUpdate` variant.
    ///
    /// `SessionUpdate` is internally tagged on `sessionUpdate` and its
    /// `UsageUpdate` variant is `#[cfg(feature = "unstable_session_usage")]`,
    /// which this build does not enable. So the tag is an *unknown variant*
    /// and the entire notification fails to deserialize — the frame never
    /// reaches a `match`, and no amount of arms in `streaming.rs` could have
    /// caught it. If a future dependency bump stabilizes the variant this test
    /// starts failing, which is the signal to reconsider the raw-JSON reader.
    #[test]
    fn usage_update_does_not_deserialize_as_a_typed_session_notification() {
        let params = json!({
            "sessionId": "c299d62f",
            "update": {
                "sessionUpdate": "usage_update",
                "used": 22700,
                "size": 1_000_000,
                "cost": { "amount": 0.14204, "currency": "USD" }
            }
        });

        let err =
            serde_json::from_value::<agent_client_protocol::SessionNotification>(params.clone())
                .expect_err("usage_update parsed as a typed SessionNotification");
        assert!(
            err.to_string().contains("unknown variant"),
            "expected an unknown-variant error, got {err}"
        );

        // The raw reader gets it anyway.
        assert_eq!(extract_context_window(&params), Some((22700, 1_000_000)));
    }

    #[test]
    fn extracts_the_opencode_window() {
        // Second recorded agent, different window — the reader must not be
        // tuned to one agent's numbers.
        let params = json!({
            "sessionId": "ses_257dac",
            "update": {
                "sessionUpdate": "usage_update",
                "used": 28224,
                "size": 200_000,
                "cost": { "amount": 0, "currency": "USD" }
            }
        });

        assert_eq!(extract_context_window(&params), Some((28224, 200_000)));
    }

    #[test]
    fn ignores_session_updates_that_are_not_usage_updates() {
        let params = json!({
            "sessionId": "s",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "hi" }
            }
        });

        assert!(extract_context_window(&params).is_none());
    }

    #[test]
    fn a_half_reported_window_is_no_window() {
        // Zero-filling the missing half would be indistinguishable downstream
        // from a real reading: `size: 0` divides the statusline by zero and
        // `used: 0` makes it draw a confident "0% ctx".
        let used_only = json!({
            "update": { "sessionUpdate": "usage_update", "used": 22700 }
        });
        let size_only = json!({
            "update": { "sessionUpdate": "usage_update", "size": 1_000_000 }
        });

        assert!(extract_context_window(&used_only).is_none());
        assert!(extract_context_window(&size_only).is_none());
    }

    #[test]
    fn saturates_huge_values_at_u32_max() {
        let result = json!({
            "usage": {
                "inputTokens": u64::MAX,
                "outputTokens": 1,
                "totalTokens": u64::MAX
            }
        });
        let usage = extract_usage(&result).expect("usage present");
        assert_eq!(usage.prompt_tokens, u32::MAX);
        assert_eq!(usage.total_tokens, u32::MAX);
    }
}
