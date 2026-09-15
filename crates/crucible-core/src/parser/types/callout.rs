//! The kind of a callout block, used by block-level retrieval.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Standard Obsidian callout types
///
/// This enum represents all recognized callout types. Custom types are
/// supported via the `Custom` variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CalloutType {
    Note,
    Tip,
    Warning,
    Danger,
    Info,
    Abstract,
    Summary,
    Tldr,
    Todo,
    Question,
    Success,
    Failure,
    Example,
    Quote,
    Cite,
    Help,
    Important,
    Check,
    Bug,
    Caution,
    Attention,
    Tbd,
    /// Custom callout type not in the standard set
    #[serde(untagged)]
    Custom(String),
}

impl CalloutType {
    /// Get the string representation of this callout type
    pub fn as_str(&self) -> &str {
        match self {
            CalloutType::Note => "note",
            CalloutType::Tip => "tip",
            CalloutType::Warning => "warning",
            CalloutType::Danger => "danger",
            CalloutType::Info => "info",
            CalloutType::Abstract => "abstract",
            CalloutType::Summary => "summary",
            CalloutType::Tldr => "tldr",
            CalloutType::Todo => "todo",
            CalloutType::Question => "question",
            CalloutType::Success => "success",
            CalloutType::Failure => "failure",
            CalloutType::Example => "example",
            CalloutType::Quote => "quote",
            CalloutType::Cite => "cite",
            CalloutType::Help => "help",
            CalloutType::Important => "important",
            CalloutType::Check => "check",
            CalloutType::Bug => "bug",
            CalloutType::Caution => "caution",
            CalloutType::Attention => "attention",
            CalloutType::Tbd => "tbd",
            CalloutType::Custom(s) => s.as_str(),
        }
    }
}

impl fmt::Display for CalloutType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CalloutType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "note" => CalloutType::Note,
            "tip" => CalloutType::Tip,
            "warning" => CalloutType::Warning,
            "danger" => CalloutType::Danger,
            "info" => CalloutType::Info,
            "abstract" => CalloutType::Abstract,
            "summary" => CalloutType::Summary,
            "tldr" => CalloutType::Tldr,
            "todo" => CalloutType::Todo,
            "question" => CalloutType::Question,
            "success" => CalloutType::Success,
            "failure" => CalloutType::Failure,
            "example" => CalloutType::Example,
            "quote" => CalloutType::Quote,
            "cite" => CalloutType::Cite,
            "help" => CalloutType::Help,
            "important" => CalloutType::Important,
            "check" => CalloutType::Check,
            "bug" => CalloutType::Bug,
            "caution" => CalloutType::Caution,
            "attention" => CalloutType::Attention,
            "tbd" => CalloutType::Tbd,
            other => CalloutType::Custom(other.to_string()),
        })
    }
}

impl From<&str> for CalloutType {
    fn from(s: &str) -> Self {
        s.parse().unwrap() // Infallible
    }
}

impl From<String> for CalloutType {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}
