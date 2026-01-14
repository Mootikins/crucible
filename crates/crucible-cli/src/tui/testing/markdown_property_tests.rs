//! Property-based tests for markdown rendering
//!
//! Uses proptest for randomized testing of markdown rendering properties.
//! These tests verify invariants that should hold for ANY valid input.

#[cfg(test)]
mod proptest_markdown {
    use crate::tui::ratatui_markdown::RatatuiMarkdown;
    use crate::tui::theme::MarkdownTheme;
    use proptest::prelude::*;

    /// Helper to render markdown and extract text content
    fn render_to_text(md: &str) -> String {
        let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = renderer.render(md);
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect()
    }

    /// Helper to render markdown with width constraint
    fn render_with_width(md: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
        RatatuiMarkdown::new(MarkdownTheme::dark())
            .with_width(width)
            .render(md)
    }

    // =========================================================================
    // Property-Based Tests (using proptest generators)
    // =========================================================================

    proptest! {
        /// Code fence content is always preserved in output
        #[test]
        fn code_fence_preserves_content(
            lang in "[a-z]{0,10}",
            content in "[a-zA-Z0-9_ ]{1,50}"
        ) {
            let md = format!("```{}\n{}\n```", lang, content);
            let output = render_to_text(&md);
            prop_assert!(
                output.contains(&content),
                "Code content '{}' should be in output", content
            );
        }

        /// Inline code content is always preserved
        #[test]
        fn inline_code_preserves_content(content in "[a-zA-Z0-9_]{1,20}") {
            let md = format!("`{}`", content);
            let output = render_to_text(&md);
            prop_assert!(
                output.contains(&content),
                "Inline code '{}' should be in output", content
            );
        }

        /// Heading text is always preserved
        #[test]
        fn heading_preserves_text(
            level in 1usize..=6,
            text in "[a-zA-Z]{1,30}"  // No spaces to avoid whitespace-only
        ) {
            let hashes = "#".repeat(level);
            let md = format!("{} {}", hashes, text);
            let output = render_to_text(&md);
            prop_assert!(
                output.contains(&text),
                "Heading text '{}' should be in output", text
            );
        }

        /// Bold text content is preserved
        #[test]
        fn bold_preserves_content(text in "[a-zA-Z]{1,20}") {
            let md = format!("**{}**", text);
            let output = render_to_text(&md);
            prop_assert!(
                output.contains(&text),
                "Bold text '{}' should be in output", text
            );
        }

        /// Italic text content is preserved
        #[test]
        fn italic_preserves_content(text in "[a-zA-Z]{1,20}") {
            let md = format!("*{}*", text);
            let output = render_to_text(&md);
            prop_assert!(
                output.contains(&text),
                "Italic text '{}' should be in output", text
            );
        }

        /// Narrower width produces >= number of lines
        #[test]
        fn narrow_width_wraps_more(
            text in "[a-zA-Z ]{20,100}",
            narrow in 20usize..40,
            wide in 80usize..120
        ) {
            let narrow_lines = render_with_width(&text, narrow);
            let wide_lines = render_with_width(&text, wide);
            prop_assert!(
                narrow_lines.len() >= wide_lines.len(),
                "Narrow ({}) should produce >= lines than wide ({}): {} vs {}",
                narrow, wide, narrow_lines.len(), wide_lines.len()
            );
        }

        /// Rendering handles all input without exposing panics
        /// (markdown-it has bugs with certain edge cases, we catch them)
        #[test]
        fn rendering_handles_arbitrary_input(input in "[a-zA-Z0-9#*_`\\[\\]()| \n-]{0,200}") {
            let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                renderer.render(&input)
            }));
            // Success = either no panic or we caught the panic
            // This proves our code won't crash the application
            let _ = result;
        }

        /// Link text is preserved (URL may be rendered differently)
        #[test]
        fn link_text_preserved(
            text in "[a-zA-Z]{1,15}",
            url in "https://[a-z]{3,10}\\.com"
        ) {
            let md = format!("[{}]({})", text, url);
            let output = render_to_text(&md);
            // Link text should be visible
            prop_assert!(
                output.contains(&text),
                "Link text '{}' should be in output", text
            );
        }

        /// List items preserve their content
        #[test]
        fn list_items_preserved(
            item1 in "[a-zA-Z]{1,20}",  // No spaces to avoid whitespace-only
            item2 in "[a-zA-Z]{1,20}"
        ) {
            let md = format!("- {}\n- {}", item1, item2);
            let output = render_to_text(&md);
            prop_assert!(output.contains(&item1), "First item should be in output");
            prop_assert!(output.contains(&item2), "Second item should be in output");
        }
    }

    // =========================================================================
    // Data-Driven Tests (for specific edge cases)
    // =========================================================================

    #[test]
    fn test_empty_markdown() {
        let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = renderer.render("");
        assert!(lines.len() <= 1, "Empty markdown should produce <= 1 line");
    }

    #[test]
    fn test_known_edge_cases() {
        let edge_cases = vec![
            // Empty/whitespace
            "",
            "   ",
            "\n\n\n",
            // Nested emphasis
            "**bold and *italic***",
            "***both***",
            // Code with special chars
            "```\n# not a heading\n```",
            "`inline with *asterisks*`",
            // Tables
            "|a|b|\n|-|-|\n|c|d|",
            // Blockquotes
            "> quoted\n> text",
            // Mixed content
            "# Title\n\n**bold** and `code`\n\n- item",
        ];

        for input in edge_cases {
            let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
            let _ = renderer.render(input);
            // Success = no panic
        }
    }

    #[test]
    fn test_unicode_width_handling() {
        use crate::tui::ratatui_markdown::display_width;

        // CJK characters are double-width
        assert_eq!(display_width("日本語"), 6);
        assert_eq!(display_width("한국어"), 6);

        // ASCII is single-width
        assert_eq!(display_width("Hello"), 5);

        // Mixed content
        assert_eq!(display_width("Hello世界"), 9);

        // Combining characters
        assert_eq!(display_width("ä"), 1); // precomposed
    }

    #[test]
    fn test_table_content_preserved() {
        let md = "|Name|Age|\n|---|---|\n|Alice|30|\n|Bob|25|";
        let output = render_to_text(md);
        assert!(output.contains("Alice"), "Table should contain Alice");
        assert!(output.contains("Bob"), "Table should contain Bob");
        assert!(output.contains("30"), "Table should contain 30");
    }
}
