use crucible_parser::{CrucibleParser, MarkdownParserImplementation};
use std::path::Path;

#[tokio::test]
async fn test_horizontal_rule_extraction() {
    let content = r#"
Section 1

---

Section 2

***

Section 3
"#;

    let parser = CrucibleParser::with_default_extensions();
    let result = parser.parse_content(content, Path::new("test.md")).await.unwrap();

    assert_eq!(
        result.content.horizontal_rules.len(),
        2,
        "Should extract 2 horizontal rules"
    );

    // Note: pulldown-cmark doesn't expose the original characters used for horizontal rules,
    // so all horizontal rules are normalized to "---" with "dash" style
    let hr1 = &result.content.horizontal_rules[0];
    assert_eq!(hr1.style, "dash");
    assert_eq!(hr1.raw_content, "---");

    let hr2 = &result.content.horizontal_rules[1];
    assert_eq!(hr2.style, "dash");
    assert_eq!(hr2.raw_content, "---");
}

#[tokio::test]
async fn test_horizontal_rule_with_underscores() {
    let content = r#"
Title

___

Content
"#;

    let parser = CrucibleParser::with_default_extensions();
    let result = parser.parse_content(content, Path::new("test.md")).await.unwrap();

    assert_eq!(
        result.content.horizontal_rules.len(),
        1,
        "Should extract 1 horizontal rule"
    );

    // Note: pulldown-cmark normalizes all horizontal rules to the same representation
    let hr = &result.content.horizontal_rules[0];
    assert_eq!(hr.style, "dash");
    assert_eq!(hr.raw_content, "---");
}

#[tokio::test]
async fn test_horizontal_rule_style_detection() {
    use crucible_parser::types::HorizontalRule;

    assert_eq!(HorizontalRule::detect_style("---"), "dash");
    assert_eq!(HorizontalRule::detect_style("***"), "asterisk");
    assert_eq!(HorizontalRule::detect_style("___"), "underscore");
    assert_eq!(HorizontalRule::detect_style(""), "unknown");
}

#[tokio::test]
async fn test_horizontal_rule_in_complex_document() {
    let content = r#"# Main Title

First paragraph with content.

---

## Section 1

Some content here.

***

## Section 2

More content.

___

### Subsection

Final content.
"#;

    let parser = CrucibleParser::with_default_extensions();
    let result = parser.parse_content(content, Path::new("test.md")).await.unwrap();

    assert_eq!(
        result.content.horizontal_rules.len(),
        3,
        "Should extract 3 horizontal rules"
    );

    // Note: All horizontal rules are normalized by pulldown-cmark
    assert_eq!(result.content.horizontal_rules[0].style, "dash");
    assert_eq!(result.content.horizontal_rules[1].style, "dash");
    assert_eq!(result.content.horizontal_rules[2].style, "dash");
}
