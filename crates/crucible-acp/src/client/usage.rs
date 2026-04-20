//! Extract token usage from raw ACP `PromptResponse` JSON.
//!
//! The ACP spec defines a `usage` field on `PromptResponse` (see the
//! `unstable_session_usage` feature in `agent-client-protocol-schema`), but
//! it's gated behind an unstable feature flag in the upstream Rust types.
//! Rather than coupling to that flag, we extract the field directly from the
//! deserialized JSON so we capture the data whether or not the upstream
//! crate exposes it as a typed field.
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
