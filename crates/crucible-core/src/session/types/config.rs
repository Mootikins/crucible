//! Context strategy, output validation, and related defaults/helpers.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Strategy for managing conversation context when it exceeds the token budget.
///
/// Serializes lowercase (`"truncate"`/`"summarize"`), matching [`FromStr`] and
/// [`Display`](std::fmt::Display) below. Before this type carried
/// `#[serde(rename_all)]`, the derive wrote the variant names as-is
/// (`"Truncate"`/`"Summarize"`) wherever a `SessionAgent` serialized this
/// field — including every session a daemon persisted to disk. The `alias`
/// on each variant keeps those old records loading: [`Deserialize`] accepts
/// either spelling, and [`Serialize`] only ever writes the new one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ContextStrategy {
    /// Drop oldest non-system messages until under budget (default)
    #[default]
    #[serde(alias = "Truncate")]
    Truncate,
    /// Replace the oldest non-system, non-last messages with one recap.
    ///
    /// The daemon asks the model to summarise what it drained and puts that
    /// in the hole (`summarize_via_backend`). A failed or empty summarize
    /// call leaves a static "[N earlier turns elided]" marker instead, so the
    /// model is always told that context was dropped.
    ///
    /// `SlidingWindow` used to sit between this and `Truncate`. It drained
    /// exactly what this drains and left nothing in its place — the same
    /// turns lost, with no marker saying so.
    #[serde(alias = "Summarize")]
    Summarize,
}

impl std::fmt::Display for ContextStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncate => write!(f, "truncate"),
            Self::Summarize => write!(f, "summarize"),
        }
    }
}

impl FromStr for ContextStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "truncate" => Ok(Self::Truncate),
            "summarize" => Ok(Self::Summarize),
            _ => Err(format!(
                "unknown context strategy '{}'. Valid: truncate, summarize",
                s
            )),
        }
    }
}
