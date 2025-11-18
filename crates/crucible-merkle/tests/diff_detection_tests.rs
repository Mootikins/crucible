//! Merkle tree diff detection tests
//!
//! Tests for granular change detection, diff algorithms, and incremental updates.

use crucible_merkle::HybridMerkleTree;
use crucible_parser::{CrucibleParser, MarkdownParserImplementation, ParsedNote};
use std::path::Path;

async fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse_content(content, Path::new(path)).await?)
}

#[tokio::test]
async fn test_detect_new_section_addition() {
    let original = "# Section 1\n\nContent 1";
    let modified = "# Section 1\n\nContent 1\n\n# Section 2\n\nContent 2";

    let parsed1 = parse_note(original, "test.md").await.unwrap();
    let parsed2 = parse_note(modified, "test.md").await.unwrap();

    let tree1 = HybridMerkleTree::from_document(&parsed1);
    let tree2 = HybridMerkleTree::from_document(&parsed2);

    // Root hashes should differ
    assert_ne!(tree1.root_hash, tree2.root_hash);

    // Tree2 should have more sections
    assert!(tree2.section_count() > tree1.section_count());
}

#[tokio::test]
async fn test_detect_section_content_change() {
    let original = "# Section 1\n\nOriginal content";
    let modified = "# Section 1\n\nModified content";

    let parsed1 = parse_note(original, "test.md").await.unwrap();
    let parsed2 = parse_note(modified, "test.md").await.unwrap();

    let tree1 = HybridMerkleTree::from_document(&parsed1);
    let tree2 = HybridMerkleTree::from_document(&parsed2);

    // Root hashes should differ
    assert_ne!(tree1.root_hash, tree2.root_hash);
}

#[tokio::test]
async fn test_no_change_same_hash() {
    let content = "# Section 1\n\nSame content";

    let parsed1 = parse_note(content, "test.md").await.unwrap();
    let parsed2 = parse_note(content, "test.md").await.unwrap();

    let tree1 = HybridMerkleTree::from_document(&parsed1);
    let tree2 = HybridMerkleTree::from_document(&parsed2);

    // Root hashes should be identical
    assert_eq!(tree1.root_hash, tree2.root_hash);
}

#[tokio::test]
async fn test_whitespace_only_change() {
    let original = "# Section 1\n\nContent";
    let modified = "# Section 1\n\n\nContent";  // Extra newline

    let parsed1 = parse_note(original, "test.md").await.unwrap();
    let parsed2 = parse_note(modified, "test.md").await.unwrap();

    let tree1 = HybridMerkleTree::from_document(&parsed1);
    let tree2 = HybridMerkleTree::from_document(&parsed2);

    // May or may not differ depending on normalization
    // Just verify trees are created successfully
    assert!(!tree1.root_hash.is_zero());
    assert!(!tree2.root_hash.is_zero());
}

#[tokio::test]
async fn test_section_deletion_detection() {
    let original = "# Section 1\n\nContent 1\n\n# Section 2\n\nContent 2";
    let modified = "# Section 1\n\nContent 1";

    let parsed1 = parse_note(original, "test.md").await.unwrap();
    let parsed2 = parse_note(modified, "test.md").await.unwrap();

    let tree1 = HybridMerkleTree::from_document(&parsed1);
    let tree2 = HybridMerkleTree::from_document(&parsed2);

    // Root hashes should differ
    assert_ne!(tree1.root_hash, tree2.root_hash);

    // Tree2 should have fewer sections
    assert!(tree2.section_count() < tree1.section_count());
}

#[tokio::test]
async fn test_section_reordering_detection() {
    let original = "# Section A\n\nContent A\n\n# Section B\n\nContent B";
    let reordered = "# Section B\n\nContent B\n\n# Section A\n\nContent A";

    let parsed1 = parse_note(original, "test.md").await.unwrap();
    let parsed2 = parse_note(reordered, "test.md").await.unwrap();

    let tree1 = HybridMerkleTree::from_document(&parsed1);
    let tree2 = HybridMerkleTree::from_document(&parsed2);

    // Root hashes should differ due to order change
    assert_ne!(tree1.root_hash, tree2.root_hash);
}

#[tokio::test]
async fn test_nested_section_change_detection() {
    let original = r#"# Level 1

Content

## Level 2

Nested content
"#;

    let modified = r#"# Level 1

Content

## Level 2

Modified nested content
"#;

    let parsed1 = parse_note(original, "test.md").await.unwrap();
    let parsed2 = parse_note(modified, "test.md").await.unwrap();

    let tree1 = HybridMerkleTree::from_document(&parsed1);
    let tree2 = HybridMerkleTree::from_document(&parsed2);

    // Change in nested section should propagate to root
    assert_ne!(tree1.root_hash, tree2.root_hash);
}

#[tokio::test]
async fn test_multiple_sections_same_content() {
    let content = r#"# Section 1

Same content

# Section 2

Same content

# Section 3

Same content
"#;

    let parsed = parse_note(content, "test.md").await.unwrap();
    let tree = HybridMerkleTree::from_document(&parsed);

    // Should handle duplicate content in different sections
    assert!(!tree.root_hash.is_zero());
    // Note: 4 sections = 1 root section + 3 heading sections
    assert_eq!(tree.section_count(), 4);
}

#[tokio::test]
async fn test_empty_sections() {
    let content = r#"# Section 1

# Section 2

# Section 3

Content in last section
"#;

    let parsed = parse_note(content, "test.md").await.unwrap();
    let tree = HybridMerkleTree::from_document(&parsed);

    // Should handle empty sections
    assert!(!tree.root_hash.is_zero());
}

#[tokio::test]
async fn test_hash_stability_across_rebuilds() {
    let content = "# Section\n\nContent";

    let parsed1 = parse_note(content, "test.md").await.unwrap();
    let tree1 = HybridMerkleTree::from_document(&parsed1);

    let parsed2 = parse_note(content, "test.md").await.unwrap();
    let tree2 = HybridMerkleTree::from_document(&parsed2);

    // Rebuilding from same content should give same hash
    assert_eq!(tree1.root_hash, tree2.root_hash);
}
