//! Thinking budget presets for LLM reasoning control.
//!
//! This module defines static presets that control how much "thinking" or reasoning
//! a language model should use when generating responses. Each preset has a suggested
//! token budget and a soft prompt that can be injected into the conversation.
//!
//! ## Usage
//!
//! ```rust
//! use crucible_cli::tui::oil::config::presets::{ThinkingPreset, THINKING_PRESETS};
//!
//! // Look up a preset by name
//! if let Some(preset) = ThinkingPreset::by_name("medium") {
//!     println!("Using {} tokens", preset.tokens.unwrap_or(0));
//!     println!("Prompt: {}", preset.render_soft_prompt());
//! }
//!
//! // Iterate all available presets
//! for name in ThinkingPreset::names() {
//!     println!("Available: {}", name);
//! }
//! ```

/// A thinking budget preset that controls LLM reasoning depth.
///
/// Each preset defines:
/// - A token budget (or unlimited for max reasoning)
/// - A soft prompt template that guides the model's internal reasoning
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

    /// Soft prompt template with optional `{tokens}` placeholder.
    ///
    /// When rendered, `{tokens}` is replaced with the actual token count.
    /// For presets with `tokens = None`, no substitution is needed.
    pub soft_prompt: &'static str,
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

    /// Render the soft prompt, substituting `{tokens}` with the actual token count.
    ///
    /// For the "off" preset, returns an empty string.
    /// For the "max" preset, returns the prompt as-is (no substitution needed).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use crucible_cli::tui::oil::config::presets::ThinkingPreset;
    /// let preset = ThinkingPreset::by_name("low").unwrap();
    /// let prompt = preset.render_soft_prompt();
    /// assert!(prompt.contains("1024"));
    /// assert!(prompt.contains("<thinking_budget>"));
    /// ```
    #[must_use]
    pub fn render_soft_prompt(&self) -> String {
        if self.soft_prompt.is_empty() {
            return String::new();
        }

        match self.tokens {
            Some(count) => self.soft_prompt.replace("{tokens}", &count.to_string()),
            None => self.soft_prompt.to_string(),
        }
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
        soft_prompt: "",
    },
    ThinkingPreset {
        name: "minimal",
        tokens: Some(512),
        soft_prompt: "<thinking_budget>Be concise. Limit internal reasoning to ~{tokens} tokens.</thinking_budget>",
    },
    ThinkingPreset {
        name: "low",
        tokens: Some(1024),
        soft_prompt: "<thinking_budget>Use brief reasoning, around {tokens} tokens.</thinking_budget>",
    },
    ThinkingPreset {
        name: "medium",
        tokens: Some(4096),
        soft_prompt: "<thinking_budget>Use moderate reasoning depth, around {tokens} tokens.</thinking_budget>",
    },
    ThinkingPreset {
        name: "high",
        tokens: Some(8192),
        soft_prompt: "<thinking_budget>Use thorough step-by-step reasoning, up to {tokens} tokens.</thinking_budget>",
    },
    ThinkingPreset {
        name: "max",
        tokens: None,
        soft_prompt: "<thinking_budget>Use extensive reasoning. Take as much space as needed.</thinking_budget>",
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
    fn render_soft_prompt_off_returns_empty() {
        let preset = ThinkingPreset::by_name("off").unwrap();
        assert_eq!(preset.render_soft_prompt(), "");
    }

    #[test]
    fn render_soft_prompt_substitutes_tokens() {
        let preset = ThinkingPreset::by_name("minimal").unwrap();
        let prompt = preset.render_soft_prompt();
        assert!(prompt.contains("512"), "Expected 512 in prompt: {prompt}");
        assert!(
            prompt.contains("<thinking_budget>"),
            "Expected XML tag in prompt"
        );
        assert!(
            prompt.contains("</thinking_budget>"),
            "Expected closing XML tag in prompt"
        );
        assert!(
            !prompt.contains("{tokens}"),
            "Placeholder should be replaced"
        );
    }

    #[test]
    fn render_soft_prompt_low_substitutes_correctly() {
        let preset = ThinkingPreset::by_name("low").unwrap();
        let prompt = preset.render_soft_prompt();
        assert!(prompt.contains("1024"), "Expected 1024 in prompt: {prompt}");
    }

    #[test]
    fn render_soft_prompt_medium_substitutes_correctly() {
        let preset = ThinkingPreset::by_name("medium").unwrap();
        let prompt = preset.render_soft_prompt();
        assert!(prompt.contains("4096"), "Expected 4096 in prompt: {prompt}");
    }

    #[test]
    fn render_soft_prompt_high_substitutes_correctly() {
        let preset = ThinkingPreset::by_name("high").unwrap();
        let prompt = preset.render_soft_prompt();
        assert!(prompt.contains("8192"), "Expected 8192 in prompt: {prompt}");
    }

    #[test]
    fn render_soft_prompt_max_no_substitution_needed() {
        let preset = ThinkingPreset::by_name("max").unwrap();
        let prompt = preset.render_soft_prompt();
        assert!(
            !prompt.contains("{tokens}"),
            "Max should not have placeholder"
        );
        assert!(
            prompt.contains("extensive reasoning"),
            "Expected max prompt content"
        );
        assert!(
            prompt.contains("<thinking_budget>"),
            "Expected XML tag in prompt"
        );
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
