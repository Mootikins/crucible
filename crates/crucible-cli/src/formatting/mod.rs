// Shared formatting utilities for CLI output
//
// This module provides common formatting functions and types to eliminate
// duplication across command implementations, following the DRY principle.

mod markdown_renderer;
pub use markdown_renderer::render_markdown;

pub mod syntax;
pub mod syntax_theme;
pub use syntax::SyntaxHighlighter;

/// Standard output format types supported across all commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Plain text output
    Plain,
    /// JSON output for programmatic consumption
    Json,
    /// Human-readable table format
    Table,
}

impl OutputFormat {
    /// Parse format from string
    #[allow(clippy::should_implement_trait)] // Infallible parsing with default, not FromStr semantics
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" => OutputFormat::Json,
            "table" => OutputFormat::Table,
            _ => OutputFormat::Plain,
        }
    }
}

impl From<String> for OutputFormat {
    fn from(s: String) -> Self {
        Self::from_str(&s)
    }
}

impl From<&str> for OutputFormat {
    fn from(s: &str) -> Self {
        Self::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_from_str() {
        assert_eq!(OutputFormat::from_str("json"), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str("JSON"), OutputFormat::Json);
        assert_eq!(OutputFormat::from_str("table"), OutputFormat::Table);
        assert_eq!(OutputFormat::from_str("unknown"), OutputFormat::Plain);
        assert_eq!(OutputFormat::from_str(""), OutputFormat::Plain);
    }
}
