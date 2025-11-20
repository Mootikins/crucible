// Test PulldownParser integration with enhanced functionality
use crucible_parser::{PulldownParser, MarkdownParser};
use std::path::PathBuf;

#[tokio::test]
async fn test_pulldown_parser_integration() {
    let parser = PulldownParser::new();

    // Test with a small document that has tables, lists, and code blocks
    let test_content = r#"---
title: "Test Document"
tags: [test, markdown]
---

# Enhanced Parser Test

## Tables

| Feature | Status | Description |
|---------|--------|-------------|
| Tables | ✅ Working | Full GFM table support |
| Lists | ✅ Working | Nested and task lists |
| Code Blocks | ✅ Working | Language detection |
| Callouts | ✅ Working | Obsidian-style callouts |

## Lists

### Ordered List
1. First item
2. Second item
3. Third item

### Nested List
- Top level
  - Nested item 1
  - Nested item 2
- Another top level

### Task Lists
- [x] Completed task
- [ ] Pending task
- [ ] Another pending task

## Code Blocks

```rust
fn main() {
    println!("Hello, Rust!");
}
```

```javascript
function greet(name) {
    return `Hello, ${name}!`;
}
```

```sql
SELECT * FROM users WHERE active = true;
```

## Callouts

> [!note]
> This is a note callout
> With multiple lines

> [!warning] Important Warning
> This needs attention

## Conclusion

All enhanced features are working!
"#;

    let path = PathBuf::from("test.md");
    match parser.parse_content(test_content, &path).await {
        Ok(result) => {
            assert!(result.frontmatter.is_some(), "Frontmatter should be extracted");

            // Check tables
            assert!(!result.content.tables.is_empty(), "Should extract at least one table");
            let table = &result.content.tables[0];
            println!("   Table extracted: {} columns, {} rows", table.columns, table.rows);
            println!("   Table headers: {:?}", table.headers);
            println!("   Table raw content preview: {}", &table.raw_content[..200.min(table.raw_content.len())]);

            // Basic table functionality check (adjust expected values as needed)
            assert_eq!(table.columns, 3, "Table should have 3 columns");
            assert!(table.rows >= 4, "Table should have at least 4 data rows");
            assert_eq!(table.headers.len(), 3, "Table should have 3 headers");

            // Check lists
            assert!(!result.content.lists.is_empty(), "Should extract lists");

            // Check code blocks
            assert!(!result.content.code_blocks.is_empty(), "Should extract code blocks");

            // Verify specific languages were detected
            let languages: Vec<_> = result.content.code_blocks.iter()
                .filter_map(|cb| cb.language.as_ref())
                .collect();
            assert!(languages.contains(&&"rust".to_string()), "Should detect Rust code");
            assert!(languages.contains(&&"javascript".to_string()), "Should detect JavaScript code");
            assert!(languages.contains(&&"sql".to_string()), "Should detect SQL code");

            println!("✅ All PulldownParser integration tests passed!");
            println!("   Frontmatter: {}", result.frontmatter.is_some());
            println!("   Tables: {} extracted", result.content.tables.len());
            println!("   Lists: {} extracted", result.content.lists.len());
            println!("   Code blocks: {} extracted", result.content.code_blocks.len());
        }
        Err(e) => {
            panic!("Parsing failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_pulldown_parser_capabilities() {
    let parser = PulldownParser::new();
    let capabilities = parser.capabilities();

    assert_eq!(capabilities.name, "PulldownParser");
    assert!(capabilities.tables, "Should support tables");
    assert!(capabilities.code_blocks, "Should support code blocks");
    assert!(capabilities.callouts, "Should support callouts");
    assert!(capabilities.wikilinks, "Should support wikilinks");
}

#[tokio::test]
async fn test_pulldown_parser_file_validation() {
    let parser = PulldownParser::new();

    // Test valid markdown files
    assert!(parser.can_parse(PathBuf::from("test.md").as_path()));
    assert!(parser.can_parse(PathBuf::from("document.markdown").as_path()));

    // Test invalid files
    assert!(!parser.can_parse(PathBuf::from("test.txt").as_path()));
    assert!(!parser.can_parse(PathBuf::from("image.png").as_path()));
}