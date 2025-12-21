//! Callout and LaTeX expression types

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
    /// Returns true if this is a standard (non-custom) callout type
    pub fn is_standard(&self) -> bool {
        !matches!(self, CalloutType::Custom(_))
    }

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

/// Obsidian-style callout > [!type]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Callout {
    /// Callout type
    pub callout_type: CalloutType,

    /// Callout title (optional)
    pub title: Option<String>,

    /// Callout content
    pub content: String,

    /// Character offset in source note
    pub offset: usize,
}

impl Callout {
    /// Create a new callout
    pub fn new(callout_type: impl Into<CalloutType>, content: String, offset: usize) -> Self {
        Self {
            callout_type: callout_type.into(),
            title: None,
            content,
            offset,
        }
    }

    /// Create a callout with title
    pub fn with_title(
        callout_type: impl Into<CalloutType>,
        title: impl Into<String>,
        content: String,
        offset: usize,
    ) -> Self {
        Self {
            callout_type: callout_type.into(),
            title: Some(title.into()),
            content,
            offset,
        }
    }

    /// Returns true if this is a standard (non-custom) callout type
    pub fn is_standard_type(&self) -> bool {
        self.callout_type.is_standard()
    }

    /// Get the display type with fallback for custom types
    pub fn display_type(&self) -> &str {
        if self.callout_type.is_standard() {
            self.callout_type.as_str()
        } else {
            "note" // fallback to generic note type for rendering
        }
    }

    /// Get the start offset (backward compatibility)
    pub fn start_offset(&self) -> usize {
        self.offset
    }

    /// Get the total length of the callout
    pub fn length(&self) -> usize {
        let header_len = if let Some(title) = &self.title {
            format!("> [!{}] {}\n", self.callout_type, title).len()
        } else {
            format!("> [!{}]\n", self.callout_type).len()
        };
        header_len + self.content.len()
    }
}

/// LaTeX mathematical expression
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatexExpression {
    /// LaTeX expression content
    pub expression: String,

    /// Whether this is inline ($) or block ($$) math
    pub is_block: bool,

    /// Character offset in source note
    pub offset: usize,

    /// Length of the expression in source
    pub length: usize,
}

impl LatexExpression {
    /// Create a new LaTeX expression
    pub fn new(expression: String, is_block: bool, offset: usize, length: usize) -> Self {
        Self {
            expression,
            is_block,
            offset,
            length,
        }
    }

    /// Get the expression type as a string
    pub fn expression_type(&self) -> &'static str {
        if self.is_block {
            "block"
        } else {
            "inline"
        }
    }

    /// Get the start offset (backward compatibility)
    pub fn start_offset(&self) -> usize {
        self.offset
    }
}
