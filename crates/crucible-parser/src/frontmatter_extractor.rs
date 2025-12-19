//! Comprehensive Frontmatter Extraction Utility
//!
//! This module provides a unified, robust frontmatter extraction implementation
//! that handles edge cases, multiple formats, and various line ending styles.
//!
//! ## Features
//!
//! - ✅ YAML frontmatter (--- delimiters)
//! - ✅ TOML frontmatter (+++ delimiters)
//! - ✅ Mixed line ending support (\n, \r\n, \r)
//! - ✅ Security (size limits, validation)
//! - ✅ Error reporting and recovery
//! - ✅ Comprehensive edge case handling
//!
//! ## Usage
//!
//! ```rust
//! use crucible_parser::frontmatter_extractor::FrontmatterExtractor;
//!
//! let content = "---\ntitle: Test\n---\n# Content here";
//! let extractor = FrontmatterExtractor::new();
//! let result = extractor.extract(content)?;
//!
//! // Access the frontmatter and body from the result struct
//! let frontmatter = result.frontmatter;
//! let body = result.body;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::error::{ParserError, ParserResult};
use crate::types::{Frontmatter, FrontmatterFormat};

/// Maximum allowed size for frontmatter sections (64KB)
/// This prevents denial-of-service attacks with extremely large frontmatter
const MAX_FRONTMATTER_SIZE: usize = 64 * 1024;

/// Frontmatter extraction configuration
#[derive(Debug, Clone)]
pub struct FrontmatterExtractorConfig {
    /// Maximum allowed frontmatter size
    pub max_size: usize,
    /// Whether to report parse errors or silently ignore them
    pub report_errors: bool,
    /// Whether to validate frontmatter structure
    pub validate_structure: bool,
}

impl Default for FrontmatterExtractorConfig {
    fn default() -> Self {
        Self {
            max_size: MAX_FRONTMATTER_SIZE,
            report_errors: true,
            validate_structure: true,
        }
    }
}

/// Result of frontmatter extraction
#[derive(Debug, Clone)]
pub struct FrontmatterResult {
    /// Extracted frontmatter (if any)
    pub frontmatter: Option<Frontmatter>,
    /// Remaining body content
    pub body: String,
    /// Frontmatter format detected
    pub format: FrontmatterFormat,
    /// Any warnings or errors encountered
    pub warnings: Vec<String>,
    /// Statistics about the extraction
    pub stats: ExtractionStats,
}

/// Statistics about frontmatter extraction
#[derive(Debug, Clone, Default)]
pub struct ExtractionStats {
    /// Length of frontmatter section
    pub frontmatter_size: usize,
    /// Length of body content
    pub body_size: usize,
    /// Number of lines in frontmatter
    pub frontmatter_lines: usize,
    /// Detected line ending style
    pub line_ending_style: LineEndingStyle,
    /// Time taken for extraction (in microseconds)
    pub extraction_time_us: u64,
}

/// Detected line ending style
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEndingStyle {
    /// Unix style (\n)
    Unix,
    /// Windows style (\r\n)
    Windows,
    /// Old Mac style (\r)
    OldMac,
    /// Mixed line endings
    Mixed,
}

impl Default for LineEndingStyle {
    fn default() -> Self {
        Self::Unix
    }
}

/// Comprehensive frontmatter extractor
///
/// This extractor handles all edge cases and provides robust error recovery.
/// It supports both YAML and TOML frontmatter with various line ending styles.
#[derive(Debug, Clone)]
pub struct FrontmatterExtractor {
    config: FrontmatterExtractorConfig,
}

impl FrontmatterExtractor {
    /// Create a new extractor with default configuration
    pub fn new() -> Self {
        Self::with_config(FrontmatterExtractorConfig::default())
    }

    /// Create a new extractor with custom configuration
    pub fn with_config(config: FrontmatterExtractorConfig) -> Self {
        Self { config }
    }

    /// Extract frontmatter from content
    ///
    /// This method handles all edge cases and returns detailed information
    /// about the extraction process.
    pub fn extract(&self, content: &str) -> ParserResult<FrontmatterResult> {
        let start_time = std::time::Instant::now();
        let mut warnings = Vec::new();
        let mut stats = ExtractionStats::default();

        // Early exit optimization: check if content has any frontmatter delimiters
        if !self.has_frontmatter_delimiters(content) {
            stats.body_size = content.len();
            stats.line_ending_style = self.detect_line_ending_style(content);
            stats.extraction_time_us = start_time.elapsed().as_micros() as u64;

            return Ok(FrontmatterResult {
                frontmatter: None,
                body: content.to_string(),
                format: FrontmatterFormat::None,
                warnings,
                stats,
            });
        }

        // Detect line ending style
        stats.line_ending_style = self.detect_line_ending_style(content);

        // Normalize line endings for processing
        let normalized = self.normalize_line_endings(content, stats.line_ending_style);

        // Extract frontmatter based on detected format
        let (frontmatter_content, body, format) =
            self.extract_frontmatter_content(&normalized, &mut warnings)?;

        // Validate frontmatter size
        if let Some(fm_content) = &frontmatter_content {
            // Calculate actual delimiter lengths based on the content structure
            let delimiters_len = match format {
                FrontmatterFormat::Yaml => {
                    // Opening: "---\n" (4 bytes) + Closing: "\n---\n" (5 bytes) = 9 bytes
                    9
                }
                FrontmatterFormat::Toml => {
                    // Opening: "+++\n" (4 bytes) + Closing: "\n+++\n" (5 bytes) = 9 bytes
                    9
                }
                FrontmatterFormat::None => 0,
            };
            stats.frontmatter_size = fm_content.len() + delimiters_len;
            stats.frontmatter_lines = fm_content.lines().count();

            if fm_content.len() > self.config.max_size {
                return Err(ParserError::FrontmatterTooLarge {
                    size: fm_content.len(),
                    max: self.config.max_size,
                });
            }

            // Create frontmatter object
            let frontmatter = Frontmatter::new(fm_content.clone(), format);

            stats.body_size = body.len();
            stats.extraction_time_us = start_time.elapsed().as_micros() as u64;

            Ok(FrontmatterResult {
                frontmatter: Some(frontmatter),
                body: body.to_string(),
                format,
                warnings,
                stats,
            })
        } else {
            stats.body_size = body.len();
            stats.extraction_time_us = start_time.elapsed().as_micros() as u64;

            Ok(FrontmatterResult {
                frontmatter: None,
                body: body.to_string(),
                format: FrontmatterFormat::None,
                warnings,
                stats,
            })
        }
    }

    /// Quick check if content contains any frontmatter delimiters
    /// This provides an early exit optimization for files without frontmatter
    fn has_frontmatter_delimiters(&self, content: &str) -> bool {
        // Check for common frontmatter patterns at the start of content
        let trimmed = content.trim_start();

        trimmed.starts_with("---")
            || trimmed.starts_with("+++")
            || trimmed.starts_with("---\r\n")
            || trimmed.starts_with("+++\r\n")
    }

    /// Detect the line ending style used in the content
    fn detect_line_ending_style(&self, content: &str) -> LineEndingStyle {
        let has_cr_lf = content.contains("\r\n");
        // Check for standalone \n that aren't part of \r\n
        let has_standalone_lf = content.matches("\n").count() > content.matches("\r\n").count();
        // Check for standalone \r that aren't part of \r\n
        let has_standalone_cr = content.matches('\r').count() > content.matches("\r\n").count();

        if has_cr_lf && (has_standalone_lf || has_standalone_cr) {
            LineEndingStyle::Mixed
        } else if has_cr_lf {
            LineEndingStyle::Windows
        } else if has_standalone_lf {
            LineEndingStyle::Unix
        } else if has_standalone_cr {
            LineEndingStyle::OldMac
        } else {
            LineEndingStyle::Unix // Default for empty content
        }
    }

    /// Normalize line endings to Unix style (\n) for consistent processing
    fn normalize_line_endings(&self, content: &str, style: LineEndingStyle) -> String {
        match style {
            LineEndingStyle::Unix => content.to_string(),
            LineEndingStyle::Windows => content.replace("\r\n", "\n"),
            LineEndingStyle::OldMac => content.replace('\r', "\n"),
            LineEndingStyle::Mixed => {
                // Handle mixed line endings by normalizing everything to \n
                content.replace("\r\n", "\n").replace('\r', "\n")
            }
        }
    }

    /// Extract frontmatter content from normalized content
    fn extract_frontmatter_content<'a>(
        &self,
        content: &'a str,
        warnings: &mut Vec<String>,
    ) -> ParserResult<(Option<String>, &'a str, FrontmatterFormat)> {
        // Try YAML frontmatter first
        if let Some((fm_content, body)) = self.extract_yaml_frontmatter(content, warnings)? {
            return Ok((Some(fm_content), body, FrontmatterFormat::Yaml));
        }

        // Try TOML frontmatter
        if let Some((fm_content, body)) = self.extract_toml_frontmatter(content, warnings)? {
            return Ok((Some(fm_content), body, FrontmatterFormat::Toml));
        }

        // No frontmatter found
        Ok((None, content, FrontmatterFormat::None))
    }

    /// Extract YAML frontmatter with comprehensive edge case handling
    fn extract_yaml_frontmatter<'a>(
        &self,
        content: &'a str,
        warnings: &mut Vec<String>,
    ) -> ParserResult<Option<(String, &'a str)>> {
        // Check for YAML frontmatter with various patterns
        let patterns = [
            "---\n",   // Standard Unix
            "---\r\n", // Windows (already normalized to \n)
            "---\r",   // Old Mac (already normalized to \n)
        ];

        for pattern in &patterns {
            if let Some(rest) = content.strip_prefix(pattern) {
                // Find closing delimiter
                if let Some(end_idx) = rest.find("\n---\n") {
                    let yaml_content = rest[..end_idx].trim();
                    let body = &rest[end_idx + 5..];

                    // Validate YAML content (basic checks)
                    if self.config.validate_structure {
                        if yaml_content.is_empty() {
                            warnings.push("Empty YAML frontmatter section".to_string());
                        } else {
                            // Check for common YAML syntax errors
                            self.validate_yaml_syntax(yaml_content, warnings)?;
                        }
                    }

                    return Ok(Some((yaml_content.to_string(), body)));
                } else {
                    // Opening delimiter found but no closing delimiter
                    if self.config.report_errors {
                        warnings.push(
                            "YAML frontmatter opening delimiter found but no closing delimiter"
                                .to_string(),
                        );
                    }
                }
            }
        }

        Ok(None)
    }

    /// Extract TOML frontmatter with comprehensive edge case handling
    fn extract_toml_frontmatter<'a>(
        &self,
        content: &'a str,
        warnings: &mut Vec<String>,
    ) -> ParserResult<Option<(String, &'a str)>> {
        // Check for TOML frontmatter with various patterns
        let patterns = [
            "+++\n",   // Standard Unix
            "+++\r\n", // Windows (already normalized to \n)
            "+++\r",   // Old Mac (already normalized to \n)
        ];

        for pattern in &patterns {
            if let Some(rest) = content.strip_prefix(pattern) {
                // Find closing delimiter
                if let Some(end_idx) = rest.find("\n+++\n") {
                    let toml_content = rest[..end_idx].trim();
                    let body = &rest[end_idx + 5..];

                    // Validate TOML content (basic checks)
                    if self.config.validate_structure {
                        if toml_content.is_empty() {
                            warnings.push("Empty TOML frontmatter section".to_string());
                        } else {
                            // Check for common TOML syntax errors
                            self.validate_toml_syntax(toml_content, warnings)?;
                        }
                    }

                    return Ok(Some((toml_content.to_string(), body)));
                } else {
                    // Opening delimiter found but no closing delimiter
                    if self.config.report_errors {
                        warnings.push(
                            "TOML frontmatter opening delimiter found but no closing delimiter"
                                .to_string(),
                        );
                    }
                }
            }
        }

        Ok(None)
    }

    /// Basic YAML syntax validation
    fn validate_yaml_syntax(&self, content: &str, warnings: &mut Vec<String>) -> ParserResult<()> {
        // Check for common YAML syntax issues
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Check for unbalanced quotes
            let open_quotes = trimmed.matches('"').count();
            if open_quotes % 2 != 0 {
                warnings.push(format!("Line {}: Unbalanced quotes detected", line_num + 1));
            }

            // Check for invalid indentation (YAML uses spaces, not tabs)
            if line.starts_with('\t') {
                warnings.push(format!(
                    "Line {}: Tab indentation detected (use spaces instead)",
                    line_num + 1
                ));
            }

            // Basic colon check for key-value pairs
            if trimmed.contains(':') && !trimmed.starts_with('-') {
                if let Some(colon_pos) = trimmed.find(':') {
                    let key = &trimmed[..colon_pos].trim();
                    if key.is_empty() {
                        warnings.push(format!("Line {}: Empty key before colon", line_num + 1));
                    }
                }
            }
        }

        Ok(())
    }

    /// Basic TOML syntax validation
    fn validate_toml_syntax(&self, content: &str, warnings: &mut Vec<String>) -> ParserResult<()> {
        // Check for common TOML syntax issues
        let lines: Vec<&str> = content.lines().collect();

        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Check for unbalanced quotes
            let open_quotes = trimmed.matches('"').count();
            if open_quotes % 2 != 0 {
                warnings.push(format!("Line {}: Unbalanced quotes detected", line_num + 1));
            }

            // Check for section headers
            if trimmed.starts_with('[') && !trimmed.ends_with(']') {
                warnings.push(format!("Line {}: Unclosed section header", line_num + 1));
            }

            // Basic equals sign check for key-value pairs
            if trimmed.contains('=') && !trimmed.starts_with('[') {
                if let Some(eq_pos) = trimmed.find('=') {
                    let key = &trimmed[..eq_pos].trim();
                    if key.is_empty() {
                        warnings.push(format!("Line {}: Empty key before equals", line_num + 1));
                    }
                }
            }
        }

        Ok(())
    }
}

impl Default for FrontmatterExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function for quick frontmatter extraction
pub fn extract_frontmatter(content: &str) -> ParserResult<FrontmatterResult> {
    let extractor = FrontmatterExtractor::new();
    extractor.extract(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_extraction_unix() {
        let content = "---\ntitle: Test\n---\n# Content";
        let result = extract_frontmatter(content).unwrap();

        assert!(result.frontmatter.is_some());
        assert_eq!(result.format, FrontmatterFormat::Yaml);
        assert_eq!(result.body, "# Content");
    }

    #[test]
    fn test_toml_extraction() {
        let content = "+++\ntitle = \"Test\"\n+++\n# Content";
        let result = extract_frontmatter(content).unwrap();

        assert!(result.frontmatter.is_some());
        assert_eq!(result.format, FrontmatterFormat::Toml);
        assert_eq!(result.body, "# Content");
    }

    #[test]
    fn test_no_frontmatter() {
        let content = "# Just content\nNo frontmatter here";
        let result = extract_frontmatter(content).unwrap();

        assert!(result.frontmatter.is_none());
        assert_eq!(result.format, FrontmatterFormat::None);
        assert_eq!(result.body, content);
    }

    #[test]
    fn test_mixed_line_endings() {
        let content = "---\r\ntitle: Test\r\n---\r\n# Content";
        let result = extract_frontmatter(content).unwrap();

        assert!(result.frontmatter.is_some());
        assert_eq!(result.stats.line_ending_style, LineEndingStyle::Windows);
    }

    #[test]
    fn test_empty_frontmatter() {
        let content = "---\n\n---\n# Content";
        let result = extract_frontmatter(content).unwrap();

        assert!(result.frontmatter.is_some());
        assert!(!result.warnings.is_empty());
        assert!(result
            .warnings
            .iter()
            .any(|w| w.to_lowercase().contains("empty")));
    }

    #[test]
    fn test_size_limit() {
        let large_content = "---\ntitle: ".to_string() + &"x".repeat(100_000) + "\n---\n# Content";
        let config = FrontmatterExtractorConfig {
            max_size: 1000,
            ..Default::default()
        };
        let extractor = FrontmatterExtractor::with_config(config);

        assert!(extractor.extract(&large_content).is_err());
    }

    #[test]
    fn test_unclosed_delimiters() {
        let content = "---\ntitle: Test\n# No closing delimiter";
        let result = extract_frontmatter(content).unwrap();

        assert!(result.frontmatter.is_none());
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_extra_delimiters() {
        let content = "----\ntitle: Test\n----\n# Content";
        let result = extract_frontmatter(content).unwrap();

        assert!(result.frontmatter.is_none());
    }
}
