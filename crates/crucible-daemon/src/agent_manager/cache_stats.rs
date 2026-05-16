//! Per-session prompt-cache accounting.
//!
//! Anthropic returns `cache_read_tokens` (cache hit) and
//! `cache_creation_tokens` (cache miss / first-write) on each completion.
//! We aggregate them per session so the TUI statusline and Lua plugins
//! can show a hit rate without subscribing to every event.

use crucible_core::traits::llm::TokenUsage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheStats {
    /// Completions where `cache_read_tokens > 0`.
    pub hits: u64,
    /// Completions where `cache_read_tokens` was 0 or absent.
    pub misses: u64,
    /// Total tokens served from cache across all completions.
    pub read_tokens: u64,
    /// Total tokens written into cache across all completions.
    pub creation_tokens: u64,
    /// Total prompt tokens (cached + uncached). Useful for sanity checks.
    pub prompt_tokens: u64,
    /// Total completion tokens. Surfaced for parity with prompt_tokens.
    pub completion_tokens: u64,
}

impl CacheStats {
    pub fn record(&mut self, usage: &TokenUsage) {
        let read = usage.cache_read_tokens.unwrap_or(0) as u64;
        let creation = usage.cache_creation_tokens.unwrap_or(0) as u64;
        if read > 0 {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        self.read_tokens = self.read_tokens.saturating_add(read);
        self.creation_tokens = self.creation_tokens.saturating_add(creation);
        self.prompt_tokens = self
            .prompt_tokens
            .saturating_add(usage.prompt_tokens as u64);
        self.completion_tokens = self
            .completion_tokens
            .saturating_add(usage.completion_tokens as u64);
    }

    /// Fraction of cacheable prompt tokens served from cache.
    /// Returns `None` when no completion has reported cache fields yet.
    pub fn hit_rate(&self) -> Option<f64> {
        let denom = self.read_tokens + self.creation_tokens;
        if denom == 0 {
            None
        } else {
            Some(self.read_tokens as f64 / denom as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(prompt: u32, completion: u32, read: Option<u32>, creation: Option<u32>) -> TokenUsage {
        TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: prompt + completion,
            cache_read_tokens: read,
            cache_creation_tokens: creation,
        }
    }

    #[test]
    fn hit_recorded_when_read_tokens_present() {
        let mut s = CacheStats::default();
        s.record(&usage(120, 30, Some(100), Some(20)));
        assert_eq!(s.hits, 1);
        assert_eq!(s.misses, 0);
        assert_eq!(s.read_tokens, 100);
        assert_eq!(s.creation_tokens, 20);
    }

    #[test]
    fn miss_recorded_when_read_tokens_zero_or_absent() {
        let mut s = CacheStats::default();
        s.record(&usage(50, 10, None, None));
        s.record(&usage(50, 10, Some(0), Some(50)));
        assert_eq!(s.hits, 0);
        assert_eq!(s.misses, 2);
    }

    #[test]
    fn hit_rate_aggregates_across_completions() {
        let mut s = CacheStats::default();
        // First call: 100 cached, 20 written
        s.record(&usage(120, 30, Some(100), Some(20)));
        // Second call: 200 cached, 0 written
        s.record(&usage(200, 50, Some(200), None));
        // 300 / (300 + 20) = 0.9375
        let r = s.hit_rate().expect("hit_rate should be Some");
        assert!((r - 0.9375).abs() < 1e-9, "got {r}");
    }

    #[test]
    fn hit_rate_none_before_any_data() {
        let s = CacheStats::default();
        assert!(s.hit_rate().is_none());
    }
}
