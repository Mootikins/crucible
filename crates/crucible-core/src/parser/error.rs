//! Parser error types

use serde::{Deserialize, Serialize};
use std::io;
use thiserror::Error;

/// Parser error type
#[derive(Debug, Error)]
pub enum ParserError {
    /// IO error reading file
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// Frontmatter parsing failed
    #[error("Frontmatter parse error: {0}")]
    FrontmatterError(String),

    /// File exceeds size limit
    #[error("File too large: {size} bytes (max {max} bytes)")]
    FileTooLarge {
        /// Actual file size
        size: usize,
        /// Maximum allowed size
        max: usize,
    },

    /// File content is not valid UTF-8
    #[error("Invalid UTF-8 encoding in file")]
    EncodingError,

    /// General parsing failure
    #[error("Parsing failed: {0}")]
    ParseFailed(String),

    /// Feature not supported by this parser
    #[error("Feature not supported: {0}")]
    Unsupported(String),

    /// Invalid file path
    #[error("Invalid file path: {0}")]
    InvalidPath(String),
}

/// Non-fatal parsing error for tracking issues during document parsing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseError {
    /// Error message
    pub message: String,

    /// Error type
    pub error_type: ParseErrorType,

    /// Line number where error occurred (0-based)
    pub line: usize,

    /// Column number where error occurred (0-based)
    pub column: usize,

    /// Character offset in source
    pub offset: usize,

    /// Error severity
    pub severity: ErrorSeverity,
}

impl ParseError {
    /// Create a new parse error
    pub fn new(
        message: String,
        error_type: ParseErrorType,
        line: usize,
        column: usize,
        offset: usize,
    ) -> Self {
        Self {
            message,
            error_type,
            line,
            column,
            offset,
            severity: ErrorSeverity::Warning,
        }
    }

    /// Create a warning
    pub fn warning(
        message: String,
        error_type: ParseErrorType,
        line: usize,
        column: usize,
        offset: usize,
    ) -> Self {
        Self {
            message,
            error_type,
            line,
            column,
            offset,
            severity: ErrorSeverity::Warning,
        }
    }

    /// Create an error
    pub fn error(
        message: String,
        error_type: ParseErrorType,
        line: usize,
        column: usize,
        offset: usize,
    ) -> Self {
        Self {
            message,
            error_type,
            line,
            column,
            offset,
            severity: ErrorSeverity::Error,
        }
    }
}

/// Types of parsing errors
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParseErrorType {
    /// Malformed wikilink syntax
    MalformedWikilink,

    /// Invalid LaTeX expression
    InvalidLatex,

    /// Broken footnote reference
    BrokenFootnoteReference,

    /// Invalid callout syntax
    InvalidCallout,

    /// Frontmatter syntax issue
    FrontmatterSyntax,

    /// Circular transclusion detected
    CircularTransclusion,

    /// Invalid tag format
    InvalidTag,

    /// General syntax error
    SyntaxError,
}

/// Error severity levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorSeverity {
    /// Warning - non-critical issue
    Warning,

    /// Error - more serious issue
    Error,
}

/// Specialized Result type for parser operations
pub type ParserResult<T> = Result<T, ParserError>;

impl ParserError {
    /// Create a frontmatter error
    pub fn frontmatter(msg: impl Into<String>) -> Self {
        Self::FrontmatterError(msg.into())
    }

    /// Create a parse failure error
    pub fn parse_failed(msg: impl Into<String>) -> Self {
        Self::ParseFailed(msg.into())
    }

    /// Create an unsupported feature error
    pub fn unsupported(feature: impl Into<String>) -> Self {
        Self::Unsupported(feature.into())
    }

    /// Check if this error is recoverable (non-fatal)
    pub fn is_recoverable(&self) -> bool {
        matches!(self, Self::FrontmatterError(_) | Self::ParseFailed(_))
    }

    /// Check if this error is fatal (should stop parsing)
    pub fn is_fatal(&self) -> bool {
        !self.is_recoverable()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification() {
        let err = ParserError::frontmatter("bad yaml");
        assert!(err.is_recoverable());
        assert!(!err.is_fatal());

        let err = ParserError::FileTooLarge {
            size: 1000,
            max: 500,
        };
        assert!(err.is_fatal());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_error_display() {
        let err = ParserError::FileTooLarge {
            size: 1000,
            max: 500,
        };
        assert_eq!(
            err.to_string(),
            "File too large: 1000 bytes (max 500 bytes)"
        );

        let err = ParserError::frontmatter("invalid syntax");
        assert_eq!(err.to_string(), "Frontmatter parse error: invalid syntax");
    }
}
