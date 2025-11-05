//! Obsidian Callouts Extension Tests
//!
//! Test Obsidian-style callout parsing for Phase 1B features:
//! - Standard callout types (note, warning, danger, etc.)
//! - Custom callout types
//! - Multi-line callout content
//! - Nested content in callouts

use crucible_parser::{CrucibleParser, MarkdownParserImplementation, callouts::create_callout_extension};
use std::path::Path;

/// Test standard Obsidian callout types
#[tokio::test]
async fn test_standard_callout_types() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Standard Callouts

> [!NOTE] This is a note callout
> It can have multiple paragraphs.

> [!TIP] This is a tip callout
> **Pro tip**: Use formatting in callouts.

> [!WARNING] This is a warning callout
> Be careful with this information.

> [!DANGER] This is a danger callout
> ⚠️ Critical information here.

> [!INFO] This is an info callout
> Additional context and details.

> [!SUCCESS] This is a success callout
> ✅ Operation completed successfully.

> [!QUESTION] This is a question callout
> How does this work?

> [!FAILURE] This is a failure callout
> ❌ Something went wrong.
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse callout content");

    assert!(!document.callouts.is_empty(), "Should find callouts");

    // Should find all standard callout types
    let callout_types: std::collections::HashMap<_, _> = document.callouts.iter()
        .map(|c| (c.callout_type.as_str(), c))
        .collect();

    assert!(callout_types.contains_key("note"), "Should have NOTE callout");
    assert!(callout_types.contains_key("tip"), "Should have TIP callout");
    assert!(callout_types.contains_key("warning"), "Should have WARNING callout");
    assert!(callout_types.contains_key("danger"), "Should have DANGER callout");
    assert!(callout_types.contains_key("info"), "Should have INFO callout");
    assert!(callout_types.contains_key("success"), "Should have SUCCESS callout");
    assert!(callout_types.contains_key("question"), "Should have QUESTION callout");
    assert!(callout_types.contains_key("failure"), "Should have FAILURE callout");

    // Verify callout content
    let note_callout = callout_types["note"];
    assert!(note_callout.content.contains("This is a note callout"),
           "Note should contain expected content");
    assert!(note_callout.content.contains("multiple paragraphs"),
           "Note should preserve multiple paragraphs");

    let tip_callout = callout_types["tip"];
    assert!(tip_callout.content.contains("**Pro tip**"),
           "Tip should preserve markdown formatting");
}

/// Test custom callout types
#[tokio::test]
async fn test_custom_callout_types() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Custom Callouts

> [!CUSTOM] This is a custom callout
> Custom callouts should work too.

> [!IMPORTANT] Important information here
> This is a commonly used custom type.

> [!CAUTION] Caution advised
> Be careful with this approach.

> [!BUG] Bug report information
> Related to issue #123.

> [!EXAMPLE] Example usage
> Here's how to use this feature.

> [!QUOTE] Inspirational quote
> "The only way to do great work is to love what you do." - Steve Jobs

> [!CITE] Academic citation
> Author (Year, p.123) made this claim.
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse custom callout content");

    assert!(!document.callouts.is_empty(), "Should find callouts");

    let callout_types: std::collections::HashMap<_, _> = document.callouts.iter()
        .map(|c| (c.callout_type.as_str(), c))
        .collect();

    // Should handle custom types gracefully
    assert!(callout_types.contains_key("custom"), "Should have CUSTOM callout");
    assert!(callout_types.contains_key("important"), "Should have IMPORTANT callout");
    assert!(callout_types.contains_key("caution"), "Should have CAUTION callout");
    assert!(callout_types.contains_key("bug"), "Should have BUG callout");
    assert!(callout_types.contains_key("example"), "Should have EXAMPLE callout");
    assert!(callout_types.contains_key("quote"), "Should have QUOTE callout");
    assert!(callout_types.contains_key("cite"), "Should have CITE callout");

    // Verify custom callout content is preserved
    let quote_callout = callout_types["quote"];
    assert!(quote_callout.content.contains("Steve Jobs"),
           "Quote callout should preserve full content");
}

/// Test multi-line callout content
#[tokio::test]
async fn test_multiline_callout_content() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Multi-line Callouts

> [!NOTE] This is a multi-line note
> First paragraph of the note with **bold** text and *italic* text.
>
> Second paragraph with a list:
> - Item 1 with `inline code`
> - Item 2 with #hashtags
> - Item 3 with [links](https://example.com)
>
> Third paragraph with mathematical expression: $E = mc^2$.
>
> Final paragraph concluding the note.

> [!WARNING] Complex warning content
> This warning has multiple sections:
>
> ## Subheading in Warning
>
> Some warning text here.
>
> ### Another Subheading
>
> More detailed warning information.
>
> - [ ] Task item in warning
> - [x] Completed task in warning
>
> End of warning content.
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse multi-line callout content");

    assert!(!document.callouts.is_empty(), "Should find callouts");

    // Find the note callout
    let note_callout = document.callouts.iter()
        .find(|c| c.callout_type == "note")
        .expect("Should find note callout");

    assert!(note_callout.content.contains("First paragraph"), "Should preserve first paragraph");
    assert!(note_callout.content.contains("Second paragraph"), "Should preserve second paragraph");
    assert!(note_callout.content.contains("Third paragraph"), "Should preserve third paragraph");
    assert!(note_callout.content.contains("Final paragraph"), "Should preserve final paragraph");
    assert!(note_callout.content.contains("**bold**"), "Should preserve bold formatting");
    assert!(note_callout.content.contains("*italic*"), "Should preserve italic formatting");
    assert!(note_callout.content.contains("`inline code`"), "Should preserve inline code");
    assert!(note_callout.content.contains("#hashtags"), "Should preserve hashtags");
    assert!(note_callout.content.contains("[links]"), "Should preserve links");
    assert!(note_callout.content.contains("$E = mc^2$"), "Should preserve LaTeX");

    // Find the warning callout
    let warning_callout = document.callouts.iter()
        .find(|c| c.callout_type == "warning")
        .expect("Should find warning callout");

    assert!(warning_callout.content.contains("## Subheading"), "Should preserve subheadings");
    assert!(warning_callout.content.contains("### Another Subheading"), "Should preserve subheadings");
    assert!(warning_callout.content.contains("- [ ] Task item"), "Should preserve task lists");
}

/// Test callouts with nested content
#[tokio::test]
async fn test_callouts_with_nested_content() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Callouts with Nested Content

> [!INFO] Callout with nested quote
> This is the main content.
>
> > This is a nested quote inside the callout.
> > It should be preserved correctly.
>
> Back to main callout content.

> [!TIP] Callout with code block
> Here's some code:
>
> ```rust
> fn main() {
>     println!("Hello, world!");
> }
> ```
>
> And more text after the code block.

> [!WARNING] Callout with list
> Important information:
>
> 1. First important point
> 2. Second important point
>    - Nested subpoint
>    - Another subpoint
> 3. Third important point
>
> Concluding text.

> [!DANGER] Callout with math block
> Critical formula:
>
> $$
> \sum_{i=1}^{n} i = \frac{n(n+1)}{2}
> $$
>
> This is the sum formula.
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse nested callout content");

    assert!(!document.callouts.is_empty(), "Should find callouts");

    // Check each callout type and content
    let callout_types: std::collections::HashMap<_, _> = document.callouts.iter()
        .map(|c| (c.callout_type.as_str(), c))
        .collect();

    // Check nested quote
    if let Some(info_callout) = callout_types.get("info") {
        assert!(info_callout.content.contains("nested quote"), "Should preserve nested quotes");
        assert!(info_callout.content.contains("> This is a nested quote"),
               "Should preserve quote formatting");
    }

    // Check code block
    if let Some(tip_callout) = callout_types.get("tip") {
        assert!(tip_callout.content.contains("```rust"), "Should preserve code blocks");
        assert!(tip_callout.content.contains("println!"), "Should preserve code content");
    }

    // Check ordered list
    if let Some(warning_callout) = callout_types.get("warning") {
        assert!(warning_callout.content.contains("1. First important point"),
               "Should preserve ordered lists");
        assert!(warning_callout.content.contains("- Nested subpoint"),
               "Should preserve nested list items");
    }

    // Check math block
    if let Some(danger_callout) = callout_types.get("danger") {
        assert!(danger_callout.content.contains("$$"), "Should preserve math blocks");
        assert!(danger_callout.content.contains("\\sum_{i=1}^{n}"),
               "Should preserve LaTeX in callouts");
    }
}

/// Test callout edge cases
#[tokio::test]
async fn test_callout_edge_cases() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Callout Edge Cases

> [!NOTE] Callout with empty lines
>
> Line after empty line.
>
>
> Another line after multiple empty lines.

> [!TIP] Callout with special characters: <, >, &, ", ', and emojis: 🚀⭐🔥

> [!WARNING] Callout with markdown
> **Bold text**, *italic text*, `code`, [links](url), and #hashtags.

> [!DANGER] Callout with inline math $x^2 + y^2 = z^2$ and footnotes[^footnote].

> [!INFO] Callout ending without proper paragraph break
> No blank line after this.

> [!CUSTOM] Callout with mixed case and numbers: Test123
> Mixed content should work.

Regular quote not a callout:
> This is just a regular quote.

Invalid callout syntax:
> [INVALID] Missing exclamation mark
> [! Not a callout either

> [!EMPTY]

> [!ONLY-HEADER]
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse callout edge cases");

    // Should find valid callouts
    let valid_callouts: Vec<_> = document.callouts.iter()
        .filter(|c| !c.callout_type.is_empty())
        .collect();

    assert!(!valid_callouts.is_empty(), "Should find valid callouts");

    // Check that special characters are preserved
    let callout_contents: Vec<_> = valid_callouts.iter()
        .map(|c| c.content.as_str())
        .collect();

    let special_chars_found = callout_contents.iter()
        .any(|content| content.contains("🚀") || content.contains("<, >"));
    assert!(special_chars_found, "Should preserve special characters");

    // Check markdown formatting in callouts
    let markdown_found = callout_contents.iter()
        .any(|content| content.contains("**Bold text**") || content.contains("*italic text*"));
    assert!(markdown_found, "Should preserve markdown formatting");

    // Check LaTeX in callouts
    let latex_found = callout_contents.iter()
        .any(|content| content.contains("$x^2 + y^2 = z^2$"));
    assert!(latex_found, "Should preserve LaTeX in callouts");
}

/// Test callout positioning
#[tokio::test]
async fn test_callout_positioning() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Callout Position Test

First content here.

> [!NOTE] First callout
> Content of first callout.

Middle content here.

> [!TIP] Second callout
> Content of second callout.

Final content here.
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse callout positioning");

    assert_eq!(document.callouts.len(), 2, "Should find exactly 2 callouts");

    // Verify positions are recorded and in order
    let mut positions: Vec<_> = document.callouts.iter()
        .map(|callout| callout.start_offset)
        .collect();
    positions.sort_unstable();

    let original_positions: Vec<_> = document.callouts.iter()
        .map(|callout| callout.start_offset)
        .collect();

    assert_eq!(positions, original_positions, "Positions should be in parsing order");

    // Verify positions are within content bounds
    for callout in &document.callouts {
        assert!(callout.start_offset > 0, "Should have valid start offset");
        assert!(callout.length > 0, "Should have valid length");
        assert!(callout.start_offset < content.len() as u64, "Should be within content bounds");
    }
}

/// Test callout extension directly
#[test]
fn test_callout_extension_creation() {
    let extension = create_callout_extension();

    assert_eq!(extension.name(), "callouts", "Extension should be named 'callouts'");
    assert!(extension.supports_callouts(), "Should support callouts");

    let capabilities = extension.capabilities();
    assert!(capabilities.supports_callouts, "Capabilities should indicate callout support");

    // Test extension processes content correctly
    let test_content = "> [!NOTE] This is a note callout\n> Multi-line content.";

    let mut document_content = crucible_parser::DocumentContent::default();
    let result = extension.process_content(test_content, Path::new("test.md"), &mut document_content);

    assert!(result.is_ok(), "Should process content without errors");
}

/// Test callout with different list markers
#[tokio::test]
async fn test_callouts_with_different_list_markers() {
    let parser = CrucibleParser::with_default_extensions();

    let content = r#"
# Callouts with Lists

> [!INFO] Callout with dash lists
> Information here:
> - First item
> - Second item
>   - Nested item
> - Third item

> [!TIP] Callout with asterisk lists
> Tips here:
> * First tip
> * Second tip
> * Third tip

> [!WARNING] Callout with plus lists
> Warnings:
> + Warning 1
> + Warning 2
> + Warning 3

> [!DANGER] Callout with numbered lists
> Steps:
> 1. First step
> 2. Second step
> 3. Third step
>    - Nested in numbered
>    - Another nested

> [!SUCCESS] Callout with mixed lists
> Mixed content:
> - Dash item
> * Asterisk item
> + Plus item
> 1. Numbered item
"#;

    let document = parser.parse_content(content, Path::new("test.md")).await
        .expect("Should parse callouts with lists");

    assert!(!document.callouts.is_empty(), "Should find callouts");

    // Should have callouts with different list types
    let callout_types: std::collections::HashMap<_, _> = document.callouts.iter()
        .map(|c| (c.callout_type.as_str(), c))
        .collect();

    assert!(callout_types.contains_key("info"), "Should have INFO callout");
    assert!(callout_types.contains_key("tip"), "Should have TIP callout");
    assert!(callout_types.contains_key("warning"), "Should have WARNING callout");
    assert!(callout_types.contains_key("danger"), "Should have DANGER callout");
    assert!(callout_types.contains_key("success"), "Should have SUCCESS callout");

    // Check that list content is preserved
    for callout in &document.callouts {
        // Should preserve various list markers
        let has_list_markers = callout.content.contains("- ") ||
                              callout.content.contains("* ") ||
                              callout.content.contains("+ ") ||
                              callout.content.contains("1. ");
        assert!(has_list_markers, "Callout should preserve list markers: {}", callout.callout_type);
    }

    println!("✅ Callout list test passed! Found {} callouts with various list types", document.callouts.len());
}