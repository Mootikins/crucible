//! Tests for heading hierarchy tracking in block extraction
//!
//! This module tests that the BlockExtractor correctly assigns parent_block_id
//! and depth to blocks based on heading hierarchy.

use crucible_parser::types::ParsedNote;
use crucible_parser::{BlockExtractor, MarkdownParser};
use std::path::{Path, PathBuf};

#[tokio::test]
async fn test_single_heading_with_paragraph() {
    let content = r#"# Top Heading

This is a paragraph under the heading.

This is another paragraph under the same heading.
"#;

    let parser = crucible_parser::CrucibleParser::with_block_processing();
    let parsed = parser
        .parse_content(content, Path::new("test.md"))
        .await
        .unwrap();

    let extractor = BlockExtractor::new();
    let blocks = extractor.extract_blocks(&parsed).unwrap();

    // Find the heading block
    let heading = blocks.iter().find(|b| b.is_heading()).unwrap();

    // Heading should have depth 0 and no parent (it's top-level)
    assert_eq!(heading.depth, Some(0));
    assert_eq!(heading.parent_block_id, None);

    // Find paragraph blocks
    let paragraphs: Vec<_> = blocks.iter().filter(|b| !b.is_heading()).collect();
    assert!(paragraphs.len() >= 1, "Expected at least one paragraph");

    // All paragraphs should have the heading as parent
    for para in paragraphs {
        assert!(
            para.parent_block_id.is_some(),
            "Paragraph should have parent"
        );
        assert_eq!(
            para.depth,
            Some(1),
            "Paragraph under H1 should have depth 1"
        );
    }
}

#[tokio::test]
async fn test_nested_headings_h1_h2() {
    let content = r#"# Level 1

Paragraph under H1.

## Level 2

Paragraph under H2.
"#;

    let parser = crucible_parser::CrucibleParser::with_block_processing();
    let parsed = parser
        .parse_content(content, Path::new("test.md"))
        .await
        .unwrap();

    let extractor = BlockExtractor::new();
    let blocks = extractor.extract_blocks(&parsed).unwrap();

    // Find headings
    let headings: Vec<_> = blocks.iter().filter(|b| b.is_heading()).collect();
    assert_eq!(headings.len(), 2, "Expected H1 and H2");

    let h1 = headings
        .iter()
        .find(|h| h.heading_level() == Some(1))
        .unwrap();
    let h2 = headings
        .iter()
        .find(|h| h.heading_level() == Some(2))
        .unwrap();

    // H1 should be top-level
    assert_eq!(h1.depth, Some(0));
    assert_eq!(h1.parent_block_id, None);

    // H2 should have H1 as parent
    assert_eq!(h2.depth, Some(1));
    assert!(h2.parent_block_id.is_some(), "H2 should have H1 as parent");
}

#[tokio::test]
async fn test_nested_headings_h1_h2_h3() {
    let content = r#"# Level 1

## Level 2

### Level 3

Content under H3.
"#;

    let parser = crucible_parser::CrucibleParser::with_block_processing();
    let parsed = parser
        .parse_content(content, Path::new("test.md"))
        .await
        .unwrap();

    let extractor = BlockExtractor::new();
    let blocks = extractor.extract_blocks(&parsed).unwrap();

    // Find headings
    let headings: Vec<_> = blocks.iter().filter(|b| b.is_heading()).collect();
    assert_eq!(headings.len(), 3, "Expected H1, H2, and H3");

    let h1 = headings
        .iter()
        .find(|h| h.heading_level() == Some(1))
        .unwrap();
    let h2 = headings
        .iter()
        .find(|h| h.heading_level() == Some(2))
        .unwrap();
    let h3 = headings
        .iter()
        .find(|h| h.heading_level() == Some(3))
        .unwrap();

    // H1 should be top-level
    assert_eq!(h1.depth, Some(0));
    assert_eq!(h1.parent_block_id, None);

    // H2 should have depth 1, parent H1
    assert_eq!(h2.depth, Some(1));
    assert!(h2.parent_block_id.is_some());

    // H3 should have depth 2, parent H2
    assert_eq!(h3.depth, Some(2));
    assert!(h3.parent_block_id.is_some());
}

#[tokio::test]
async fn test_skipped_heading_level() {
    // Test H1 → H3 (skipped H2)
    let content = r#"# Level 1

### Level 3

Content under H3.
"#;

    let parser = crucible_parser::CrucibleParser::with_block_processing();
    let parsed = parser
        .parse_content(content, Path::new("test.md"))
        .await
        .unwrap();

    let extractor = BlockExtractor::new();
    let blocks = extractor.extract_blocks(&parsed).unwrap();

    let headings: Vec<_> = blocks.iter().filter(|b| b.is_heading()).collect();
    assert_eq!(headings.len(), 2, "Expected H1 and H3");

    let h1 = headings
        .iter()
        .find(|h| h.heading_level() == Some(1))
        .unwrap();
    let h3 = headings
        .iter()
        .find(|h| h.heading_level() == Some(3))
        .unwrap();

    // H1 should be top-level
    assert_eq!(h1.depth, Some(0));
    assert_eq!(h1.parent_block_id, None);

    // H3 should have H1 as direct parent (we don't create virtual H2)
    assert_eq!(h3.depth, Some(1), "H3 under H1 should have depth 1");
    assert!(h3.parent_block_id.is_some(), "H3 should have H1 as parent");
}

#[tokio::test]
async fn test_multiple_h1_sections() {
    let content = r#"# Section 1

Content in section 1.

# Section 2

Content in section 2.
"#;

    let parser = crucible_parser::CrucibleParser::with_block_processing();
    let parsed = parser
        .parse_content(content, Path::new("test.md"))
        .await
        .unwrap();

    let extractor = BlockExtractor::new();
    let blocks = extractor.extract_blocks(&parsed).unwrap();

    let headings: Vec<_> = blocks.iter().filter(|b| b.is_heading()).collect();
    assert_eq!(headings.len(), 2, "Expected two H1 headings");

    // Both H1s should be top-level
    for h1 in headings {
        assert_eq!(h1.heading_level(), Some(1));
        assert_eq!(h1.depth, Some(0));
        assert_eq!(h1.parent_block_id, None);
    }
}

#[tokio::test]
async fn test_blocks_without_headings() {
    // Note preamble before any headings
    let content = r#"This is the note preamble.

More preamble content.

# First Heading

Now we have a heading.
"#;

    let parser = crucible_parser::CrucibleParser::with_block_processing();
    let parsed = parser
        .parse_content(content, Path::new("test.md"))
        .await
        .unwrap();

    let extractor = BlockExtractor::new();
    let blocks = extractor.extract_blocks(&parsed).unwrap();

    // Find blocks before the first heading
    let heading_idx = blocks.iter().position(|b| b.is_heading()).unwrap();
    let preamble_blocks = &blocks[..heading_idx];

    // Preamble blocks should have no parent and depth 0
    for block in preamble_blocks {
        assert_eq!(
            block.parent_block_id, None,
            "Preamble should have no parent"
        );
        assert_eq!(block.depth, Some(0), "Preamble should have depth 0");
    }
}
