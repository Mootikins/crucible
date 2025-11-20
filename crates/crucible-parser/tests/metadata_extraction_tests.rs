//! Tests for structural metadata extraction during parsing
//!
//! Verifies that the parser correctly extracts all structural metadata
//! (word counts, element counts) without computing derived metadata.

use crucible_parser::{CrucibleParser, MarkdownParser};
use std::path::PathBuf;

#[tokio::test]
async fn test_parser_extracts_structural_metadata() {
    let markdown = r#"---
title: Test Note
tags: [rust, testing]
---

# Introduction

This is a test note with various elements.

## Code Example

```rust
fn main() {
    println!("Hello, world!");
}
```

## Lists

- Item 1
- Item 2
- Item 3

1. First
2. Second

Some paragraph text here.

[^1]: This is a footnote

Some text with a footnote reference[^1].
"#;

    let parser = CrucibleParser::new();
    let path = PathBuf::from("test.md");
    let parsed = parser.parse_content(markdown, &path).await.unwrap();

    // Verify structural metadata is extracted
    let meta = &parsed.metadata;

    // Word count should be computed from content
    assert!(meta.word_count > 0, "Word count should be extracted: got {}", meta.word_count);

    // Character count should be computed
    assert!(meta.char_count > 0, "Character count should be extracted: got {}", meta.char_count);

    // Element counts should be accurate
    assert_eq!(meta.heading_count, 3, "Should count all headings: got {}", meta.heading_count);
    assert_eq!(meta.code_block_count, 1, "Should count code blocks: got {}", meta.code_block_count);
    assert!(meta.list_count >= 2, "Should count lists (unordered + ordered): got {}", meta.list_count);
    assert!(meta.paragraph_count > 0, "Should count paragraphs: got {}", meta.paragraph_count);

    // Note: Callouts and LaTeX may require specific extensions to be enabled
    // We test those separately

    assert_eq!(meta.footnote_count, 1, "Should count footnotes: got {}", meta.footnote_count);
}

#[tokio::test]
async fn test_parser_metadata_no_derived_fields() {
    let markdown = "# Simple Note\n\nJust some text.";

    let parser = CrucibleParser::new();
    let path = PathBuf::from("test.md");
    let parsed = parser.parse_content(markdown, &path).await.unwrap();

    let meta = &parsed.metadata;

    // Parser should provide structural counts
    assert_eq!(meta.heading_count, 1);
    assert!(meta.word_count > 0);

    // Parser does NOT compute:
    // - Reading time (that's enrichment's job)
    // - Complexity score (that's enrichment's job)
    // - Language detection (that's enrichment's job)

    // The ParsedNoteMetadata struct only contains structural fields
}

#[tokio::test]
async fn test_metadata_extraction_performance() {
    // Test that metadata extraction adds minimal overhead
    let large_markdown = generate_large_markdown(100); // 100 paragraphs

    let parser = CrucibleParser::new();
    let path = PathBuf::from("test.md");

    let start = std::time::Instant::now();
    let parsed = parser.parse_content(&large_markdown, &path).await.unwrap();
    let duration = start.elapsed();

    // Metadata should be populated
    assert!(parsed.metadata.word_count > 0);
    assert!(parsed.metadata.paragraph_count >= 100);

    // Performance: should complete quickly (adjust threshold as needed)
    // This is a rough check - actual performance will depend on hardware
    assert!(duration.as_millis() < 1000, "Parsing should be fast even with metadata extraction");
}

#[tokio::test]
async fn test_empty_note_metadata() {
    let markdown = "";

    let parser = CrucibleParser::new();
    let path = PathBuf::from("empty.md");
    let parsed = parser.parse_content(markdown, &path).await.unwrap();

    let meta = &parsed.metadata;

    // Empty note should have zero counts
    assert_eq!(meta.word_count, 0);
    assert_eq!(meta.char_count, 0);
    assert_eq!(meta.heading_count, 0);
    assert_eq!(meta.code_block_count, 0);
    assert_eq!(meta.list_count, 0);
    assert_eq!(meta.paragraph_count, 0);
    assert_eq!(meta.callout_count, 0);
    assert_eq!(meta.latex_count, 0);
    assert_eq!(meta.footnote_count, 0);
}

#[tokio::test]
async fn test_metadata_counts_match_content() {
    let markdown = r#"# Heading 1
## Heading 2
### Heading 3

Paragraph 1

Paragraph 2

```rust
code
```

- List item

> [!tip] Callout
> Content
"#;

    let parser = CrucibleParser::new();
    let path = PathBuf::from("test.md");
    let parsed = parser.parse_content(markdown, &path).await.unwrap();

    let meta = &parsed.metadata;

    // Verify counts match actual content structure
    assert_eq!(meta.heading_count, parsed.content.headings.len());
    assert_eq!(meta.code_block_count, parsed.content.code_blocks.len());
    assert_eq!(meta.list_count, parsed.content.lists.len());
    assert_eq!(meta.paragraph_count, parsed.content.paragraphs.len());
    assert_eq!(meta.callout_count, parsed.callouts.len());
}

/// Helper function to generate large markdown for performance testing
fn generate_large_markdown(paragraph_count: usize) -> String {
    let mut markdown = String::from("# Test Document\n\n");

    for i in 0..paragraph_count {
        markdown.push_str(&format!(
            "This is paragraph {}. It contains some text for testing purposes. ",
            i
        ));
        markdown.push_str("Lorem ipsum dolor sit amet, consectetur adipiscing elit.\n\n");
    }

    markdown
}
