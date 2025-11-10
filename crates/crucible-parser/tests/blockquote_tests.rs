use crucible_parser::{CrucibleParser, MarkdownParserImplementation};
use std::path::Path;

#[tokio::test]
async fn test_blockquote_extraction() {
    let content = r#"
> This is a regular blockquote
> It spans multiple lines

> [!note] This is a callout
> This is callout content
"#;

    let parser = CrucibleParser::with_default_extensions();
    let result = parser.parse_content(content, Path::new("test.md")).await.unwrap();

    // Should extract one blockquote and one callout
    assert_eq!(result.content.blockquotes.len(), 1);
    assert_eq!(result.callouts.len(), 1);

    let blockquote = &result.content.blockquotes[0];
    assert!(blockquote.content.contains("regular blockquote"));
    assert_eq!(blockquote.nested_level, 0);
}

#[tokio::test]
async fn test_nested_blockquotes() {
    let content = r#"
> Level 1 quote

>> Level 2 nested quote

>>> Level 3 nested quote
"#;

    let parser = CrucibleParser::with_default_extensions();
    let result = parser.parse_content(content, Path::new("test.md")).await.unwrap();

    assert_eq!(result.content.blockquotes.len(), 3);

    // Check nesting levels
    assert_eq!(result.content.blockquotes[0].nested_level, 0);
    assert_eq!(result.content.blockquotes[1].nested_level, 1);
    assert_eq!(result.content.blockquotes[2].nested_level, 2);
}

#[tokio::test]
async fn test_blockquote_not_callout() {
    let content = r#"
> This is a normal quote

> [!warning] This is a callout
> Callout content

> Another normal quote
"#;

    let parser = CrucibleParser::with_default_extensions();
    let result = parser.parse_content(content, Path::new("test.md")).await.unwrap();

    // Should have 2 blockquotes and 1 callout
    assert_eq!(result.content.blockquotes.len(), 2);
    assert_eq!(result.callouts.len(), 1);

    // Verify content
    assert!(result.content.blockquotes[0].content.contains("normal quote"));
    assert!(result.content.blockquotes[1].content.contains("Another normal quote"));
    assert!(result.callouts[0].content.contains("Callout content"));
}
