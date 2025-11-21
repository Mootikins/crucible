//! LaTeX mathematical expression syntax extension
//!
//! This module implements support for inline and block LaTeX math:
//! - Inline math: `$\frac{3}{2}$`
//! - Block math: `$$\int_0^1 f(x)dx$$`

use super::error::{ParseError, ParseErrorType};
use super::extensions::SyntaxExtension;
use async_trait::async_trait;
use crate::parser::types::{LatexExpression, NoteContent};
use regex::Regex;
use std::sync::Arc;

/// LaTeX mathematical expression syntax extension
pub struct LatexExtension;

impl LatexExtension {
    /// Create a new LaTeX extension
    pub fn new() -> Self {
        Self
    }
}

impl Default for LatexExtension {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SyntaxExtension for LatexExtension {
    fn name(&self) -> &'static str {
        "latex-math"
    }

    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn description(&self) -> &'static str {
        "Supports LaTeX mathematical expressions using $...$ (inline) and $$...$$ (block) syntax"
    }

    fn can_handle(&self, content: &str) -> bool {
        content.contains('$')
            && (content.contains("$$") || content.chars().filter(|&c| c == '$').count() >= 2)
    }

    async fn parse(&self, content: &str, doc_content: &mut NoteContent) -> Vec<ParseError> {
        let mut errors = Vec::new();

        // Extract block math expressions first ($$...$$)
        if let Err(err) = self.extract_block_latex(content, doc_content) {
            errors.push(err);
        }

        // Extract inline math expressions ($...$) avoiding those in blocks
        if let Err(err) = self.extract_inline_latex(content, doc_content) {
            errors.push(err);
        }

        errors
    }

    fn priority(&self) -> u8 {
        80 // High priority to process before other extensions
    }
}

impl LatexExtension {
    /// Extract block LaTeX expressions ($$...$$)
    fn extract_block_latex(
        &self,
        content: &str,
        doc_content: &mut NoteContent,
    ) -> Result<(), ParseError> {
        // Pattern to match $$...$$ blocks
        let re = Regex::new(r"\$\$([\s\S]*?)\$\$").map_err(|e| {
            ParseError::error(
                format!("Failed to compile block LaTeX regex: {}", e),
                ParseErrorType::SyntaxError,
                0,
                0,
                0,
            )
        })?;

        for cap in re.captures_iter(content) {
            let full_match = cap.get(0).unwrap();
            let latex_content = cap.get(1).unwrap().as_str();

            // Basic LaTeX validation
            self.validate_latex_syntax(latex_content)?;

            // Add the LaTeX expression to note content
            doc_content.latex_expressions.push(LatexExpression::new(
                latex_content.to_string(),
                true, // is_block
                full_match.start(),
                full_match.len(),
            ));
        }

        Ok(())
    }

    /// Extract inline LaTeX expressions ($...$)
    fn extract_inline_latex(
        &self,
        original_content: &str,
        doc_content: &mut NoteContent,
    ) -> Result<(), ParseError> {
        // Remove block expressions first to avoid double-matching
        let content_without_blocks = Regex::new(r"\$\$[\s\S]*?\$\$")
            .map_err(|e| {
                ParseError::error(
                    format!("Failed to compile block removal regex: {}", e),
                    ParseErrorType::SyntaxError,
                    0,
                    0,
                    0,
                )
            })?
            .replace_all(original_content, "⟨REMOVED⟩");

        // Pattern for inline math (single $ delimiters, not escaped)
        // Note: Rust regex doesn't support lookbehind, so we match and filter manually
        let re = Regex::new(r"\$([^\$\n]+?)\$").map_err(|e| {
            ParseError::error(
                format!("Failed to compile inline LaTeX regex: {}", e),
                ParseErrorType::SyntaxError,
                0,
                0,
                0,
            )
        })?;

        for cap in re.captures_iter(&content_without_blocks) {
            let full_match = cap.get(0).unwrap();
            let latex_content = cap.get(1).unwrap().as_str();

            // Skip empty expressions
            if latex_content.trim().is_empty() {
                continue;
            }

            // Check if the $ is escaped (preceded by backslash in original content)
            let match_start = full_match.start();
            if match_start > 0 {
                let chars_before: Vec<char> = content_without_blocks.chars().collect();
                if chars_before.get(match_start.saturating_sub(1)) == Some(&'\\') {
                    continue; // Skip escaped dollar signs
                }
            }

            // Basic LaTeX validation
            if let Err(_error) = self.validate_latex_syntax(latex_content) {
                // For inline math, we can be more lenient with errors
                continue; // Skip problematic expressions rather than fail
            }

            // Add the LaTeX expression to note content
            doc_content.latex_expressions.push(LatexExpression::new(
                latex_content.to_string(),
                false, // is_inline
                full_match.start(),
                full_match.len(),
            ));
        }

        Ok(())
    }

    /// Basic LaTeX syntax validation
    fn validate_latex_syntax(&self, latex: &str) -> Result<(), ParseError> {
        let latex = latex.trim();

        // Check for balanced braces
        let mut brace_count: i32 = 0;
        let mut brace_positions = Vec::new();

        for (i, c) in latex.chars().enumerate() {
            match c {
                '{' => {
                    brace_count += 1;
                    brace_positions.push((i, '{'));
                }
                '}' => {
                    brace_count -= 1;
                    if brace_count >= 0 {
                        brace_positions.push((i, '}'));
                    }
                }
                _ => {}
            }
        }

        if brace_count != 0 {
            return Err(ParseError::warning(
                format!(
                    "Unbalanced braces in LaTeX expression: {} extra {}",
                    if brace_count > 0 {
                        "opening"
                    } else {
                        "closing"
                    },
                    brace_count.abs()
                ),
                ParseErrorType::InvalidLatex,
                0,
                0,
                0,
            ));
        }

        // Check for common problematic patterns
        if latex.contains("\\begin{") && !latex.contains("\\end{") {
            return Err(ParseError::warning(
                "LaTeX environment without \\end tag".to_string(),
                ParseErrorType::InvalidLatex,
                0,
                0,
                0,
            ));
        }

        // Check for potentially dangerous commands
        let dangerous_commands = ["\\write", "\\input", "\\include", "\\def"];
        for cmd in dangerous_commands {
            if latex.contains(cmd) {
                return Err(ParseError::warning(
                    format!("Potentially unsafe LaTeX command: {}", cmd),
                    ParseErrorType::InvalidLatex,
                    0,
                    0,
                    0,
                ));
            }
        }

        Ok(())
    }
}

/// Factory function to create the LaTeX extension
pub fn create_latex_extension() -> Arc<dyn SyntaxExtension> {
    Arc::new(LatexExtension::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::error::ErrorSeverity;

    #[tokio::test]
    async fn test_inline_latex_detection() {
        let extension = LatexExtension::new();

        assert!(extension.can_handle("$\\frac{3}{2}$"));
        assert!(extension.can_handle("The equation $E=mc^2$ is famous"));
        assert!(!extension.can_handle("Just regular text without dollar signs"));
        assert!(extension.can_handle("Mixed $$block$$ and $inline$ math"));
    }

    #[tokio::test]
    async fn test_inline_latex_parsing() {
        let extension = LatexExtension::new();
        let content = "The formula $E=mc^2$ describes mass-energy equivalence.";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.latex_expressions.len(), 1);
        assert_eq!(doc_content.latex_expressions[0].expression, "E=mc^2");
        assert_eq!(doc_content.latex_expressions[0].is_block, false);
    }

    #[tokio::test]
    async fn test_block_latex_parsing() {
        let extension = LatexExtension::new();
        let content = r#"
The integral is:
$$\int_0^1 f(x)dx = F(1) - F(0)$$
This is the result.
        "#;
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.latex_expressions.len(), 1);
        assert_eq!(
            doc_content.latex_expressions[0].expression.trim(),
            "\\int_0^1 f(x)dx = F(1) - F(0)"
        );
        assert_eq!(doc_content.latex_expressions[0].is_block, true);
    }

    #[tokio::test]
    async fn test_latex_validation() {
        let extension = LatexExtension::new();

        // Test balanced braces
        assert!(extension.validate_latex_syntax("\\frac{1}{2}").is_ok());
        assert!(extension.validate_latex_syntax("\\frac{1}{2").is_err());

        // Test dangerous commands
        let result = extension.validate_latex_syntax("\\input{malicious}");
        assert!(result.is_err());
        if let Err(error) = result {
            assert!(error.message.contains("unsafe"));
            assert_eq!(error.severity, ErrorSeverity::Warning);
        }
    }

    #[tokio::test]
    async fn test_mixed_inline_and_block() {
        let extension = LatexExtension::new();
        let content = "Inline $x+y$ and block $$\\frac{a}{b}$$ math.";
        let mut doc_content = NoteContent::new();

        let errors = extension.parse(content, &mut doc_content).await;

        assert_eq!(errors.len(), 0);
        assert_eq!(doc_content.latex_expressions.len(), 2);
        // Should extract both inline and block
        assert!(doc_content
            .latex_expressions
            .iter()
            .any(|e| !e.is_block && e.expression == "x+y"));
        assert!(doc_content
            .latex_expressions
            .iter()
            .any(|e| e.is_block && e.expression.trim() == "\\frac{a}{b}"));
    }

    #[tokio::test]
    async fn test_extension_metadata() {
        let extension = LatexExtension::new();

        assert_eq!(extension.name(), "latex-math");
        assert_eq!(extension.version(), "1.0.0");
        assert!(extension.description().contains("LaTeX"));
        assert_eq!(extension.priority(), 80);
        assert!(extension.is_enabled());
    }
}
