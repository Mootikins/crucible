//! Tests for table rendering in conversation history.
//!
//! These tests capture and verify how markdown tables are displayed in the TUI,
//! including simple tables, wide tables, and tables mixed with other content.

use super::fixtures::sessions;
use super::{Harness, TEST_HEIGHT, TEST_WIDTH};
use insta::assert_snapshot;

// =============================================================================
// Snapshot Tests - Table Rendering via Harness
// =============================================================================

mod snapshots {
    use super::*;

    #[test]
    fn simple_table_renders() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_table());
        assert_snapshot!("table_simple", h.render());
    }

    #[test]
    fn wide_table_at_standard_width() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_wide_table());
        assert_snapshot!("table_wide_standard", h.render());
    }

    #[test]
    fn wide_table_in_narrow_viewport() {
        let h = Harness::new(50, TEST_HEIGHT).with_session(sessions::with_wide_table());
        assert_snapshot!("table_wide_narrow", h.render());
    }

    #[test]
    fn multiple_tables_in_response() {
        let h =
            Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_multiple_tables());
        assert_snapshot!("table_multiple", h.render());
    }

    #[test]
    fn table_with_code_blocks() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_table_and_code());
        assert_snapshot!("table_with_code", h.render());
    }
}

// =============================================================================
// Unit Tests - Table Content Verification
// =============================================================================

mod content_tests {
    use super::*;

    #[test]
    fn simple_table_contains_expected_content() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_table());
        let output = h.render();

        // Should contain table headers
        assert!(
            output.contains("Feature") || output.contains("feature"),
            "Should show Feature header"
        );
        assert!(
            output.contains("Rust") || output.contains("rust"),
            "Should show Rust"
        );
        assert!(
            output.contains("Go") || output.contains("go"),
            "Should show Go"
        );

        // Should contain table data
        assert!(
            output.contains("Memory") || output.contains("memory"),
            "Should show Memory row"
        );
        assert!(
            output.contains("Safe") || output.contains("safe"),
            "Should show Safe value"
        );
    }

    #[test]
    fn wide_table_contains_all_columns() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_wide_table());
        let output = h.render();

        // Should contain all column headers
        assert!(output.contains("Package"), "Should show Package column");
        assert!(
            output.contains("Description"),
            "Should show Description column"
        );
        assert!(output.contains("Version"), "Should show Version column");
        assert!(output.contains("License"), "Should show License column");

        // Should contain data
        assert!(output.contains("serde"), "Should show serde package");
        assert!(output.contains("tokio"), "Should show tokio package");
    }

    #[test]
    fn multiple_tables_both_present() {
        let h =
            Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_multiple_tables());
        let output = h.render();

        // First table content
        assert!(
            output.contains("Type System") || output.contains("Language"),
            "Should show first table header"
        );
        assert!(
            output.contains("Static") || output.contains("Dynamic"),
            "Should show first table data"
        );

        // Second table content
        assert!(
            output.contains("Model") || output.contains("Concurrency"),
            "Should show second table header"
        );
        assert!(
            output.contains("Ownership") || output.contains("CSP"),
            "Should show second table data"
        );
    }

    #[test]
    fn table_and_code_both_render() {
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(sessions::with_table_and_code());
        let output = h.render();

        // Table content
        assert!(
            output.contains("Field") || output.contains("Type"),
            "Should show table header"
        );
        assert!(output.contains("name"), "Should show table row");

        // Code content
        assert!(output.contains("Item"), "Should show code struct name");
    }
}

// =============================================================================
// Edge Case Tests
// =============================================================================

mod edge_cases {
    use super::*;
    use crate::tui::testing::fixtures::sessions::{assistant, user};

    #[test]
    fn very_narrow_viewport_still_renders() {
        // Minimum reasonable width
        let h = Harness::new(30, TEST_HEIGHT).with_session(sessions::with_table());
        let output = h.render();

        // Should not panic and should have some content
        assert!(!output.is_empty(), "Should render something");
    }

    #[test]
    fn empty_table_like_content() {
        // Table with just pipes, edge case
        let items = vec![user("Show empty"), assistant("| | |\n|-|-|\n| | |")];
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(items);
        let output = h.render();

        // Should not panic
        assert!(!output.is_empty(), "Should render something");
    }

    #[test]
    fn table_with_long_cell_content() {
        let items = vec![
            user("Show long content"),
            assistant(
                "| Name | Description |\n\
                 |------|-------------|\n\
                 | test | This is a very long description that might wrap or be truncated depending on viewport width |",
            ),
        ];
        let h = Harness::new(TEST_WIDTH, TEST_HEIGHT).with_session(items);
        let output = h.render();

        // Should contain the content or truncation
        assert!(output.contains("test"), "Should show name");
        assert!(
            output.contains("long") || output.contains("..."),
            "Should show description or truncation"
        );
    }

    /// Test that proportional shrinking works when minimums fit but natural widths don't
    #[test]
    fn proportional_shrinking_preserves_all_columns() {
        // Table with short words (small minimums) but long total content
        // Minimums: Name=4, Info=4, Type=4, Note=4 = 16 content
        // Full widths: 4, 30, 4, 20 = 58 content
        // Overhead: 5 borders + 8 padding = 13
        // Full table: 71 chars, Minimum table: 29 chars
        // At 50 chars viewport: should shrink proportionally without clipping
        let items = vec![
            user("Show shrinkable table"),
            assistant(
                "| Name | Info                          | Type | Note                |\n\
                 |------|-------------------------------|------|---------------------|\n\
                 | foo  | some long info text here      | bar  | extra note text     |\n\
                 | baz  | more info with many words     | qux  | another note here   |",
            ),
        ];
        // Use 50 char viewport - enough for minimums (29) but not full widths (71)
        let h = Harness::new(50, TEST_HEIGHT).with_session(items);
        let output = h.render();

        // All columns should be present
        assert!(output.contains("Name"), "Should show Name column");
        assert!(output.contains("Info"), "Should show Info column");
        assert!(output.contains("Type"), "Should show Type column");
        assert!(output.contains("Note"), "Should show Note column");

        // Table should have proper closing borders (not clipped)
        // Check that at least one table line has a closing border
        let has_closing_border = output.lines().any(|line| {
            let trimmed = line.trim();
            (trimmed.starts_with('│')
                || trimmed.starts_with('┌')
                || trimmed.starts_with('├')
                || trimmed.starts_with('└'))
                && (trimmed.ends_with('│')
                    || trimmed.ends_with('┐')
                    || trimmed.ends_with('┤')
                    || trimmed.ends_with('┘'))
        });
        assert!(
            has_closing_border,
            "Table should have proper closing borders (proportional shrinking should avoid clipping)"
        );
    }
}
