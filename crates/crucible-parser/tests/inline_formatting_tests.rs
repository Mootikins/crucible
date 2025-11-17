//! Inline formatting tests
//!
//! Tests for inline markdown elements like bold, italic, code, and strikethrough.

use crucible_parser::{CrucibleParser, MarkdownParserImplementation, ParsedNote};

fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse(content, path)?)
}

#[test]
fn test_bold_text() {
    let content = "This is **bold text** in a paragraph.";

    let parsed = parse_note(content, "test.md").unwrap();
    assert!(!parsed.blocks.is_empty());

    // Verify the content is preserved even if not specially marked
    let content_str = parsed.blocks[0].content.to_lowercase();
    assert!(content_str.contains("bold"));
}

#[test]
fn test_italic_text() {
    let content = "This is *italic text* in a paragraph.";

    let parsed = parse_note(content, "test.md").unwrap();
    assert!(!parsed.blocks.is_empty());

    let content_str = parsed.blocks[0].content.to_lowercase();
    assert!(content_str.contains("italic"));
}

#[test]
fn test_inline_code() {
    let content = "This has `inline code` in it.";

    let parsed = parse_note(content, "test.md").unwrap();
    assert!(!parsed.blocks.is_empty());

    let content_str = parsed.blocks[0].content.to_lowercase();
    assert!(content_str.contains("code") || content_str.contains("`"));
}

#[test]
fn test_strikethrough_text() {
    let content = "This is ~~strikethrough~~ text.";

    let parsed = parse_note(content, "test.md").unwrap();
    assert!(!parsed.blocks.is_empty());

    let content_str = parsed.blocks[0].content.to_lowercase();
    assert!(content_str.contains("strikethrough"));
}

#[test]
fn test_mixed_inline_formatting() {
    let content = "Mix of **bold**, *italic*, `code`, and ~~strikethrough~~.";

    let parsed = parse_note(content, "test.md").unwrap();
    assert!(!parsed.blocks.is_empty());

    let content_str = parsed.blocks[0].content.to_lowercase();
    assert!(content_str.contains("bold"));
    assert!(content_str.contains("italic"));
    assert!(content_str.contains("code"));
    assert!(content_str.contains("strikethrough"));
}

#[test]
fn test_nested_formatting() {
    let content = "Nested: ***bold and italic*** together.";

    let parsed = parse_note(content, "test.md").unwrap();
    assert!(!parsed.blocks.is_empty());

    // Content should be preserved
    let content_str = parsed.blocks[0].content.to_lowercase();
    assert!(content_str.contains("italic"));
}

#[test]
fn test_unclosed_formatting_markers() {
    // Parser should handle unclosed markers gracefully
    let content = "Unclosed **bold marker";

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty());
}

#[test]
fn test_inline_code_with_backticks_inside() {
    // Code with backticks requires escaping or double backticks
    let content = "Inline code: ``code with ` backtick``";

    let parsed = parse_note(content, "test.md").unwrap();
    assert!(!parsed.blocks.is_empty());
}
