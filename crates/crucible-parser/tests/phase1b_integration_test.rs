//! Phase 1B Integration Tests
//!
//! Comprehensive tests for all Phase 1B features:
//! - LaTeX mathematical expressions (inline $...$ and block $$...$$)
//! - Obsidian callouts (> [!type])
//! - Enhanced hashtags (#hashtag)
//! - Advanced task list parsing
//! - Footnote processing ([^ref] and ^inline^)

use crucible_parser::{CrucibleParser, MarkdownParserImplementation};
use std::path::Path;

/// Test that all Phase 1B features are working together
#[tokio::test]
async fn test_phase1b_comprehensive_integration() {
    let parser = CrucibleParser::with_default_extensions();

    // Parse the comprehensive Phase 1B test file
    let test_file_path = Path::new("/home/moot/crucible/examples/test-kiln/Phase1B-Integration-Test.md");
    assert!(test_file_path.exists(), "Phase 1B integration test file should exist");

    let document = parser.parse_file(test_file_path).await
        .expect("Should successfully parse Phase 1B integration test file");

    // Verify basic document structure
    assert!(!document.path.as_os_str().is_empty());
    assert!(document.frontmatter.is_some());

    // Verify LaTeX expressions are extracted
    assert!(!document.latex_expressions.is_empty(),
             "Should extract LaTeX mathematical expressions");
    println!("✅ Found {} LaTeX expressions", document.latex_expressions.len());

    // Verify both inline and block LaTeX
    let inline_latex = document.latex_expressions.iter()
        .any(|latex| !latex.is_block);
    let block_latex = document.latex_expressions.iter()
        .any(|latex| latex.is_block);
    assert!(inline_latex, "Should have inline LaTeX expressions");
    assert!(block_latex, "Should have block LaTeX expressions");

    // Verify callouts are extracted
    assert!(!document.callouts.is_empty(),
             "Should extract Obsidian callouts");
    println!("✅ Found {} callouts", document.callouts.len());

    // Verify different callout types
    let callout_types: std::collections::HashSet<_> = document.callouts.iter()
        .map(|c| c.callout_type.as_str())
        .collect();
    assert!(callout_types.contains("note"), "Should have NOTE callouts");
    assert!(callout_types.contains("warning"), "Should have WARNING callouts");

    // Verify enhanced tags are extracted
    assert!(!document.tags.is_empty(),
             "Should extract enhanced hashtags");
    println!("✅ Found {} tags", document.tags.len());

    // Verify specific expected tags
    let tag_names: std::collections::HashSet<_> = document.tags.iter()
        .map(|t| t.name.as_str())
        .collect();
    assert!(tag_names.contains("phase1b"), "Should have #phase1b tag");
    assert!(tag_names.contains("integration-testing"), "Should have #integration-testing tag");

    // Verify task lists are extracted from document content
    let task_lists_count = document.content.lists.iter()
        .filter(|list| list.items.iter().any(|item| item.task_status.is_some()))
        .count();
    assert!(task_lists_count > 0, "Should have task lists");
    println!("✅ Found {} task lists", task_lists_count);

    // Verify footnote processing
    assert!(!document.footnotes.references.is_empty() || !document.footnotes.definitions.is_empty(),
             "Should process footnotes");
    let footnote_count = document.footnotes.references.len() + document.footnotes.definitions.len();
    println!("✅ Found {} footnote items", footnote_count);

    // Validate specific LaTeX expressions from test file
    let einstein_found = document.latex_expressions.iter()
        .any(|latex| latex.expression.contains("E = mc^2"));
    assert!(einstein_found, "Should find Einstein's equation");

    let quadratic_found = document.latex_expressions.iter()
        .any(|latex| latex.expression.contains("frac{-b") && latex.expression.contains("sqrt{"));
    assert!(quadratic_found, "Should find quadratic formula");

    // Validate specific callout content
    let integration_note = document.callouts.iter()
        .find(|c| c.content.contains("Integration Test Overview"));
    assert!(integration_note.is_some(), "Should find integration test overview callout");

    println!("✅ Phase 1B comprehensive integration test passed!");
}

/// Test LaTeX expression parsing specifically
#[tokio::test]
async fn test_latex_expression_parsing() {
    let parser = CrucibleParser::with_default_extensions();

    let test_content = r#"
# LaTeX Test

Inline math: $E = mc^2$ and $x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}$.

Block math:
$$
\int_{0}^{\infty} e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$

Complex matrix:
$$
\begin{pmatrix}
a_{11} & a_{12} \\
a_{21} & a_{22}
\end{pmatrix}
$$
"#;

    let test_path = Path::new("test_latex.md");
    let document = parser.parse_content(test_content, test_path)
        .await.expect("Should parse LaTeX content");

    // Should find multiple LaTeX expressions
    assert!(document.latex_expressions.len() >= 4,
             "Should find at least 4 LaTeX expressions");

    // Verify inline expressions
    let inline_expressions: Vec<_> = document.latex_expressions.iter()
        .filter(|latex| !latex.is_block)
        .collect();
    assert!(inline_expressions.len() >= 2, "Should have at least 2 inline expressions");

    // Verify block expressions
    let block_expressions: Vec<_> = document.latex_expressions.iter()
        .filter(|latex| latex.is_block)
        .collect();
    assert!(block_expressions.len() >= 2, "Should have at least 2 block expressions");

    // Verify specific expressions
    let expressions: Vec<_> = document.latex_expressions.iter()
        .map(|latex| latex.expression.as_str())
        .collect();

    assert!(expressions.iter().any(|expr| expr.contains("E = mc^2")),
           "Should find Einstein's equation");
    assert!(expressions.iter().any(|expr| expr.contains("frac{-b")),
           "Should find quadratic formula");
    assert!(expressions.iter().any(|expr| expr.contains("int_{0}^{\\infty}")),
           "Should find integral");
    assert!(expressions.iter().any(|expr| expr.contains("begin{pmatrix}")),
           "Should find matrix");

    println!("✅ LaTeX parsing test passed! Found {} expressions", document.latex_expressions.len());
}

/// Test Obsidian callout parsing
#[tokio::test]
async fn test_obsidian_callout_parsing() {
    let parser = CrucibleParser::with_default_extensions();

    let test_content = r#"
# Callout Test

> [!NOTE] This is a note
> Multi-line note content here.

> [!WARNING] Warning message
> - List item in warning
> - Another item

> [!TIP] **Pro tip**: Use callouts to organize information.

> [!DANGER] ⚠️ Critical information here.

> [!CUSTOM] Custom callout type should work too.
"#;

    let test_path = Path::new("test_callouts.md");
    let document = parser.parse_content(test_content, test_path)
        .await.expect("Should parse callout content");

    // Should find multiple callouts
    assert!(document.callouts.len() >= 5,
             "Should find at least 5 callouts");

    // Verify specific callout types
    let callout_types: std::collections::HashMap<_, usize> = document.callouts.iter()
        .map(|c| (c.callout_type.as_str(), 1))
        .fold(std::collections::HashMap::new(), |mut acc, (ty, count)| {
            *acc.entry(ty).or_insert(0) += count;
            acc
        });

    assert!(callout_types.contains_key("note"), "Should have NOTE callout");
    assert!(callout_types.contains_key("warning"), "Should have WARNING callout");
    assert!(callout_types.contains_key("tip"), "Should have TIP callout");
    assert!(callout_types.contains_key("danger"), "Should have DANGER callout");
    assert!(callout_types.contains_key("custom"), "Should have CUSTOM callout");

    // Verify callout content
    let note_callout = document.callouts.iter()
        .find(|c| c.callout_type == "note");
    assert!(note_callout.is_some(), "Should find note callout");
    assert!(note_callout.unwrap().content.contains("Multi-line note"),
           "Note should contain expected content");

    let tip_callout = document.callouts.iter()
        .find(|c| c.callout_type == "tip");
    assert!(tip_callout.is_some(), "Should find tip callout");
    assert!(tip_callout.unwrap().content.contains("Pro tip"),
           "Tip should contain expected content");

    println!("✅ Callout parsing test passed! Found {} callouts", document.callouts.len());
}

/// Test enhanced hashtag parsing
#[tokio::test]
async fn test_enhanced_hashtag_parsing() {
    let parser = CrucibleParser::with_default_extensions();

    let test_content = r#"
# Hashtag Test

Simple #hashtags and #complex-tags with hyphens.

Numbers in #123tag and #test456.

Mixed case: #JavaScript, #Python, #RustLang.

Hierarchical: #Frontend/Components, #Backend/API.

Long descriptive: #MathematicalExpressionParsing, #ObsidianCalloutRendering.

Should ignore code like `#not-a-tag` but process #real-tag.

Should ignore URLs like https://example.com#section.

Edge cases: #A, #B, #X and #snake_case_example.
"#;

    let test_path = Path::new("test_hashtags.md");
    let document = parser.parse_content(test_content, test_path)
        .await.expect("Should parse hashtag content");

    // Should find multiple hashtags
    assert!(document.tags.len() >= 15,
             "Should find at least 15 hashtags");

    // Verify specific expected tags
    let tag_names: std::collections::HashSet<_> = document.tags.iter()
        .map(|t| t.name.as_str())
        .collect();

    assert!(tag_names.contains("hashtags"), "Should find #hashtags");
    assert!(tag_names.contains("complex-tags"), "Should find #complex-tags");
    assert!(tag_names.contains("123tag"), "Should find #123tag");
    assert!(tag_names.contains("JavaScript"), "Should find #JavaScript");
    assert!(tag_names.contains("Python"), "Should find #Python");
    assert!(tag_names.contains("Frontend/Components"), "Should find hierarchical tag");
    assert!(tag_names.contains("real-tag"), "Should find #real-tag");
    assert!(tag_names.contains("snake_case_example"), "Should find snake_case tag");

    // Should NOT find code-only tags or URL fragments
    assert!(!tag_names.contains("not-a-tag"), "Should not extract hashtags from code");

    // Verify tag positions
    let frontend_tag = document.tags.iter()
        .find(|t| t.name == "Frontend/Components");
    assert!(frontend_tag.is_some(), "Should find hierarchical tag");
    assert!(frontend_tag.unwrap().offset > 0, "Should have valid position");

    println!("✅ Hashtag parsing test passed! Found {} tags", document.tags.len());
}

/// Test advanced task list parsing
#[tokio::test]
async fn test_advanced_task_list_parsing() {
    let parser = CrucibleParser::with_default_extensions();

    let test_content = r#"
# Task List Test

- [ ] Simple pending task
- [x] Simple completed task
- [/] In-progress task
- [-] Cancelled task

* [ ] Asterisk style task
+ [ ] Plus style task

1. [ ] Numbered task 1
2. [x] Numbered task 2

Nested tasks:
- [ ] Parent task
  - [ ] First level subtask
    - [ ] Second level nested task
      - [x] Deeply nested completed task
  - [x] Another first level subtask

Mixed content:
- [ ] Task with **bold** and *italic* text
- [ ] Task with `inline code` and #hashtags
- [ ] Task with $x^2 + y^2 = z^2$ LaTeX
"#;

    let test_path = Path::new("test_tasks.md");
    let document = parser.parse_content(test_content, test_path)
        .await.expect("Should parse task list content");

    // Should find task lists
    let task_lists: Vec<_> = document.content.lists.iter()
        .filter(|list| list.items.iter().any(|item| item.task_status.is_some()))
        .collect();
    assert!(!task_lists.is_empty(), "Should find task lists");

    // Count total task items
    let total_tasks: usize = task_lists.iter()
        .map(|list| list.items.len())
        .sum();
    assert!(total_tasks >= 10, "Should find at least 10 task items");

    // Verify different task statuses
    let all_items: Vec<_> = task_lists.iter()
        .flat_map(|list| &list.items)
        .collect();

    let completed_tasks = all_items.iter()
        .filter(|item| matches!(item.task_status, Some(crucible_parser::TaskStatus::Completed)))
        .count();
    let pending_tasks = all_items.iter()
        .filter(|item| matches!(item.task_status, Some(crucible_parser::TaskStatus::Pending)))
        .count();

    assert!(completed_tasks > 0, "Should have completed tasks");
    assert!(pending_tasks > 0, "Should have pending tasks");

    // Verify nested structure
    let nested_tasks = all_items.iter()
        .filter(|item| item.level > 0)
        .count();
    assert!(nested_tasks > 0, "Should have nested tasks");

    // Verify mixed content in tasks
    let task_with_formatting = all_items.iter()
        .find(|item| item.content.contains("**bold**"));
    assert!(task_with_formatting.is_some(), "Should preserve formatting in tasks");

    let task_with_hashtag = all_items.iter()
        .find(|item| item.content.contains("#hashtags"));
    assert!(task_with_hashtag.is_some(), "Should extract hashtags from tasks");

    println!("✅ Task list parsing test passed! Found {} task items", total_tasks);
}

/// Test footnote processing
#[tokio::test]
async fn test_footnote_processing() {
    let parser = CrucibleParser::with_default_extensions();

    let test_content = r#"
# Footnote Test

This is the first footnote reference[^1]. Another footnote[^2] demonstrates multiple footnotes.

Footnotes with special characters[^special-chars] and mathematical expressions[^math-footnote].

Inline footnotes^[This is an inline footnote] can be mixed with regular references[^inline-mix].

Self-referential behavior[^self-ref] and duplicate references[^duplicate], [^duplicate].

## Footnote Definitions

[^1]: First footnote definition.
[^2]: Second footnote definition.
[^special-chars]: Special characters: !@#$%^&*()
[^math-footnote]: Mathematical footnote with $e^{i\pi} + 1 = 0$
[^self-ref]: Self-referential footnote content.
[^duplicate]: This footnote is referenced multiple times.
[^inline-mix]: Mixed footnote reference content.
"#;

    let test_path = Path::new("test_footnotes.md");
    let document = parser.parse_content(test_content, test_path)
        .await.expect("Should parse footnote content");

    // Should find footnote references and definitions
    assert!(!document.footnotes.references.is_empty(), "Should find footnote references");
    assert!(!document.footnotes.definitions.is_empty(), "Should find footnote definitions");

    let ref_count = document.footnotes.references.len();
    let def_count = document.footnotes.definitions.len();

    assert!(ref_count >= 7, "Should find at least 7 footnote references");
    assert!(def_count >= 7, "Should find at least 7 footnote definitions");

    // Verify specific footnote references
    let ref_names: std::collections::HashSet<_> = document.footnotes.references.iter()
        .map(|r| r.identifier.as_str())
        .collect();

    assert!(ref_names.contains("1"), "Should find footnote [^1]");
    assert!(ref_names.contains("2"), "Should find footnote [^2]");
    assert!(ref_names.contains("special-chars"), "Should find [^special-chars]");
    assert!(ref_names.contains("math-footnote"), "Should find [^math-footnote]");
    assert!(ref_names.contains("duplicate"), "Should find [^duplicate]");

    // Verify footnote definitions
    let def_names: std::collections::HashSet<_> = document.footnotes.definitions.values()
        .map(|d| d.identifier.as_str())
        .collect();

    assert!(def_names.contains("1"), "Should define footnote [^1]");
    assert!(def_names.contains("special-chars"), "Should define [^special-chars]");
    assert!(def_names.contains("duplicate"), "Should define [^duplicate]");

    // Verify duplicate reference handling
    let duplicate_refs = document.footnotes.references.iter()
        .filter(|r| r.identifier == "duplicate")
        .count();
    assert_eq!(duplicate_refs, 2, "Should handle duplicate references correctly");

    println!("✅ Footnote processing test passed! Found {} references and {} definitions",
             ref_count, def_count);
}

/// Test that parser capabilities include Phase 1B features
#[test]
fn test_parser_capabilities() {
    let parser = CrucibleParser::with_default_extensions();
    let capabilities = parser.capabilities();

    // Verify that all Phase 1B features are supported
    assert!(capabilities.tags, "Should support enhanced tags");
    assert!(capabilities.headings, "Should support headings for callouts");
    assert!(capabilities.code_blocks, "Should support code blocks");
    assert!(capabilities.full_content, "Should support full content parsing");

    // Verify extensions are loaded
    assert!(!capabilities.extensions.is_empty(), "Should have extensions loaded");

    println!("✅ Parser capabilities test passed! Extensions loaded: {}", capabilities.extensions.len());
}

/// Test Phase 1B feature integration in mixed content scenarios
#[tokio::test]
async fn test_mixed_phase1b_features() {
    let parser = CrucibleParser::with_default_extensions();

    let test_content = r#"
# Mixed Phase 1B Features

> [!NOTE] This document tests multiple Phase 1B features working together.
>
> Tasks for implementation:
> - [ ] Parse LaTeX expressions like $E = mc^2$
> - [x] Extract hashtags #latex #math
> - [ ] Process footnotes[^implementation]

The quadratic formula is $x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}$.

Advanced expression:
$$
\int_{0}^{\infty} e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$

> [!WARNING] Complex scenarios:
> - [ ] Test nested tasks with #hashtags and $latex$
>     - [ ] Validate footnotes in nested tasks[^nested]
> - [x] Complete #integration-testing

References for #Phase1B implementation[^phase1b-docs].

## Footnotes

[^implementation]: Implementation details for Phase 1B.
[^nested]: Nested task footnote reference.
[^phase1b-docs]: Phase 1B documentation and requirements.
"#;

    let test_path = Path::new("test_mixed.md");
    let document = parser.parse_content(test_content, test_path)
        .await.expect("Should parse mixed Phase 1B content");

    // Should extract all feature types
    assert!(!document.latex_expressions.is_empty(), "Should have LaTeX");
    assert!(!document.callouts.is_empty(), "Should have callouts");
    assert!(!document.tags.is_empty(), "Should have tags");
    assert!(!document.footnotes.references.is_empty(), "Should have footnotes");

    // Should have task lists with mixed content
    let task_lists: Vec<_> = document.content.lists.iter()
        .filter(|list| list.items.iter().any(|item| item.task_status.is_some()))
        .collect();
    assert!(!task_lists.is_empty(), "Should have task lists");

    // Verify specific mixed content scenarios
    let math_hashtag = document.tags.iter()
        .find(|t| t.name == "latex");
    assert!(math_hashtag.is_some(), "Should find #math hashtag");

    let math_hashtag_2 = document.tags.iter()
        .find(|t| t.name == "math");
    assert!(math_hashtag_2.is_some(), "Should find #latex hashtag");

    let phase1b_tag = document.tags.iter()
        .find(|t| t.name == "Phase1B");
    assert!(phase1b_tag.is_some(), "Should find #Phase1B hashtag");

    let integration_tag = document.tags.iter()
        .find(|t| t.name == "integration-testing");
    assert!(integration_tag.is_some(), "Should find #integration-testing hashtag");

    // Verify callout with tasks
    let callout_with_tasks = document.callouts.iter()
        .find(|c| c.content.contains("Tasks for implementation"));
    assert!(callout_with_tasks.is_some(), "Should find callout containing tasks");

    println!("✅ Mixed Phase 1B features test passed!");
    println!("   LaTeX: {}, Callouts: {}, Tags: {}, Footnotes: {}",
             document.latex_expressions.len(),
             document.callouts.len(),
             document.tags.len(),
             document.footnotes.references.len());
}