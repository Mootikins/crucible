use std::path::Path;

use crucible_core::parser::{CrucibleParser, MarkdownItParser, MarkdownParser};
use tempfile::tempdir;
use tokio::fs;

/// Asserts the behavioral contract that ALL MarkdownParser implementations must satisfy.
/// Some optional fields (like plain_text) may not be populated by all parsers — test those
/// separately per implementation in the extended contract below.
async fn assert_markdown_parser_contract(parser: &dyn MarkdownParser) {
    assert!(parser.can_parse(Path::new("note.md")));
    assert!(parser.can_parse(Path::new("note.markdown")));
    assert!(!parser.can_parse(Path::new("note.txt")));

    let source_path = Path::new("contract.md");
    let content = "# Contract Title\n\nSee [[Target]] and #contract_tag.";
    let parsed = parser
        .parse_content(content, source_path)
        .await
        .expect("parse_content should succeed for valid markdown");

    assert_eq!(parsed.path, source_path);
    // word_count > 0 is a universal contract (all parsers must count words)
    assert!(
        parsed.content.word_count > 0,
        "parsed note should report words"
    );

    let dir = tempdir().expect("tempdir should be created");
    let file_path = dir.path().join("from_file.md");
    fs::write(&file_path, content)
        .await
        .expect("test markdown file should be written");

    let from_file = parser
        .parse_file(&file_path)
        .await
        .expect("parse_file should succeed for existing markdown files");

    assert_eq!(from_file.path, file_path);

    let capabilities = parser.capabilities();
    assert!(!capabilities.name.is_empty());
    assert!(!capabilities.extensions.is_empty());
    assert!(
        capabilities.extensions.contains(&"md"),
        "parser must advertise markdown support"
    );
}

/// Extended contract for parsers that populate plain_text (e.g. CrucibleParser).
async fn assert_plain_text_contract(parser: &dyn MarkdownParser) {
    let source_path = Path::new("contract.md");
    let content = "# Contract Title\n\nSee [[Target]] and #contract_tag.";
    let parsed = parser
        .parse_content(content, source_path)
        .await
        .expect("parse_content should succeed");
    assert!(
        parsed.content.plain_text.contains("Contract Title"),
        "parser should retain source content in plain_text"
    );
}

#[tokio::test]
async fn contract_crucible_parser_satisfies_markdown_parser_contract() {
    let parser = CrucibleParser::new();
    assert_markdown_parser_contract(&parser).await;
}

#[tokio::test]
async fn contract_crucible_parser_populates_plain_text() {
    let parser = CrucibleParser::new();
    assert_plain_text_contract(&parser).await;
}

#[tokio::test]
async fn contract_markdown_it_parser_satisfies_markdown_parser_contract() {
    let parser = MarkdownItParser::new();
    assert_markdown_parser_contract(&parser).await;
}

#[tokio::test]
async fn contract_parse_file_returns_error_for_missing_path() {
    let missing = Path::new("definitely_missing_contract_file.md");

    let crucible = CrucibleParser::new();
    let markdown_it = MarkdownItParser::new();

    let crucible_result = crucible.parse_file(missing).await;
    let markdown_it_result = markdown_it.parse_file(missing).await;

    assert!(
        crucible_result.is_err(),
        "parse_file should return an error for missing files"
    );
    assert!(
        markdown_it_result.is_err(),
        "all parser implementations should return an error for missing files"
    );
}
