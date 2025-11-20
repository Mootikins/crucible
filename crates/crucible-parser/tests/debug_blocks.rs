//! Debug test to check if blocks are being extracted

use crucible_parser::{BlockExtractor, MarkdownParser};
use std::path::Path;

#[tokio::test]
async fn debug_block_extraction() {
    let content = "# Test Heading

Some paragraph.

## Subheading

More text.
";

    let parser = crucible_parser::CrucibleParser::with_block_processing();
    let parsed = parser.parse_content(content, Path::new("test.md")).await.unwrap();

    println!("Parsed note:");
    println!("  - path: {:?}", parsed.path);
    println!("  - headings count: {}", parsed.content.headings.len());
    println!("  - block_hashes count: {}", parsed.block_hashes.len());

    for (i, heading) in parsed.content.headings.iter().enumerate() {
        println!("  - Heading {}: level={}, text={:?}", i, heading.level, heading.text);
    }

    let extractor = BlockExtractor::new();
    let blocks = extractor.extract_blocks(&parsed).unwrap();

    println!("\nExtracted blocks: {}", blocks.len());
    for (i, block) in blocks.iter().enumerate() {
        println!("  - Block {}: type={:?}, is_heading={}, level={:?}, parent={:?}, depth={:?}",
            i, block.block_type, block.is_heading(), block.heading_level(), block.parent_block_id, block.depth);
    }
}
