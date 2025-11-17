//! Nested list tests
//!
//! Tests for parsing nested and complex list structures.

use crucible_parser::{CrucibleParser, MarkdownParserImplementation, ParsedNote};
use std::path::Path;

async fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse_content(content, Path::new(path)).await?)
}

#[tokio::test]
async fn test_simple_nested_list() {
    let content = r#"- Level 1
  - Level 2
    - Level 3
  - Level 2 again
- Back to Level 1
"#;

    let result = parse_note(content, "test.md").await;
    assert!(result.is_ok());

    let parsed = result.unwrap();

    // Should have list items
    assert!(!parsed.content.lists.is_empty());
}

#[tokio::test]
async fn test_mixed_ordered_unordered_nested() {
    let content = r#"1. First ordered
   - Nested unordered
   - Another unordered
2. Second ordered
   1. Nested ordered
   2. Another nested ordered
"#;

    let result = parse_note(content, "test.md").await;
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.content.lists.is_empty());
}

#[tokio::test]
async fn test_list_with_paragraphs() {
    let content = r#"- First item

  Continuation paragraph for first item

- Second item
"#;

    let result = parse_note(content, "test.md").await;
    assert!(result.is_ok());

    let parsed = result.unwrap();
    // Should have lists or paragraphs
    assert!(!parsed.content.lists.is_empty() || !parsed.content.paragraphs.is_empty());
}

#[tokio::test]
async fn test_list_with_code_block() {
    let content = r#"- List item with code:

  ```rust
  fn example() {}
  ```

- Next item
"#;

    let result = parse_note(content, "test.md").await;
    assert!(result.is_ok());

    let parsed = result.unwrap();
    // Should have lists and code blocks
    assert!(!parsed.content.lists.is_empty() || !parsed.content.code_blocks.is_empty());
}

#[tokio::test]
async fn test_deeply_nested_list() {
    let content = r#"- Level 1
  - Level 2
    - Level 3
      - Level 4
        - Level 5
          - Level 6
"#;

    let result = parse_note(content, "test.md").await;
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.content.lists.is_empty());
}

#[tokio::test]
async fn test_task_list() {
    let content = r#"- [ ] Unchecked task
- [x] Checked task
- [X] Also checked (capital X)
- Regular list item
"#;

    let result = parse_note(content, "test.md").await;
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.content.lists.is_empty());
}
