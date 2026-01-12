//! Property-based tests for markdown rendering
//!
//! Uses standard proptest with explicit test cases.

#[cfg(test)]
mod proptest_markdown {
    use crate::tui::ratatui_markdown::RatatuiMarkdown;
    use crate::tui::theme::MarkdownTheme;
    use proptest::prelude::*;

    #[test]
    fn test_code_blocks() {
        let test_cases = vec![
            ("rust", "fn main() {}"),
            ("python", "print('hello')"),
            ("", "plain code"),
            ("js", "console.log(x)"),
        ];
        for (lang, content) in test_cases {
            let md = format!("```{}\n{}\n```", lang, content);
            let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
            let lines = renderer.render(&md);
            let all_text: String = lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|s| s.content.as_ref())
                .collect();
            assert!(
                all_text.contains(content),
                "Code content '{}' should be in output for lang '{}'",
                content,
                lang
            );
        }
    }

    #[test]
    fn test_links() {
        let urls = vec![
            "https://example.com",
            "http://test.org/path",
            "https://api.github.com/users",
        ];
        for url in urls {
            let md = format!("[link]({})", url);
            let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
            let lines = renderer.render(&md);
            let all_text: String = lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|s| s.content.as_ref())
                .collect();
            assert!(all_text.contains(url), "URL should be in output");
        }
    }

    #[test]
    fn test_inline_code() {
        let codes = vec!["foo", "bar-baz", "test_123", "hello world"];
        for code in codes {
            let md = format!("`{}`", code);
            let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
            let lines = renderer.render(&md);
            let all_text: String = lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|s| s.content.as_ref())
                .collect();
            assert!(
                all_text.contains(code),
                "Inline code '{}' should be in output",
                code
            );
        }
    }

    #[test]
    fn test_headings() {
        let test_cases = vec![(1, "Title"), (3, "Subtitle"), (6, "Small")];
        for (level, text) in test_cases {
            let hashes = "#".repeat(level);
            let md = format!("{} {}", hashes, text);
            let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
            let lines = renderer.render(&md);
            let all_text: String = lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|s| s.content.as_ref())
                .collect();
            assert!(all_text.contains(&text), "Heading text should be in output");
        }
    }

    #[test]
    fn test_emphasis() {
        let texts = vec!["bold", "italic", "mixed-text"];
        for text in texts {
            let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
            let bold_lines = renderer.render(&format!("**{}**", text));
            let bold_text: String = bold_lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|s| s.content.as_ref())
                .collect();
            assert!(bold_text.contains(text), "Bold should contain text");

            let italic_lines = renderer.render(&format!("*{}*", text));
            let italic_text: String = italic_lines
                .iter()
                .flat_map(|l| l.spans.iter())
                .map(|s| s.content.as_ref())
                .collect();
            assert!(italic_text.contains(text), "Italic should contain text");
        }
    }

    #[test]
    fn test_width_affects_wrapping() {
        let text = "this is a long sentence that should wrap at narrow widths but fit on fewer lines at wide widths";

        let narrow = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(20);
        let wide = RatatuiMarkdown::new(MarkdownTheme::dark()).with_width(100);

        let narrow_lines = narrow.render(&text);
        let wide_lines = wide.render(&text);

        assert!(
            narrow_lines.len() >= wide_lines.len(),
            "Narrow width ({}) should produce >= lines than wide ({})",
            narrow_lines.len(),
            wide_lines.len()
        );
    }

    #[test]
    fn test_empty_markdown() {
        let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = renderer.render("");
        assert!(lines.len() <= 1, "Empty markdown should produce <= 1 line");
    }

    #[test]
    fn test_tables() {
        let cell = "data";
        let md = "|col1|col2|\n|----|----|\n|data|data|\n";
        let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
        let lines = renderer.render(&md);
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains(cell), "Table cell should be in output");
    }

    #[test]
    fn test_parsing_various_inputs() {
        let inputs = vec![
            "",
            "# heading",
            "**bold**",
            "*italic*",
            "`code`",
            "[link](url)",
            "- item",
            "1. item",
            "```code```",
            "> quote",
            "|a|b|\n|-|-|\n|c|d|",
            "text\n\nparagraph",
            "**bold and *italic***",
        ];
        for input in inputs {
            let renderer = RatatuiMarkdown::new(MarkdownTheme::dark());
            let _ = renderer.render(input);
            // If we get here, parsing didn't panic
        }
    }
}
