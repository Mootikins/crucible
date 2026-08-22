//! Thinking budget presets for LLM reasoning control.
//!
//! This module defines static presets that control how much "thinking" or reasoning
//! a language model should use when generating responses. Each preset has a suggested
//! token budget.
//!
//! ## Usage
//!
//! ```rust
//! use crucible_cli::tui::oil::config::presets::{ThinkingPreset, THINKING_PRESETS};
//!
//! // Look up a preset by name
//! if let Some(preset) = ThinkingPreset::by_name("medium") {
//!     println!("Using {} tokens", preset.tokens.unwrap_or(0));
//! }
//!
//! // Iterate all available presets
//! for name in ThinkingPreset::names() {
//!     println!("Available: {}", name);
//! }
//! ```

/// A thinking budget preset that controls LLM reasoning depth.
///
/// Each preset defines a token budget (or unlimited for max reasoning).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThinkingPreset {
    /// The preset name (e.g., "off", "minimal", "low", "medium", "high", "max").
    pub name: &'static str,

    /// Suggested token count for internal reasoning.
    ///
    /// - `Some(0)` = thinking disabled
    /// - `Some(n)` = target approximately `n` tokens
    /// - `None` = unlimited (no constraint)
    pub tokens: Option<u32>,
}

impl ThinkingPreset {
    /// Look up a preset by name (case-insensitive).
    ///
    /// Returns `None` if no preset matches the given name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use crucible_cli::tui::oil::config::presets::ThinkingPreset;
    /// let preset = ThinkingPreset::by_name("medium").unwrap();
    /// assert_eq!(preset.tokens, Some(4096));
    ///
    /// assert!(ThinkingPreset::by_name("unknown").is_none());
    /// ```
    #[must_use]
    pub fn by_name(name: &str) -> Option<&'static ThinkingPreset> {
        let name_lower = name.to_lowercase();
        THINKING_PRESETS.iter().find(|p| p.name == name_lower)
    }

    /// Iterate over all available preset names.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use crucible_cli::tui::oil::config::presets::ThinkingPreset;
    /// let names: Vec<_> = ThinkingPreset::names().collect();
    /// assert!(names.contains(&"off"));
    /// assert!(names.contains(&"max"));
    /// ```
    pub fn names() -> impl Iterator<Item = &'static str> {
        THINKING_PRESETS.iter().map(|p| p.name)
    }

    /// Convert preset to daemon-compatible i64 budget.
    /// Returns: -1 for unlimited, 0 for off, >0 for token count.
    #[must_use]
    pub fn to_budget(&self) -> i64 {
        match self.tokens {
            Some(n) => n as i64,
            None => -1,
        }
    }
}

/// Static array of all thinking budget presets.
///
/// Presets are ordered from least to most reasoning:
/// - `off` - No thinking, empty prompt
/// - `minimal` - Brief reasoning (~512 tokens)
/// - `low` - Light reasoning (~1024 tokens)
/// - `medium` - Moderate reasoning (~4096 tokens)
/// - `high` - Thorough reasoning (~8192 tokens)
/// - `max` - Unlimited reasoning
pub static THINKING_PRESETS: &[ThinkingPreset] = &[
    ThinkingPreset {
        name: "off",
        tokens: Some(0),
    },
    ThinkingPreset {
        name: "minimal",
        tokens: Some(512),
    },
    ThinkingPreset {
        name: "low",
        tokens: Some(1024),
    },
    ThinkingPreset {
        name: "medium",
        tokens: Some(4096),
    },
    ThinkingPreset {
        name: "high",
        tokens: Some(8192),
    },
    ThinkingPreset {
        name: "max",
        tokens: None,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_name_finds_existing_presets() {
        assert!(ThinkingPreset::by_name("off").is_some());
        assert!(ThinkingPreset::by_name("minimal").is_some());
        assert!(ThinkingPreset::by_name("low").is_some());
        assert!(ThinkingPreset::by_name("medium").is_some());
        assert!(ThinkingPreset::by_name("high").is_some());
        assert!(ThinkingPreset::by_name("max").is_some());
    }

    #[test]
    fn by_name_is_case_insensitive() {
        assert!(ThinkingPreset::by_name("OFF").is_some());
        assert!(ThinkingPreset::by_name("Medium").is_some());
        assert!(ThinkingPreset::by_name("MAX").is_some());
    }

    #[test]
    fn by_name_returns_none_for_unknown() {
        assert!(ThinkingPreset::by_name("unknown").is_none());
        assert!(ThinkingPreset::by_name("").is_none());
        assert!(ThinkingPreset::by_name("super_high").is_none());
    }

    #[test]
    fn names_returns_all_preset_names() {
        let names: Vec<_> = ThinkingPreset::names().collect();
        assert_eq!(names.len(), 6);
        assert!(names.contains(&"off"));
        assert!(names.contains(&"minimal"));
        assert!(names.contains(&"low"));
        assert!(names.contains(&"medium"));
        assert!(names.contains(&"high"));
        assert!(names.contains(&"max"));
    }

    #[test]
    fn presets_are_ordered_by_token_count() {
        let mut prev_tokens: Option<u32> = Some(0);

        for preset in THINKING_PRESETS.iter().take(5) {
            // Skip "max" which is None
            let current = preset.tokens.unwrap();
            assert!(
                current >= prev_tokens.unwrap(),
                "{} tokens ({}) should be >= previous ({})",
                preset.name,
                current,
                prev_tokens.unwrap()
            );
            prev_tokens = Some(current);
        }

        // Verify max is last and unlimited
        let max = THINKING_PRESETS.last().unwrap();
        assert_eq!(max.name, "max");
        assert!(max.tokens.is_none());
    }

    #[test]
    fn preset_tokens_values_are_correct() {
        assert_eq!(ThinkingPreset::by_name("off").unwrap().tokens, Some(0));
        assert_eq!(
            ThinkingPreset::by_name("minimal").unwrap().tokens,
            Some(512)
        );
        assert_eq!(ThinkingPreset::by_name("low").unwrap().tokens, Some(1024));
        assert_eq!(
            ThinkingPreset::by_name("medium").unwrap().tokens,
            Some(4096)
        );
        assert_eq!(ThinkingPreset::by_name("high").unwrap().tokens, Some(8192));
        assert_eq!(ThinkingPreset::by_name("max").unwrap().tokens, None);
    }
}
