//! Basic Phase 1B Test
//!
//! Simple test to verify Phase 1B features are working without complex regex

use crucible_parser::{CrucibleParser, MarkdownParserImplementation};
use std::path::Path;

#[tokio::test]
async fn test_basic_phase1b_features() {
    let parser = CrucibleParser::with_default_extensions();

    let simple_content = r#"---
title: Simple Test
tags: [test, basic]
---

# Simple Phase 1B Test

> [!NOTE] This is a simple note callout.

Some inline math: $x + y = z$.

Hashtags: #simple #test

- [ ] Task item
- [x] Completed task

Footnote reference[^1].

[^1]: Footnote definition.
"#;

    let document = parser.parse_content(simple_content, Path::new("simple.md")).await
        .expect("Should parse simple content");

    // Basic checks
    assert!(!document.path.as_os_str().is_empty());
    println!("✅ Basic parsing test passed!");
    println!("   LaTeX expressions: {}", document.latex_expressions.len());
    println!("   Callouts: {}", document.callouts.len());
    println!("   Tags: {}", document.tags.len());
    println!("   Task lists: {}", document.content.lists.len());
    println!("   Footnote refs: {}", document.footnotes.references.len());
    println!("   Footnote defs: {}", document.footnotes.definitions.len());

    // Check we found some content
    assert!(document.latex_expressions.len() > 0, "Should find LaTeX");
    assert!(document.callouts.len() > 0, "Should find callouts");
    assert!(document.tags.len() > 0, "Should find tags");
    assert!(document.content.lists.len() > 0, "Should find lists");
}