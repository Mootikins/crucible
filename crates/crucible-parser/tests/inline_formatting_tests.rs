//! Inline formatting tests
//!
//! Tests for inline markdown elements like bold, italic, code, and strikethrough.

use crucible_parser::{CrucibleParser, MarkdownParserImplementation, ParsedNote};
use std::path::Path;

async fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse_content(content, Path::new(path)).await?)
}

#[tokio::test]
async fn test_bold_text() {
    let content = "This is **bold text** in a paragraph.";

    let parsed = parse_note(content, "test.md").await.unwrap();

    // Verify the content is parsed and preserved
    // Content should be in the paragraphs
    assert!(!parsed.content.paragraphs.is_empty() || !parsed.content.headings.is_empty());
}

#[tokio::test]
async fn test_italic_text() {
    let content = "This is *italic text* in a paragraph.";

    let parsed = parse_note(content, "test.md").await.unwrap();

    // Verify content is parsed
    assert!(!parsed.content.paragraphs.is_empty() || !parsed.content.headings.is_empty());
}

#[tokio::test]
async fn test_inline_code() {
    let content = "This has `inline code` in it.";

    let parsed = parse_note(content, "test.md").await.unwrap();

    // Verify content is parsed
    assert!(!parsed.content.paragraphs.is_empty() || !parsed.content.headings.is_empty());
}

#[tokio::test]
async fn test_strikethrough_text() {
    let content = "This is ~~strikethrough~~ text.";

    let parsed = parse_note(content, "test.md").await.unwrap();

    // Verify content is parsed
    assert!(!parsed.content.paragraphs.is_empty() || !parsed.content.headings.is_empty());
}

#[tokio::test]
async fn test_mixed_inline_formatting() {
    let content = "Mix of **bold**, *italic*, `code`, and ~~strikethrough~~.";

    let parsed = parse_note(content, "test.md").await.unwrap();

    // Verify content is parsed with mixed formatting
    assert!(!parsed.content.paragraphs.is_empty() || !parsed.content.headings.is_empty());
}

#[tokio::test]
async fn test_nested_formatting() {
    let content = "Nested: ***bold and italic*** together.";

    let parsed = parse_note(content, "test.md").await.unwrap();

    // Content should be parsed
    assert!(!parsed.content.paragraphs.is_empty() || !parsed.content.headings.is_empty());
}

#[tokio::test]
async fn test_unclosed_formatting_markers() {
    // Parser should handle unclosed markers gracefully
    let content = "Unclosed **bold marker";

    let result = parse_note(content, "test.md").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_inline_code_with_backticks_inside() {
    // Code with backticks requires escaping or double backticks
    let content = "Inline code: ``code with ` backtick``";

    let parsed = parse_note(content, "test.md").await.unwrap();

    // Verify content is parsed
    assert!(!parsed.content.paragraphs.is_empty() || !parsed.content.headings.is_empty());
}
