//! Regression tests for rendering issues
//!
//! These tests reproduce specific rendering bugs to prevent regressions.

use crate::tui::oil::ansi::strip_ansi;
use crate::tui::oil::app::{App, ViewContext};
use crate::tui::oil::chat_app::{ChatAppMsg, InkChatApp};
use crate::tui::oil::focus::FocusContext;
use crate::tui::oil::render::render_to_string;
use insta::assert_snapshot;

fn render_app(app: &InkChatApp) -> String {
    let focus = FocusContext::new();
    let ctx = ViewContext::new(&focus);
    let tree = app.view(&ctx);
    let rendered = render_to_string(&tree, 80);
    strip_ansi(&rendered)
}

/// Issue: Table content duplicated after graduation
///
/// When streaming content with tables completes and graduates to scrollback,
/// the table content appears twice - once as the table, once as plain text.
///
/// Expected: Table should appear exactly once in graduated output.
#[test]
fn table_not_duplicated_after_graduation() {
    let mut app = InkChatApp::default();

    // Simulate streaming a message with a table
    let markdown_with_table = r#"Here's a summary:

| Feature | Status |
|---------|--------|
| Tables  | Working |
| Lists   | Working |

That's the overview."#;

    // Start conversation with user message
    app.on_message(ChatAppMsg::UserMessage("Show me a summary".to_string()));

    // Send as streaming deltas
    app.on_message(ChatAppMsg::TextDelta(markdown_with_table.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);

    // Count occurrences of "Tables" - should appear exactly once in the table
    let tables_count = rendered.matches("Tables").count();
    assert_eq!(
        tables_count, 1,
        "Table content should not be duplicated. Found {} occurrences of 'Tables'",
        tables_count
    );

    // Snapshot the full output
    assert_snapshot!("table_graduation_no_duplication", rendered);
}

/// Issue: Table cells lose spacing when wrapped
///
/// Multi-line content in table cells (like bullet points) gets split
/// incorrectly, with spacing lost between wrapped lines.
#[test]
fn table_cell_wrapping_preserves_spacing() {
    let mut app = InkChatApp::default();

    let markdown_with_wrapped_cells = r#"
| Section | Description |
|---------|-------------|
| Core ideas | • Markdown sessions – every chat is a file • Your notes = memory – embed every block |
"#;

    app.on_message(ChatAppMsg::UserMessage(
        "Explain the core ideas".to_string(),
    ));
    app.on_message(ChatAppMsg::TextDelta(
        markdown_with_wrapped_cells.to_string(),
    ));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);

    // Check that bullet points aren't orphaned on separate lines
    let lines: Vec<&str> = rendered.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains("•") {
            // If a line has a bullet, it should have more than just the bullet
            let bullet_line = line.trim();
            assert!(
                bullet_line.len() > 3,
                "Line {} has orphaned bullet: '{}'",
                i,
                line
            );
        }
    }

    assert_snapshot!("table_cell_wrapping", rendered);
}

/// Issue: Content duplication during streaming-to-graduated transition
///
/// When content transitions from streaming (viewport) to graduated (scrollback),
/// content may appear in both places or be duplicated.
#[test]
fn no_duplication_during_graduation_transition() {
    let mut app = InkChatApp::default();

    // Add a distinctive message
    let unique_content = "This is a unique test message with identifier XYZ123";

    app.on_message(ChatAppMsg::UserMessage("Test question".to_string()));
    app.on_message(ChatAppMsg::TextDelta(unique_content.to_string()));

    // Capture state before completion
    let before_graduation = render_app(&app);
    let before_count = before_graduation.matches("XYZ123").count();

    // Complete streaming (triggers graduation)
    app.on_message(ChatAppMsg::StreamComplete);

    // Capture state after graduation
    let after_graduation = render_app(&app);
    let after_count = after_graduation.matches("XYZ123").count();

    // Content should appear exactly once before and after
    assert_eq!(before_count, 1, "Content duplicated before graduation");
    assert_eq!(after_count, 1, "Content duplicated after graduation");

    assert_snapshot!("graduation_transition_atomic", after_graduation);
}

/// Issue: Spacing lost between graduated elements
///
/// When multiple elements (paragraphs, lists, tables) graduate together,
/// the spacing between them is sometimes lost.
#[test]
fn spacing_preserved_between_graduated_elements() {
    let mut app = InkChatApp::default();

    let markdown_with_spacing = r#"First paragraph here.

Second paragraph here.

- List item 1
- List item 2

Final paragraph."#;

    app.on_message(ChatAppMsg::UserMessage("Explain with examples".to_string()));
    app.on_message(ChatAppMsg::TextDelta(markdown_with_spacing.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);
    let lines: Vec<&str> = rendered.lines().collect();

    // Find "Second paragraph" and check there's spacing before it
    let second_para_idx = lines.iter().position(|l| l.contains("Second paragraph"));
    if let Some(idx) = second_para_idx {
        assert!(idx > 0, "Second paragraph should not be first line");
        // There should be a blank line or the first paragraph before it
        let has_spacing = idx > 1 && lines[idx - 1].trim().is_empty();
        assert!(
            has_spacing || lines.iter().take(idx).any(|l| l.contains("First")),
            "Should have proper spacing before 'Second paragraph'"
        );
    }

    assert_snapshot!("graduated_spacing_preserved", rendered);
}

/// Issue: Complex markdown with tables renders correctly
///
/// Test the actual output from the user's example to see what's happening.
#[test]
fn complex_markdown_with_table() {
    let mut app = InkChatApp::default();

    // Simplified version of the user's actual output
    let complex_markdown = r#"Crucible – a local-first AI assistant

| Section | What it covers |
|---------|----------------|
| What it is | A Rust-powered AI agent that lives on your machine. All conversations are stored as plain-text Markdown files. |
| Core ideas | • Markdown sessions – every chat is a file<br>• Your notes = memory – embed every block |

Why it matters

1. Control – All data lives on your machine
2. Flexibility – Plug in any LLM"#;

    app.on_message(ChatAppMsg::UserMessage("What is Crucible?".to_string()));
    app.on_message(ChatAppMsg::TextDelta(complex_markdown.to_string()));
    app.on_message(ChatAppMsg::StreamComplete);

    let rendered = render_app(&app);

    // Check for duplication - "Crucible" should appear a reasonable number of times
    let crucible_count = rendered.matches("Crucible").count();
    assert!(
        crucible_count <= 2,
        "Content appears to be duplicated. 'Crucible' appears {} times",
        crucible_count
    );

    assert_snapshot!("complex_markdown_table", rendered);
}
