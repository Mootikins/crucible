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

/// Validation to apply to agent text responses before returning to the user.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputValidation {
    /// No validation (default)
    #[default]
    None,
    /// Response must be valid JSON
    Json,
    /// Response must match the given regex pattern
    Regex(String),
    /// Lua-defined validator referenced by registered name.
    ///
    /// `validate_output` returns `Ok(())` for this variant; the daemon
    /// stream loop dispatches to the plugin's `LuaValidatorRegistry`
    /// separately. The variant exists in core only so it can flow through
    /// config, RPC, and Display/FromStr without reaching back into the
    /// Lua crate.
    Lua { name: String },
}

impl std::fmt::Display for OutputValidation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Json => write!(f, "json"),
            Self::Regex(p) => write!(f, "regex:{p}"),
            Self::Lua { name } => write!(f, "lua:{name}"),
        }
    }
}

impl FromStr for OutputValidation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Prefix-tagged forms preserve the case of their payload (regex
        // patterns and lua validator names are arbitrary identifiers).
        // Match those against the raw input before falling back to a
        // case-insensitive keyword match.
        if let Some(pattern) = s.strip_prefix("regex:") {
            regex::Regex::new(pattern).map_err(|e| format!("invalid regex pattern: {e}"))?;
            return Ok(Self::Regex(pattern.to_string()));
        }
        if let Some(name) = s.strip_prefix("lua:") {
            if name.is_empty() {
                return Err("lua validator requires a name (lua:<name>)".into());
            }
            return Ok(Self::Lua {
                name: name.to_string(),
            });
        }
        match s.to_lowercase().as_str() {
            "none" | "off" => Ok(Self::None),
            "json" => Ok(Self::Json),
            _ => Err(format!(
                "unknown validation '{}'. Valid: none, json, regex:<pattern>, lua:<name>",
                s
            )),
        }
    }
}

pub(super) fn default_validation_retries() -> u32 {
    3
}

pub(super) fn default_precognition_results() -> usize {
    5
}

/// Validate agent output against the configured validation mode.
///
/// Returns `Ok(())` if validation passes or is disabled, `Err(reason)` otherwise.
///
/// Note: `Regex` patterns are recompiled on each call. This is acceptable because
/// validation runs at most once per agent turn (after output generation). If this
/// becomes a hot path, consider caching the compiled regex in a side map.
pub fn validate_output(response: &str, validation: &OutputValidation) -> Result<(), String> {
    match validation {
        OutputValidation::None => Ok(()),
        OutputValidation::Json => serde_json::from_str::<serde_json::Value>(response)
            .map(|_| ())
            .map_err(|e| format!("Invalid JSON: {e}")),
        OutputValidation::Regex(pattern) => {
            // Pattern was validated at parse time (FromStr), so this should not fail
            let re =
                regex::Regex::new(pattern).map_err(|e| format!("Invalid regex pattern: {e}"))?;
            if re.is_match(response) {
                Ok(())
            } else {
                Err(format!("Response does not match pattern: {pattern}"))
            }
        }
        // Lua validators are evaluated by the daemon stream loop against
        // the plugin registry. Core's pure `validate_output` cannot hold a
        // `Lua` handle, so this is a no-op pass-through here.
        OutputValidation::Lua { .. } => Ok(()),
    }
}
