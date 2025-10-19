//! Parser error types

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
        matches!(
            self,
            Self::FrontmatterError(_) | Self::ParseFailed(_)
        )
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

        let err = ParserError::FileTooLarge { size: 1000, max: 500 };
        assert!(err.is_fatal());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_error_display() {
        let err = ParserError::FileTooLarge { size: 1000, max: 500 };
        assert_eq!(err.to_string(), "File too large: 1000 bytes (max 500 bytes)");

        let err = ParserError::frontmatter("invalid syntax");
        assert_eq!(err.to_string(), "Frontmatter parse error: invalid syntax");
    }
}
