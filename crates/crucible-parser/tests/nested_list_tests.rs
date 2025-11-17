//! Nested list tests
//!
//! Tests for parsing nested and complex list structures.

use crucible_parser::{CrucibleParser, MarkdownParserImplementation, ParsedNote};

fn parse_note(content: &str, path: &str) -> Result<ParsedNote, Box<dyn std::error::Error>> {
    let parser = CrucibleParser::with_default_extensions();
    Ok(parser.parse(content, path)?)
}

#[test]
fn test_simple_nested_list() {
    let content = r#"- Level 1
  - Level 2
    - Level 3
  - Level 2 again
- Back to Level 1
"#;

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty());

    // Should have list items
    let list_blocks: Vec<_> = parsed.blocks.iter().filter(|b| b.is_list()).collect();
    assert!(!list_blocks.is_empty());
}

#[test]
fn test_mixed_ordered_unordered_nested() {
    let content = r#"1. First ordered
   - Nested unordered
   - Another unordered
2. Second ordered
   1. Nested ordered
   2. Another nested ordered
"#;

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    let list_blocks: Vec<_> = parsed.blocks.iter().filter(|b| b.is_list()).collect();
    assert!(!list_blocks.is_empty());
}

#[test]
fn test_list_with_paragraphs() {
    let content = r#"- First item

  Continuation paragraph for first item

- Second item
"#;

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty());
}

#[test]
fn test_list_with_code_block() {
    let content = r#"- List item with code:

  ```rust
  fn example() {}
  ```

- Next item
"#;

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert!(!parsed.blocks.is_empty());
}

#[test]
fn test_deeply_nested_list() {
    let content = r#"- Level 1
  - Level 2
    - Level 3
      - Level 4
        - Level 5
          - Level 6
"#;

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    let list_blocks: Vec<_> = parsed.blocks.iter().filter(|b| b.is_list()).collect();
    assert!(!list_blocks.is_empty());
}

#[test]
fn test_task_list() {
    let content = r#"- [ ] Unchecked task
- [x] Checked task
- [X] Also checked (capital X)
- Regular list item
"#;

    let result = parse_note(content, "test.md");
    assert!(result.is_ok());

    let parsed = result.unwrap();
    let list_blocks: Vec<_> = parsed.blocks.iter().filter(|b| b.is_list()).collect();
    assert!(!list_blocks.is_empty());
}
