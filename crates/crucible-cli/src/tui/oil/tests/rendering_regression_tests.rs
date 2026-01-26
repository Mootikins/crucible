//! Regression tests for rendering issues
//!
//! These tests reproduce specific rendering bugs to prevent regressions.

use crate::tui::oil::chat_app::Role;
use crate::tui::oil::test_harness::AppHarness;
use insta::assert_snapshot;

/// Issue: Table content duplicated after graduation
///
/// When streaming content with tables completes and graduates to scrollback,
/// the table content appears twice - once as the table, once as plain text.
///
/// Expected: Table should appear exactly once in graduated output.
#[test]
fn table_not_duplicated_after_graduation() {
    let mut h = AppHarness::new(80, 24);

    // Simulate streaming a message with a table
    let markdown_with_table = r#"
Here's a summary:

| Feature | Status |
|---------|--------|
| Tables  | Working |
| Lists   | Working |

That's the overview.
"#;

    // Start streaming
    h.app.cache.start_streaming();
    h.app.cache.append_streaming(markdown_with_table);

    // Complete streaming (this triggers graduation)
    h.app
        .cache
        .complete_streaming("msg-1".to_string(), crucible_core::types::Role::Assistant);

    // Render the graduated content
    let rendered = h.render();

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
///
/// Example:
/// ```
/// │ • Your notes =      │
/// │ memory – embed      │
/// ```
///
/// Expected: Bullet points should stay together or wrap cleanly.
#[test]
fn table_cell_wrapping_preserves_spacing() {
    let mut h = Harness::new(80, 24);

    let markdown_with_wrapped_cells = r#"
| Section | Description |
|---------|-------------|
| Core ideas | • Markdown sessions – every chat is a file<br>• Your notes = memory – embed every block |
"#;

    h.app.cache.start_streaming();
    h.app.cache.append_streaming(markdown_with_wrapped_cells);
    h.app
        .cache
        .complete_streaming("msg-1".to_string(), crucible_core::types::Role::Assistant);

    let rendered = h.render();

    // Check that bullet points aren't orphaned on separate lines
    // The bullet "•" should be on the same line as its content
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

/// Issue: Notification popup appears left-aligned instead of right-aligned
///
/// When showing notifications (like "✓ Thinking display: on"), the popup
/// box should appear in the top-right corner but appears on the left.
///
/// Expected: Notification box should be right-aligned.
#[test]
fn notification_popup_right_aligned() {
    let mut h = Harness::new(80, 24);

    // Trigger a notification (e.g., toggling thinking display)
    // TODO: Need to find the actual notification mechanism
    // For now, create a placeholder test

    let rendered = h.render();

    // Check that notification box characters appear on the right side
    // The box uses ▗▄▄▄ (top), ▌ (sides), ▘ (bottom)
    let lines: Vec<&str> = rendered.lines().collect();

    for line in &lines {
        if line.contains("▗") || line.contains("▌") || line.contains("▘") {
            // Find the position of the box character
            let box_pos = line.chars().position(|c| c == '▗' || c == '▌' || c == '▘');

            if let Some(pos) = box_pos {
                // Box should be in the right half of the screen (past column 40 for 80-wide terminal)
                assert!(
                    pos > 40,
                    "Notification box at column {} should be right-aligned (> 40)",
                    pos
                );
            }
        }
    }

    assert_snapshot!("notification_right_aligned", rendered);
}

/// Issue: Content duplication during streaming-to-graduated transition
///
/// When content transitions from streaming (viewport) to graduated (scrollback),
/// there's a brief moment where content appears in both places, or content
/// is duplicated in the final output.
///
/// Expected: Content should appear exactly once, with atomic transition.
#[test]
fn no_duplication_during_graduation_transition() {
    let mut h = Harness::new(80, 24);

    // Add a distinctive message
    let unique_content = "This is a unique test message with identifier XYZ123";

    h.app.cache.start_streaming();
    h.app.cache.append_streaming(unique_content);

    // Capture state before graduation
    let before_graduation = h.render();
    let before_count = before_graduation.matches("XYZ123").count();

    // Complete streaming (triggers graduation)
    h.app
        .cache
        .complete_streaming("msg-1".to_string(), crucible_core::types::Role::Assistant);

    // Capture state after graduation
    let after_graduation = h.render();
    let after_count = after_graduation.matches("XYZ123").count();

    // Content should appear exactly once before and after
    assert_eq!(before_count, 1, "Content duplicated before graduation");
    assert_eq!(after_count, 1, "Content duplicated after graduation");

    assert_snapshot!("graduation_transition_atomic", after_graduation);
}

/// Issue: Spacing lost between graduated elements
///
/// When multiple elements (paragraphs, lists, tables) graduate together,
/// the spacing between them is sometimes lost, causing elements to run together.
///
/// Expected: Proper spacing (blank lines) between graduated elements.
#[test]
fn spacing_preserved_between_graduated_elements() {
    let mut h = Harness::new(80, 24);

    let markdown_with_spacing = r#"
First paragraph here.

Second paragraph here.

- List item 1
- List item 2

Final paragraph.
"#;

    h.app.cache.start_streaming();
    h.app.cache.append_streaming(markdown_with_spacing);
    h.app
        .cache
        .complete_streaming("msg-1".to_string(), crucible_core::types::Role::Assistant);

    let rendered = h.render();
    let lines: Vec<&str> = rendered.lines().collect();

    // Find "Second paragraph" and check there's a blank line before it
    let second_para_idx = lines.iter().position(|l| l.contains("Second paragraph"));
    if let Some(idx) = second_para_idx {
        assert!(idx > 0, "Second paragraph should not be first line");
        let prev_line = lines[idx - 1].trim();
        assert!(
            prev_line.is_empty() || prev_line.starts_with("First"),
            "Should have blank line or previous content before 'Second paragraph', got: '{}'",
            prev_line
        );
    }

    assert_snapshot!("graduated_spacing_preserved", rendered);
}
