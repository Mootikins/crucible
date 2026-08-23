//! Extract token usage and context-window size from raw ACP JSON.
//!
//! The ACP spec defines a `usage` field on `PromptResponse` and a
//! `usage_update` session update. When this module was written, both sat
//! behind an unstable feature flag in the upstream Rust types, and a typed
//! parse of `sessionUpdate: "usage_update"` failed as an unknown variant. So
//! the fields are read from the raw JSON ahead of the typed parse in
//! `streaming.rs`. Schema 1.5 stabilized `UsageUpdate`; ACP item W9 moves
//! this reader to the typed variant.
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
///
/// `size: 0` is refused for the same reason a missing `size` is: it is not a
/// window. It travels as `TurnEvent::ContextWindow { limit: 0 }` and is
/// re-emitted as `context_limit_resolved { limit: 0, source: Agent }`, which
/// tells every subscriber the agent's window has been *resolved* — the
/// statusline consumers guard on `total > 0` so nothing divides by zero, but
/// they then display the "no data" state under a source that claims otherwise.
/// Reporting nothing lets the unresolved path stay unresolved.
pub fn extract_context_window(params: &Value) -> Option<(u64, u64)> {
    let update = params.get("update")?;
    if update.get("sessionUpdate").and_then(Value::as_str) != Some("usage_update") {
        return None;
    }

    let used = update.get("used").and_then(Value::as_u64)?;
    let size = update.get("size").and_then(Value::as_u64)?;

    (size > 0).then_some((used, size))
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

    /// `extract_context_window` reads raw JSON because the schema that the
    /// SDK 0.10 pulled in had no stable `UsageUpdate` variant. Schema 1.5
    /// stabilized it, so the typed parse now accepts the frame too. The raw
    /// reader still runs first in `streaming.rs` and keeps the behaviour
    /// identical. ACP item W9 moves the reader to the typed variant.
    #[test]
    fn usage_update_deserializes_as_a_typed_session_notification() {
        use agent_client_protocol::schema::v1::{SessionNotification, SessionUpdate};

        let params = json!({
            "sessionId": "c299d62f",
            "update": {
                "sessionUpdate": "usage_update",
                "used": 22700,
                "size": 1_000_000,
                "cost": { "amount": 0.14204, "currency": "USD" }
            }
        });

        let notification = serde_json::from_value::<SessionNotification>(params.clone())
            .expect("usage_update parses as a typed SessionNotification");
        assert!(
            matches!(notification.update, SessionUpdate::UsageUpdate(_)),
            "expected the UsageUpdate variant, got {:?}",
            notification.update
        );

        // The raw reader still reads it.
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
        // from a real reading: a stand-in `size` sets a window the agent never
        // reported, and `used: 0` makes the statusline draw a confident
        // "0% ctx".
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
    fn a_zero_size_is_no_window() {
        // Same reasoning as the missing half, one step further in: an explicit
        // `size: 0` is a value, so it survives `as_u64`, but it describes no
        // window. Passing it on emits `context_limit_resolved { limit: 0,
        // source: Agent }` — a claim that the agent's window is *resolved*,
        // sitting under a statusline that renders the no-data state because it
        // guards `total > 0`. Reporting nothing keeps unresolved unresolved.
        let zero_size = json!({
            "update": { "sessionUpdate": "usage_update", "used": 22700, "size": 0 }
        });
        assert!(extract_context_window(&zero_size).is_none());

        // Only `size` is refused for being zero. A fresh turn legitimately has
        // used nothing yet, and that is a real reading of a real window.
        let zero_used = json!({
            "update": { "sessionUpdate": "usage_update", "used": 0, "size": 200_000 }
        });
        assert_eq!(extract_context_window(&zero_used), Some((0, 200_000)));
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
