//! Auto-compaction trigger logic.
//!
//! Decides — purely from the configured budget/threshold and the most
//! recent `TokenUsage` — whether the session should be flagged for
//! compaction at the next turn boundary. The actual state transition
//! happens via `SessionManager::request_compaction`; this module only
//! provides the boolean.

/// Default fraction of `context_budget` that triggers auto-compaction.
/// Surfaced here (rather than inline in `should_autocompact`) so the
/// constant is documented and can be referenced from tests.
pub const DEFAULT_AUTOCOMPACT_THRESHOLD: f32 = 0.95;

/// Decide whether `prompt_tokens` warrants a compaction request given
/// the session's `context_budget` and `autocompact_threshold` config.
///
/// Semantics:
/// - `budget == None`: never trigger (no budget to compare against).
/// - `threshold == None`: use [`DEFAULT_AUTOCOMPACT_THRESHOLD`].
/// - `threshold <= 0.0`: explicitly disabled (`:set autocompact_threshold=off`).
/// - `threshold >= 1.0`: only triggers if usage strictly exceeds budget.
pub fn should_autocompact(
    prompt_tokens: u32,
    context_budget: Option<usize>,
    autocompact_threshold: Option<f32>,
) -> bool {
    let Some(budget) = context_budget else {
        return false;
    };
    if budget == 0 {
        return false;
    }
    let threshold = autocompact_threshold.unwrap_or(DEFAULT_AUTOCOMPACT_THRESHOLD);
    if threshold <= 0.0 {
        return false;
    }
    let limit = (budget as f32) * threshold;
    (prompt_tokens as f32) > limit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_budget_never_triggers() {
        assert!(!should_autocompact(100_000, None, Some(0.5)));
        assert!(!should_autocompact(100_000, None, None));
    }

    #[test]
    fn zero_threshold_disables() {
        assert!(!should_autocompact(99_999, Some(100), Some(0.0)));
    }

    #[test]
    fn negative_threshold_disables() {
        assert!(!should_autocompact(99_999, Some(100), Some(-1.0)));
    }

    #[test]
    fn default_threshold_uses_95_percent() {
        // 950 / 1000 = 0.95 — at the boundary, must NOT trigger
        assert!(!should_autocompact(950, Some(1000), None));
        // 951 / 1000 — strictly over, triggers
        assert!(should_autocompact(951, Some(1000), None));
    }

    #[test]
    fn explicit_threshold_overrides_default() {
        assert!(!should_autocompact(499, Some(1000), Some(0.5)));
        assert!(should_autocompact(501, Some(1000), Some(0.5)));
    }

    #[test]
    fn threshold_above_one_only_triggers_strictly_over_budget() {
        assert!(!should_autocompact(1000, Some(1000), Some(1.0)));
        assert!(should_autocompact(1001, Some(1000), Some(1.0)));
    }
}
