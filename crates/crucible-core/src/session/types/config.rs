//! Context strategy, output validation, and related defaults/helpers.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Strategy for managing conversation context when it exceeds the token budget.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContextStrategy {
    /// Drop oldest non-system messages until under budget (default)
    #[default]
    Truncate,
    /// Keep system prompt + last N message pairs
    SlidingWindow,
    /// Replace oldest non-system non-last messages with a single
    /// elision-summary placeholder. Today the placeholder is a static
    /// "[N earlier turns elided]" line so the model knows context was
    /// dropped; a follow-up commit will replace this with a live
    /// LLM-generated recap that preserves names, decisions, and
    /// code references.
    Summarize,
}

impl std::fmt::Display for ContextStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncate => write!(f, "truncate"),
            Self::SlidingWindow => write!(f, "sliding_window"),
            Self::Summarize => write!(f, "summarize"),
        }
    }
}

impl FromStr for ContextStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "truncate" => Ok(Self::Truncate),
            "sliding_window" | "slidingwindow" => Ok(Self::SlidingWindow),
            "summarize" => Ok(Self::Summarize),
            _ => Err(format!(
                "unknown context strategy '{}'. Valid: truncate, sliding_window, summarize",
                s
            )),
        }
    }
}
