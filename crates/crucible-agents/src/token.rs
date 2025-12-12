//! Token Budget Management
//!
//! Provides token counting and budget management with drift correction.

use serde::{Deserialize, Serialize};

/// Token budget tracker with drift correction
///
/// Tracks estimated token usage and periodically corrects against actual
/// token counts from the LLM provider to prevent drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Maximum tokens allowed
    max_tokens: usize,

    /// Current estimated token count
    estimated_tokens: usize,

    /// Correction factor (actual / estimated)
    correction_factor: f64,

    /// Number of corrections performed
    corrections: usize,
}

impl TokenBudget {
    /// Create a new token budget
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            estimated_tokens: 0,
            correction_factor: 1.0,
            corrections: 0,
        }
    }

    /// Estimate tokens in text using a simple heuristic
    ///
    /// Uses the approximation: tokens ≈ text.len() / 4
    /// This will be corrected over time using actual provider counts.
    pub fn estimate_tokens(&self, text: &str) -> usize {
        let raw_estimate = text.len() / 4;
        (raw_estimate as f64 * self.correction_factor) as usize
    }

    /// Add tokens to the budget
    pub fn add(&mut self, tokens: usize) {
        self.estimated_tokens = self.estimated_tokens.saturating_add(tokens);
    }

    /// Remove tokens from the budget
    pub fn remove(&mut self, tokens: usize) {
        self.estimated_tokens = self.estimated_tokens.saturating_sub(tokens);
    }

    /// Check if we have room for additional tokens
    pub fn has_room(&self, tokens: usize) -> bool {
        self.estimated_tokens + tokens <= self.max_tokens
    }

    /// Get remaining token budget
    pub fn remaining(&self) -> usize {
        self.max_tokens.saturating_sub(self.estimated_tokens)
    }

    /// Get current estimated token count
    pub fn current(&self) -> usize {
        self.estimated_tokens
    }

    /// Correct the budget based on actual token count from provider
    ///
    /// This helps prevent drift between our estimates and actual usage.
    pub fn correct(&mut self, actual_tokens: usize) {
        if self.estimated_tokens > 0 {
            let new_factor = actual_tokens as f64 / self.estimated_tokens as f64;

            // Use exponential moving average for smooth corrections
            let alpha = 0.3; // Weight for new observation
            self.correction_factor = alpha * new_factor + (1.0 - alpha) * self.correction_factor;

            self.estimated_tokens = actual_tokens;
            self.corrections += 1;
        }
    }

    /// Get the current correction factor
    pub fn correction_factor(&self) -> f64 {
        self.correction_factor
    }

    /// Get the number of corrections performed
    pub fn corrections(&self) -> usize {
        self.corrections
    }

    /// Reset the budget to zero usage
    pub fn reset(&mut self) {
        self.estimated_tokens = 0;
    }

    /// Set a new maximum token limit
    pub fn set_max(&mut self, max_tokens: usize) {
        self.max_tokens = max_tokens;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_budget() {
        let budget = TokenBudget::new(1000);
        assert_eq!(budget.max_tokens, 1000);
        assert_eq!(budget.current(), 0);
        assert_eq!(budget.remaining(), 1000);
    }

    #[test]
    fn test_estimate_tokens() {
        let budget = TokenBudget::new(1000);
        // Rough estimate: 4 chars per token
        let text = "a".repeat(400);
        let estimate = budget.estimate_tokens(&text);
        assert_eq!(estimate, 100);
    }

    #[test]
    fn test_add_remove() {
        let mut budget = TokenBudget::new(1000);
        budget.add(100);
        assert_eq!(budget.current(), 100);
        assert_eq!(budget.remaining(), 900);

        budget.remove(50);
        assert_eq!(budget.current(), 50);
        assert_eq!(budget.remaining(), 950);
    }

    #[test]
    fn test_has_room() {
        let mut budget = TokenBudget::new(1000);
        budget.add(900);
        assert!(budget.has_room(100));
        assert!(!budget.has_room(101));
    }

    #[test]
    fn test_correction() {
        let mut budget = TokenBudget::new(1000);
        budget.add(100);

        // Actual usage was higher than estimated
        budget.correct(120);
        assert_eq!(budget.current(), 120);
        assert!(budget.correction_factor() > 1.0);
        assert_eq!(budget.corrections(), 1);
    }

    #[test]
    fn test_correction_factor_applied() {
        let mut budget = TokenBudget::new(1000);
        let text = "a".repeat(400);

        // Initial estimate
        let estimate1 = budget.estimate_tokens(&text);

        // Correct with higher actual count
        budget.add(estimate1);
        budget.correct(120);

        // Next estimate should be higher
        let estimate2 = budget.estimate_tokens(&text);
        assert!(estimate2 > estimate1);
    }

    #[test]
    fn test_reset() {
        let mut budget = TokenBudget::new(1000);
        budget.add(500);
        budget.reset();
        assert_eq!(budget.current(), 0);
        assert_eq!(budget.remaining(), 1000);
    }

    #[test]
    fn test_set_max() {
        let mut budget = TokenBudget::new(1000);
        budget.add(500);
        budget.set_max(2000);
        assert_eq!(budget.remaining(), 1500);
    }
}
