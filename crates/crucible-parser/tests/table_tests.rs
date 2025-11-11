use crucible_parser::types::{ParsedNote, Table};
use crucible_parser::BlockExtractor;
use std::path::PathBuf;

#[test]
fn test_table_extraction() {
    let content = r#"| Name | Age | Role |
|------|-----|------|
| Alice | 30 | Dev |
| Bob | 25 | Designer |"#;

    // Create a note with a table
    let mut doc = ParsedNote::new(PathBuf::from("test.md"));

    // Create table struct
    let headers = vec!["Name".to_string(), "Age".to_string(), "Role".to_string()];
    let table = Table::new(content.to_string(), headers.clone(), 3, 2, 0);

    doc.content.tables.push(table);

    // Extract blocks
    let extractor = BlockExtractor::new();
    let blocks = extractor.extract_blocks(&doc).unwrap();

    // Find table block
    let table_blocks: Vec<_> = blocks
        .iter()
        .filter(|b| matches!(b.block_type, crucible_parser::types::ASTBlockType::Table))
        .collect();

    assert_eq!(table_blocks.len(), 1, "Should have exactly 1 table block");

    let table_block = table_blocks[0];

    // Check metadata
    if let crucible_parser::types::ASTBlockMetadata::Table { rows, columns, headers: block_headers } = &table_block.metadata {
        assert_eq!(*rows, 2, "Should have 2 data rows");
        assert_eq!(*columns, 3, "Should have 3 columns");
        assert_eq!(block_headers.len(), 3, "Should have 3 headers");
        assert_eq!(block_headers[0], "Name");
        assert_eq!(block_headers[1], "Age");
        assert_eq!(block_headers[2], "Role");
    } else {
        panic!("Expected Table metadata");
    }

    // Check content
    assert!(table_block.content.contains("Alice"));
    assert!(table_block.content.contains("Bob"));
    assert!(table_block.content.contains("Dev"));
    assert!(table_block.content.contains("Designer"));
}

#[test]
fn test_table_struct() {
    let content = r#"| Header1 | Header2 |
|---------|---------|
| Data1   | Data2   |"#;

    let headers = vec!["Header1".to_string(), "Header2".to_string()];
    let table = Table::new(content.to_string(), headers.clone(), 2, 1, 0);

    assert_eq!(table.headers.len(), 2);
    assert_eq!(table.columns, 2);
    assert_eq!(table.rows, 1);
    assert_eq!(table.offset, 0);
    assert!(table.raw_content.contains("Header1"));
}

#[test]
fn test_empty_table() {
    let content = r#"| Col1 | Col2 |
|------|------|"#;

    let headers = vec!["Col1".to_string(), "Col2".to_string()];
    let table = Table::new(content.to_string(), headers, 2, 0, 0);

    assert_eq!(table.rows, 0, "Empty table should have 0 data rows");
    assert_eq!(table.columns, 2);
}
