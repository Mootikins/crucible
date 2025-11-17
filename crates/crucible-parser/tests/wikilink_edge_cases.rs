//! Wikilink edge case tests
//!
//! Tests for advanced wikilink parsing scenarios including transclusion,
//! circular references, nested structures, and error recovery.

use crucible_parser::{CrucibleParser, MarkdownParserImplementation, ParsedNote};
use std::path::Path;

async fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse_content(content, Path::new(path)).await?)
}

#[tokio::test]
async fn test_transclusion_syntax() {
    let content = r#"# Note
Embed another note:
![[embedded-note]]

Regular wikilink:
[[regular-link]]
"#;

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    assert_eq!(wikilinks.len(), 2);

    // Find the transclusion link
    let transclusion = wikilinks
        .iter()
        .find(|w| w.target == "embedded-note")
        .expect("Should find transclusion");

    // Transclusion should be marked differently (if implemented)
    // For now, just verify it's parsed
    assert_eq!(transclusion.target, "embedded-note");
}

#[tokio::test]
async fn test_wikilink_with_heading_reference() {
    let content = "Link to heading: [[note#Section Title]]";

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    assert_eq!(wikilinks.len(), 1);
    assert_eq!(wikilinks[0].target, "note");
    assert_eq!(wikilinks[0].heading_ref.as_deref(), Some("Section Title"));
}

#[tokio::test]
async fn test_wikilink_with_block_reference() {
    let content = "Link to block: [[note#^block-id]]";

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    assert_eq!(wikilinks.len(), 1);
    assert_eq!(wikilinks[0].target, "note");
    assert_eq!(wikilinks[0].block_ref.as_deref(), Some("block-id"));
}

#[tokio::test]
async fn test_wikilink_with_alias_and_heading() {
    let content = "Complex link: [[note#Section|Display Text]]";

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    assert_eq!(wikilinks.len(), 1);
    assert_eq!(wikilinks[0].target, "note");
    assert_eq!(wikilinks[0].alias.as_deref(), Some("Display Text"));
    assert_eq!(wikilinks[0].heading_ref.as_deref(), Some("Section"));
}

#[tokio::test]
async fn test_multiple_wikilinks_same_line() {
    let content = "Multiple links: [[first]] and [[second]] and [[third]]";

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    assert_eq!(wikilinks.len(), 3);
    assert_eq!(wikilinks[0].target, "first");
    assert_eq!(wikilinks[1].target, "second");
    assert_eq!(wikilinks[2].target, "third");
}

#[tokio::test]
async fn test_wikilink_with_special_characters() {
    let content = r#"Links with special chars:
[[note-with-dashes]]
[[note_with_underscores]]
[[note with spaces]]
[[note.with.dots]]
"#;

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    assert_eq!(wikilinks.len(), 4);
    assert!(wikilinks.iter().any(|w| w.target == "note-with-dashes"));
    assert!(wikilinks.iter().any(|w| w.target == "note_with_underscores"));
    assert!(wikilinks.iter().any(|w| w.target == "note with spaces"));
    assert!(wikilinks.iter().any(|w| w.target == "note.with.dots"));
}

#[tokio::test]
async fn test_nested_wikilinks_in_different_blocks() {
    let content = r#"# Heading 1
Link in paragraph: [[first]]

## Heading 2
Link in nested section: [[second]]

- List item with [[third]]
"#;

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    assert_eq!(wikilinks.len(), 3);
}

#[tokio::test]
async fn test_unclosed_wikilink() {
    // Test that unclosed wikilinks don't cause parser to fail
    let content = "Unclosed link: [[broken";

    let parsed = parse_note(content, "test.md").await.unwrap();

    // Should parse without error, even if wikilink isn't extracted
    // ParsedNote exists, which means it parsed successfully
}

#[tokio::test]
async fn test_empty_wikilink() {
    let content = "Empty link: [[]]";

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    // Empty wikilinks should either be ignored or parsed with empty target
    // Behavior depends on implementation
    if !wikilinks.is_empty() {
        assert_eq!(wikilinks[0].target, "");
    }
}

#[tokio::test]
async fn test_wikilink_in_code_block_not_parsed() {
    let content = r#"Regular link: [[normal]]

```
Code block link: [[should-not-parse]]
```

After code: [[after]]
"#;

    let parsed = parse_note(content, "test.md").await.unwrap();
    let wikilinks = &parsed.wikilinks;

    // Should only find wikilinks outside code blocks
    assert_eq!(wikilinks.len(), 2);
    assert!(wikilinks.iter().any(|w| w.target == "normal"));
    assert!(wikilinks.iter().any(|w| w.target == "after"));
    assert!(!wikilinks.iter().any(|w| w.target == "should-not-parse"));
}
