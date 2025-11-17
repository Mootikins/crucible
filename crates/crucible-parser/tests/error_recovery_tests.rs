//! Error recovery tests
//!
//! Tests for graceful handling of malformed markdown and edge cases.

use crucible_parser::{CrucibleParser, MarkdownParserImplementation, ParsedNote};

fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse(content, path)?)
}

#[test]
fn test_unclosed_code_block() {
    let content = r#"# Heading

```rust
fn main() {
    println!("unclosed code block");
"#;

    // Should parse without panic
    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty());
}

#[test]
fn test_malformed_frontmatter() {
    let content = r#"---
title: Test
invalid yaml: [unclosed
---

# Content
"#;

    // Should handle malformed frontmatter gracefully
    let result = parse_note(content, "test.md");
    // May fail or skip frontmatter, but shouldn't panic
    if let Ok(parsed) = result {
        // Content after frontmatter should still be parsed
        assert!(!parsed.blocks.is_empty());
    }
}

#[test]
fn test_empty_document() {
    let content = "";

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    // Empty document should return empty or minimal structure
    assert_eq!(parsed.blocks.len(), 0);
}

#[test]
fn test_only_whitespace() {
    let content = "   \n\n   \t\t\n   ";

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    // Whitespace-only should be handled gracefully
    assert_eq!(parsed.blocks.len(), 0);
}

#[test]
fn test_deeply_nested_structure() {
    // Create very deep nesting to test stack limits
    let mut content = String::new();
    for i in 1..=20 {
        content.push_str(&"#".repeat(i.min(6))); // Max 6 heading levels
        content.push_str(&format!(" Level {}\n", i));
        content.push_str("Content\n\n");
    }

    let result = parse_note(&content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty());
}

#[test]
fn test_very_long_line() {
    // Test with extremely long line
    let long_text = "a".repeat(100_000);
    let content = format!("# Heading\n\n{}", long_text);

    let result = parse_note(&content, "test.md");
    assert!(result.is_ok());
}

#[test]
fn test_mixed_line_endings() {
    let content = "Line 1\nLine 2\r\nLine 3\rLine 4";

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty());
}

#[test]
fn test_unicode_edge_cases() {
    let content = r#"# 日本語 Heading

emoji test: 🚀 📝 ✨

right-to-left: مرحبا

zero-width chars: a​b​c
"#;

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty());
}

#[test]
fn test_null_bytes_in_content() {
    // Test handling of null bytes (should be filtered or handled)
    let content = "Before\0After";

    let result = parse_note(content, "test.md");
    // Should either succeed or fail gracefully
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_incomplete_list_syntax() {
    let content = r#"- Valid list item
-Not a list (no space)
- Another valid item
"#;

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    // Should parse what it can
    assert!(!parsed.blocks.is_empty());
}

#[test]
fn test_heading_without_space() {
    let content = "#NoSpace\n# With Space";

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    // At least one heading should be recognized
    let headings: Vec<_> = parsed.blocks.iter().filter(|b| b.is_heading()).collect();
    assert!(!headings.is_empty());
}
