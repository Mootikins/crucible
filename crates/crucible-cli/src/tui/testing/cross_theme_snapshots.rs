//! Cross-theme snapshot tests

#[cfg(test)]
mod cross_theme_snapshots {
    use crate::tui::ratatui_markdown::RatatuiMarkdown;
    use crate::tui::theme::MarkdownTheme;
    use insta::assert_debug_snapshot;

    fn render_markdown_theme(markdown: &str, theme: MarkdownTheme) -> String {
        let renderer = RatatuiMarkdown::new(theme);
        let lines = renderer.render(markdown);
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn code_block_renders_both_themes() {
        let dark = render_markdown_theme("```rust\nfn main() {}\n```", MarkdownTheme::dark());
        let light = render_markdown_theme("```rust\nfn main() {}\n```", MarkdownTheme::light());
        assert!(dark.contains("fn main"));
        assert!(light.contains("fn main"));
        assert_debug_snapshot!("code_block_rust_dark", dark);
        assert_debug_snapshot!("code_block_rust_light", light);
    }

    #[test]
    fn table_renders_both_themes() {
        let markdown = "| Col1 | Col2 |\n|------|------|\n| A    | B    |";
        let dark = render_markdown_theme(markdown, MarkdownTheme::dark());
        let light = render_markdown_theme(markdown, MarkdownTheme::light());
        assert_debug_snapshot!("table_simple_dark", dark);
        assert_debug_snapshot!("table_simple_light", light);
    }

    #[test]
    fn heading_renders_both_themes() {
        let markdown = "# Heading 1\n\n## Heading 2\n\n### Heading 3";
        let dark = render_markdown_theme(markdown, MarkdownTheme::dark());
        let light = render_markdown_theme(markdown, MarkdownTheme::light());
        assert_debug_snapshot!("headings_dark", dark);
        assert_debug_snapshot!("headings_light", light);
    }

    #[test]
    fn list_with_code_renders_both_themes() {
        let markdown = "- Item one\n- Item two with code:\n  ```python\n  print('hello')\n  ```";
        let dark = render_markdown_theme(markdown, MarkdownTheme::dark());
        let light = render_markdown_theme(markdown, MarkdownTheme::light());
        assert_debug_snapshot!("list_with_code_dark", dark);
        assert_debug_snapshot!("list_with_code_light", light);
    }

    #[test]
    fn blockquote_renders_both_themes() {
        let markdown = "> This is a quote\n> with multiple lines";
        let dark = render_markdown_theme(markdown, MarkdownTheme::dark());
        let light = render_markdown_theme(markdown, MarkdownTheme::light());
        assert_debug_snapshot!("blockquote_dark", dark);
        assert_debug_snapshot!("blockquote_light", light);
    }

    #[test]
    fn link_renders_both_themes() {
        let markdown = "[Click here](https://example.com) for more info";
        let dark = render_markdown_theme(markdown, MarkdownTheme::dark());
        let light = render_markdown_theme(markdown, MarkdownTheme::light());
        assert_debug_snapshot!("link_dark", dark);
        assert_debug_snapshot!("link_light", light);
    }

    #[test]
    fn mixed_content_renders_both_themes() {
        let markdown = "# Main Title\n\nSome **bold** and *italic* text.\n\n- List item 1\n- List item 2\n\n```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```";
        let dark = render_markdown_theme(markdown, MarkdownTheme::dark());
        let light = render_markdown_theme(markdown, MarkdownTheme::light());
        assert_debug_snapshot!("mixed_content_dark", dark);
        assert_debug_snapshot!("mixed_content_light", light);
    }

    #[test]
    fn code_block_with_long_lines_both_themes() {
        let markdown =
            "```\nThis is a very long line that should wrap\nAnother long line here\n```";
        let dark = render_markdown_theme(markdown, MarkdownTheme::dark());
        let light = render_markdown_theme(markdown, MarkdownTheme::light());
        assert_debug_snapshot!("code_block_long_dark", dark);
        assert_debug_snapshot!("code_block_long_light", light);
    }
}
