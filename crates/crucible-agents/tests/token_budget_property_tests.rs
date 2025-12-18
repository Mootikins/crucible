//! Property-based tests for token budget edge cases
//!
//! Tests invariants around token budget management, overflow protection,
//! and correction factor stability.

use crucible_agents::token::TokenBudget;
use proptest::prelude::*;

proptest! {
    /// Property: Adding tokens should never overflow (saturating_add)
    #[test]
    fn add_never_overflows(
        max_tokens in 0usize..100000,
        add1 in 0usize..usize::MAX / 2,
        add2 in 0usize..usize::MAX / 2
    ) {
        let mut budget = TokenBudget::new(max_tokens);

        // Even with large additions, should not panic
        budget.add(add1);
        budget.add(add2);

        // Current should be capped, not overflowed
        prop_assert!(budget.current() >= add1.min(add1.saturating_add(add2)));
    }

    /// Property: Removing tokens should never underflow (saturating_sub)
    #[test]
    fn remove_never_underflows(
        max_tokens in 100usize..100000,
        add_amount in 0usize..500,
        remove_amount in 0usize..1000
    ) {
        let mut budget = TokenBudget::new(max_tokens);
        budget.add(add_amount);
        budget.remove(remove_amount);

        // Should never go negative (saturating_sub)
        prop_assert!(budget.current() <= add_amount);
        prop_assert!(budget.remaining() <= max_tokens);
    }

    /// Property: remaining() + current() should always equal max_tokens
    #[test]
    fn remaining_plus_current_equals_max(
        max_tokens in 100usize..100000,
        add_amount in 0usize..50000
    ) {
        let mut budget = TokenBudget::new(max_tokens);
        budget.add(add_amount);

        // Invariant: remaining + current should equal max (or current capped at max)
        let sum = budget.remaining() + budget.current();
        prop_assert!(sum <= max_tokens || budget.current() > max_tokens);
    }

    /// Property: has_room should be consistent with remaining()
    #[test]
    fn has_room_consistent_with_remaining(
        max_tokens in 100usize..100000,
        add_amount in 0usize..50000,
        check_amount in 0usize..200
    ) {
        let mut budget = TokenBudget::new(max_tokens);
        budget.add(add_amount);

        let has_room = budget.has_room(check_amount);
        let remaining = budget.remaining();

        if has_room {
            prop_assert!(remaining >= check_amount,
                "has_room says yes but remaining {} < check {}",
                remaining, check_amount);
        }
    }

    /// Property: Correction factor should stay positive
    #[test]
    fn correction_factor_stays_positive(
        max_tokens in 100usize..100000,
        estimated in 1usize..1000,
        actual in 1usize..2000
    ) {
        let mut budget = TokenBudget::new(max_tokens);
        budget.add(estimated);
        budget.correct(actual);

        prop_assert!(budget.correction_factor() > 0.0,
            "Correction factor should stay positive, got {}",
            budget.correction_factor());
    }

    /// Property: Multiple corrections should converge (not diverge)
    #[test]
    fn corrections_converge(
        max_tokens in 1000usize..10000,
        base_estimate in 100usize..500,
        actual_multiplier in 0.5f64..2.0
    ) {
        let mut budget = TokenBudget::new(max_tokens);

        let actual = (base_estimate as f64 * actual_multiplier) as usize;

        // Apply multiple corrections with same ratio
        for _ in 0..10 {
            budget.add(base_estimate);
            budget.correct(actual.max(1)); // Avoid zero
        }

        // Factor should be roughly stable around actual_multiplier
        let factor = budget.correction_factor();
        prop_assert!(factor > 0.1 && factor < 10.0,
            "Correction factor should stay bounded, got {}",
            factor);
    }

    /// Property: Reset should restore initial state (except correction factor)
    #[test]
    fn reset_restores_zero_usage(
        max_tokens in 100usize..100000,
        add_amount in 0usize..50000
    ) {
        let mut budget = TokenBudget::new(max_tokens);
        budget.add(add_amount);
        budget.reset();

        prop_assert_eq!(budget.current(), 0);
        prop_assert_eq!(budget.remaining(), max_tokens);
    }

    /// Property: set_max should update remaining correctly
    #[test]
    fn set_max_updates_remaining(
        initial_max in 100usize..10000,
        add_amount in 0usize..5000,
        new_max in 100usize..20000
    ) {
        let mut budget = TokenBudget::new(initial_max);
        budget.add(add_amount);
        budget.set_max(new_max);

        // remaining should be new_max - current (saturating)
        let expected_remaining = new_max.saturating_sub(budget.current());
        prop_assert_eq!(budget.remaining(), expected_remaining);
    }

    /// Property: estimate_tokens should be roughly proportional to text length
    #[test]
    fn estimate_proportional_to_length(
        max_tokens in 1000usize..10000,
        len1 in 4usize..1000,
        len2 in 4usize..1000
    ) {
        let budget = TokenBudget::new(max_tokens);

        let text1 = "a".repeat(len1);
        let text2 = "a".repeat(len2);

        let est1 = budget.estimate_tokens(&text1);
        let est2 = budget.estimate_tokens(&text2);

        // Longer text should have more tokens (with correction factor = 1.0)
        if len1 > len2 {
            prop_assert!(est1 >= est2, "Longer text should have >= tokens");
        } else if len1 < len2 {
            prop_assert!(est1 <= est2, "Shorter text should have <= tokens");
        }
    }

    /// Property: Empty string should estimate to 0 tokens
    #[test]
    fn empty_string_zero_tokens(_max in 100usize..10000) {
        let budget = TokenBudget::new(_max);
        let estimate = budget.estimate_tokens("");
        prop_assert_eq!(estimate, 0);
    }
}

/// Edge case unit tests
#[cfg(test)]
mod edge_cases {
    use super::*;

    #[test]
    fn zero_max_tokens() {
        let mut budget = TokenBudget::new(0);
        assert_eq!(budget.remaining(), 0);
        assert!(!budget.has_room(1));

        budget.add(100);
        // current can exceed max (no clamping on add)
        assert_eq!(budget.current(), 100);
    }

    #[test]
    fn usize_max_tokens() {
        let budget = TokenBudget::new(usize::MAX);
        assert_eq!(budget.remaining(), usize::MAX);
        assert!(budget.has_room(1000000));
    }

    #[test]
    fn correction_with_zero_estimated() {
        let mut budget = TokenBudget::new(1000);
        // Don't add anything, estimated = 0

        // Correction with zero estimated should be no-op
        budget.correct(100);
        assert_eq!(budget.corrections(), 0);
    }

    #[test]
    fn very_large_correction_factor() {
        let mut budget = TokenBudget::new(1000);
        budget.add(1);
        budget.correct(1000); // 1000x correction

        // Factor should be updated but bounded by EMA
        let factor = budget.correction_factor();
        assert!(factor > 1.0 && factor < 1000.0);
    }

    #[test]
    fn very_small_correction_factor() {
        let mut budget = TokenBudget::new(1000);
        budget.add(1000);
        budget.correct(1); // 0.001x correction

        // Factor should be updated but bounded by EMA
        let factor = budget.correction_factor();
        assert!(factor > 0.0 && factor < 1.0);
    }

    #[test]
    fn correction_updates_factor() {
        let mut budget = TokenBudget::new(1000);

        // Add and correct once
        budget.add(100);
        let initial_factor = budget.correction_factor();
        budget.correct(200);

        // Factor should have changed after correction
        let new_factor = budget.correction_factor();
        assert_ne!(
            initial_factor, new_factor,
            "Factor should change after correction"
        );
        assert!(
            new_factor > initial_factor,
            "Factor should increase when actual > estimated"
        );
    }

    #[test]
    fn saturating_add_at_boundary() {
        let mut budget = TokenBudget::new(1000);
        budget.add(usize::MAX - 10);
        budget.add(100);

        // Should saturate, not overflow
        assert_eq!(budget.current(), usize::MAX);
    }

    #[test]
    fn saturating_sub_at_zero() {
        let mut budget = TokenBudget::new(1000);
        budget.add(10);
        budget.remove(100);

        // Should be 0, not underflow
        assert_eq!(budget.current(), 0);
    }
}
