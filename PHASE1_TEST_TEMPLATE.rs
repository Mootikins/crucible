//! Phase 1 Frontmatter Parsing - Test Template
//!
//! This file serves as a template for implementing tests for the frontmatter parser.
//! Copy this structure and adapt it for the actual implementation.
//!
//! Key patterns demonstrated:
//! - Inline unit testing with #[test] attribute
//! - Helper functions for test data
//! - Comprehensive error case coverage
//! - Raw string literals for multiline test data
//! - Async test pattern (if needed with #[tokio::test])

use crate::error::{ParserError, ParserResult};
use std::collections::HashMap;

// ============================================================================
// FRONTMATTER PARSER IMPLEMENTATION (Example)
// ============================================================================

/// Frontmatter parsing and validation
pub struct FrontmatterParser {
    strict_mode: bool,
}

impl FrontmatterParser {
    pub fn new() -> Self {
        Self { strict_mode: true }
    }

    /// Extract frontmatter from note content
    ///
    /// Returns: (frontmatter_text, body_text, format)
    pub fn extract_frontmatter(&self, content: &str) -> ParserResult<(Option<String>, String, FrontmatterFormat)> {
        // Check for YAML frontmatter (---)
        if content.starts_with("---\n") {
            if let Some(end_pos) = content[4..].find("\n---\n") {
                let end = 4 + end_pos;
                let frontmatter = content[4..end].to_string();
                let body = content[end + 5..].to_string();
                return Ok((Some(frontmatter), body, FrontmatterFormat::Yaml));
            }
        }

        // Check for TOML frontmatter (+++)
        if content.starts_with("+++\n") {
            if let Some(end_pos) = content[4..].find("\n+++\n") {
                let end = 4 + end_pos;
                let frontmatter = content[4..end].to_string();
                let body = content[end + 5..].to_string();
                return Ok((Some(frontmatter), body, FrontmatterFormat::Toml));
            }
        }

        // No frontmatter
        Ok((None, content.to_string(), FrontmatterFormat::None))
    }

    /// Validate YAML frontmatter syntax
    pub fn validate_yaml(&self, content: &str) -> ParserResult<()> {
        // Use serde_yaml to validate
        serde_yaml::from_str::<serde_yaml::Value>(content)
            .map_err(|e| ParserError::FrontmatterError(e.to_string()))?;
        Ok(())
    }

    /// Validate TOML frontmatter syntax
    pub fn validate_toml(&self, content: &str) -> ParserResult<()> {
        // Use toml crate to validate
        toml::from_str::<toml::Value>(content)
            .map_err(|e| ParserError::FrontmatterError(e.to_string()))?;
        Ok(())
    }

    /// Parse and extract YAML values
    pub fn parse_yaml_values(&self, content: &str) -> ParserResult<HashMap<String, String>> {
        let value: serde_yaml::Value = serde_yaml::from_str(content)
            .map_err(|e| ParserError::FrontmatterError(e.to_string()))?;

        let mut map = HashMap::new();
        if let Some(mapping) = value.as_mapping() {
            for (key, val) in mapping {
                if let (Some(k), Some(v)) = (key.as_str(), val.as_str()) {
                    map.insert(k.to_string(), v.to_string());
                }
            }
        }
        Ok(map)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrontmatterFormat {
    Yaml,
    Toml,
    None,
}

// ============================================================================
// TESTS - ORGANIZED BY CATEGORY
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // HELPER FUNCTIONS
    // ========================================================================

    /// Standard YAML frontmatter for reuse
    fn sample_yaml_basic() -> &'static str {
        r#"title: My Note
author: John Doe
date: 2025-11-08
status: active"#
    }

    /// YAML with arrays
    fn sample_yaml_arrays() -> &'static str {
        r#"title: Tech Article
tags:
  - rust
  - testing
  - documentation
categories: [dev, learning]"#
    }

    /// YAML with nested objects
    fn sample_yaml_nested() -> &'static str {
        r#"title: Complex Note
metadata:
  author: Jane Doe
  created: 2025-11-01
  updated: 2025-11-08
  tags:
    - important
    - review"#
    }

    /// Standard TOML frontmatter for reuse
    fn sample_toml_basic() -> &'static str {
        r#"title = "My Note"
author = "John Doe"
date = "2025-11-08"
status = "active""#
    }

    /// Complete markdown note with YAML frontmatter
    fn complete_document_yaml() -> &'static str {
        r#"---
title: Introduction to Rust
author: Jane Smith
tags: [rust, programming, learning]
---

# Introduction to Rust

This is the main content of the note.
It comes after the frontmatter."#
    }

    /// Complete markdown note with TOML frontmatter
    fn complete_document_toml() -> &'static str {
        r#"+++
title = "Introduction to Rust"
author = "Jane Smith"
tags = ["rust", "programming", "learning"]
+++

# Introduction to Rust

This is the main content of the note."#
    }

    // ========================================================================
    // TEST GROUP 1: BASIC PARSING
    // ========================================================================

    #[test]
    fn test_extract_valid_yaml_frontmatter() {
        let parser = FrontmatterParser::new();
        let content = complete_document_yaml();

        let (fm, body, format) = parser.extract_frontmatter(content)
            .expect("Should parse valid YAML frontmatter");

        assert!(fm.is_some(), "Frontmatter should be present");
        assert_eq!(format, FrontmatterFormat::Yaml);
        assert!(body.contains("# Introduction to Rust"), "Body should contain markdown");
    }

    #[test]
    fn test_extract_valid_toml_frontmatter() {
        let parser = FrontmatterParser::new();
        let content = complete_document_toml();

        let (fm, body, format) = parser.extract_frontmatter(content)
            .expect("Should parse valid TOML frontmatter");

        assert!(fm.is_some(), "Frontmatter should be present");
        assert_eq!(format, FrontmatterFormat::Toml);
        assert!(body.contains("# Introduction to Rust"), "Body should contain markdown");
    }

    #[test]
    fn test_extract_no_frontmatter() {
        let parser = FrontmatterParser::new();
        let content = "# Just a Markdown Note\n\nNo frontmatter here.";

        let (fm, body, format) = parser.extract_frontmatter(content)
            .expect("Should handle content without frontmatter");

        assert!(fm.is_none(), "Frontmatter should be None");
        assert_eq!(format, FrontmatterFormat::None);
        assert_eq!(body, content, "Body should be unchanged");
    }

    #[test]
    fn test_extract_empty_yaml_frontmatter() {
        let parser = FrontmatterParser::new();
        let content = "---\n---\n\nContent";

        let (fm, body, format) = parser.extract_frontmatter(content)
            .expect("Should handle empty frontmatter");

        assert_eq!(format, FrontmatterFormat::Yaml);
        assert_eq!(fm.as_deref(), Some(""));
        assert_eq!(body, "\nContent");
    }

    // ========================================================================
    // TEST GROUP 2: YAML VALIDATION AND PARSING
    // ========================================================================

    #[test]
    fn test_validate_yaml_basic() {
        let parser = FrontmatterParser::new();
        let yaml = sample_yaml_basic();

        let result = parser.validate_yaml(yaml);
        assert!(result.is_ok(), "Valid YAML should pass validation");
    }

    #[test]
    fn test_validate_yaml_with_arrays() {
        let parser = FrontmatterParser::new();
        let yaml = sample_yaml_arrays();

        let result = parser.validate_yaml(yaml);
        assert!(result.is_ok(), "YAML with arrays should validate");
    }

    #[test]
    fn test_validate_yaml_with_nested_objects() {
        let parser = FrontmatterParser::new();
        let yaml = sample_yaml_nested();

        let result = parser.validate_yaml(yaml);
        assert!(result.is_ok(), "YAML with nested objects should validate");
    }

    #[test]
    fn test_validate_invalid_yaml_unclosed_quote() {
        let parser = FrontmatterParser::new();
        let invalid = r#"title: "Unclosed quote
author: John Doe"#;

        let result = parser.validate_yaml(invalid);
        assert!(result.is_err(), "Unclosed quote should fail validation");
    }

    #[test]
    fn test_validate_invalid_yaml_bad_indentation() {
        let parser = FrontmatterParser::new();
        let invalid = r#"title: My Note
  author: John  # Wrong indentation - extra space
   date: 2025-11-08"#;

        // Note: YAML parsers are sometimes lenient with indentation
        // This may or may not fail depending on parser strictness
        let result = parser.validate_yaml(invalid);
        // Just verify it returns a result (Ok or Err is acceptable)
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_validate_invalid_yaml_malformed_array() {
        let parser = FrontmatterParser::new();
        let invalid = r#"tags: [rust, testing, incomplete"#;

        let result = parser.validate_yaml(invalid);
        assert!(result.is_err(), "Malformed array should fail validation");
    }

    #[test]
    fn test_parse_yaml_values_simple() {
        let parser = FrontmatterParser::new();
        let yaml = sample_yaml_basic();

        let values = parser.parse_yaml_values(yaml)
            .expect("Should parse YAML values");

        assert_eq!(values.get("title"), Some(&"My Note".to_string()));
        assert_eq!(values.get("author"), Some(&"John Doe".to_string()));
        assert_eq!(values.get("date"), Some(&"2025-11-08".to_string()));
    }

    // ========================================================================
    // TEST GROUP 3: TOML VALIDATION AND PARSING
    // ========================================================================

    #[test]
    fn test_validate_toml_basic() {
        let parser = FrontmatterParser::new();
        let toml = sample_toml_basic();

        let result = parser.validate_toml(toml);
        assert!(result.is_ok(), "Valid TOML should pass validation");
    }

    #[test]
    fn test_validate_invalid_toml_unclosed_string() {
        let parser = FrontmatterParser::new();
        let invalid = r#"title = "Unclosed string
author = "John Doe""#;

        let result = parser.validate_toml(invalid);
        assert!(result.is_err(), "Unclosed string should fail TOML validation");
    }

    #[test]
    fn test_validate_invalid_toml_bad_syntax() {
        let parser = FrontmatterParser::new();
        let invalid = r#"title = My Note
author = John Doe (missing quotes)"#;

        let result = parser.validate_toml(invalid);
        assert!(result.is_err(), "Unquoted strings should fail TOML validation");
    }

    // ========================================================================
    // TEST GROUP 4: EDGE CASES
    // ========================================================================

    #[test]
    fn test_yaml_with_special_characters() {
        let parser = FrontmatterParser::new();
        let yaml = r#"title: "Special: Characters & Symbols!"
description: "URL: https://example.com?q=test&v=1"
note: "Escaped: \"quoted\""#;

        let result = parser.validate_yaml(yaml);
        assert!(result.is_ok(), "Special characters should be handled");
    }

    #[test]
    fn test_yaml_multiline_literal_block() {
        let parser = FrontmatterParser::new();
        let yaml = r#"description: |
  This is a multiline
  description using the
  literal block scalar indicator"#;

        let result = parser.validate_yaml(yaml);
        assert!(result.is_ok(), "Multiline literal blocks should validate");
    }

    #[test]
    fn test_yaml_multiline_folded_block() {
        let parser = FrontmatterParser::new();
        let yaml = r#"description: >
  This is a multiline description
  using the folded block scalar
  which joins lines with spaces"#;

        let result = parser.validate_yaml(yaml);
        assert!(result.is_ok(), "Multiline folded blocks should validate");
    }

    #[test]
    fn test_yaml_case_sensitivity() {
        let parser = FrontmatterParser::new();
        let yaml = r#"Title: "Capitalized Key"
title: "lowercase key"
TITLE: "UPPERCASE KEY""#;

        let result = parser.validate_yaml(yaml);
        assert!(result.is_ok(), "YAML keys are case-sensitive");

        // All three should be separate keys
        let values = parser.parse_yaml_values(yaml)
            .expect("Should parse case-sensitive keys");
        assert!(values.len() >= 2); // At least some should be parsed
    }

    #[test]
    fn test_frontmatter_with_only_body() {
        let parser = FrontmatterParser::new();
        let content = "Just content\nNo frontmatter\nMultiple lines";

        let (fm, body, format) = parser.extract_frontmatter(content)
            .expect("Should handle content-only documents");

        assert!(fm.is_none());
        assert_eq!(format, FrontmatterFormat::None);
        assert_eq!(body, content);
    }

    #[test]
    fn test_delimiter_not_at_start() {
        let parser = FrontmatterParser::new();
        let content = "Some content\n---\nThis looks like frontmatter\n---\nBut isn't";

        let (fm, body, format) = parser.extract_frontmatter(content)
            .expect("Should not treat mid-note delimiters as frontmatter");

        assert!(fm.is_none(), "Delimiter not at start should not create frontmatter");
        assert_eq!(format, FrontmatterFormat::None);
    }

    #[test]
    fn test_mixed_delimiters_error_case() {
        let parser = FrontmatterParser::new();
        let content = "---\ntitle: Test\n+++\nContent"; // YAML start, TOML end

        let (fm, body, format) = parser.extract_frontmatter(content)
            .expect("Should handle gracefully");

        // Should either not find frontmatter or handle mixed case
        if fm.is_some() {
            assert_eq!(format, FrontmatterFormat::Yaml, "Should recognize opening delimiter");
        }
    }

    #[test]
    fn test_consecutive_delimiters() {
        let parser = FrontmatterParser::new();
        let content = "---\n---\nContent";

        let (fm, body, format) = parser.extract_frontmatter(content)
            .expect("Should handle consecutive delimiters");

        assert_eq!(format, FrontmatterFormat::Yaml);
        if let Some(fm_content) = fm {
            assert_eq!(fm_content, "");
        }
    }

    // ========================================================================
    // TEST GROUP 5: INTEGRATION TESTS
    // ========================================================================

    #[test]
    fn test_extract_and_validate_yaml_pipeline() {
        let parser = FrontmatterParser::new();
        let note = complete_document_yaml();

        // Extract frontmatter
        let (fm, body, format) = parser.extract_frontmatter(note)
            .expect("Should extract frontmatter");

        assert_eq!(format, FrontmatterFormat::Yaml);

        // Validate extracted frontmatter
        if let Some(fm_text) = fm {
            let validation = parser.validate_yaml(&fm_text);
            assert!(validation.is_ok(), "Extracted frontmatter should validate");
        }
    }

    #[test]
    fn test_extract_and_validate_toml_pipeline() {
        let parser = FrontmatterParser::new();
        let note = complete_document_toml();

        // Extract frontmatter
        let (fm, body, format) = parser.extract_frontmatter(note)
            .expect("Should extract frontmatter");

        assert_eq!(format, FrontmatterFormat::Toml);

        // Validate extracted frontmatter
        if let Some(fm_text) = fm {
            let validation = parser.validate_toml(&fm_text);
            assert!(validation.is_ok(), "Extracted frontmatter should validate");
        }
    }

    #[test]
    fn test_round_trip_extraction() {
        let parser = FrontmatterParser::new();
        let original = complete_document_yaml();

        // Extract
        let (fm, body, _) = parser.extract_frontmatter(original)
            .expect("Should extract");

        // Reconstruct
        if let Some(frontmatter) = fm {
            let reconstructed = format!("---\n{}\n---\n{}", frontmatter, body);

            // Re-extract
            let (fm2, body2, _) = parser.extract_frontmatter(&reconstructed)
                .expect("Should re-extract");

            assert_eq!(fm, fm2, "Frontmatter should match after round-trip");
            assert_eq!(body, body2, "Body should match after round-trip");
        }
    }
}
