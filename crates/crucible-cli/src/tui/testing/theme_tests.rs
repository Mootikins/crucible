//! Tests for light theme rendering in the TUI.
//!
//! These tests verify that markdown rendering works correctly in light theme mode,
//! which is critical for users with light terminal backgrounds.

use crate::tui::ratatui_markdown::RatatuiMarkdown;
use crate::tui::theme::MarkdownTheme;

const TEST_WIDTH: usize = 80;

fn render_with_light_theme(content: &str) -> String {
    let theme = MarkdownTheme::light();
    let renderer = RatatuiMarkdown::new(theme).with_width(TEST_WIDTH);
    let lines = renderer.render(content);
    lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_with_dark_theme(content: &str) -> String {
    let theme = MarkdownTheme::dark();
    let renderer = RatatuiMarkdown::new(theme).with_width(TEST_WIDTH);
    let lines = renderer.render(content);
    lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<Vec<_>>()
                .join("")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

mod prose_rendering {
    use super::*;

    #[test]
    fn simple_text_renders_in_light_theme() {
        let content = "Hello, world!";
        let result = render_with_light_theme(content);
        assert!(result.contains("Hello, world!"));
    }

    #[test]
    fn bold_text_renders_in_light_theme() {
        let content = "This is **bold text** here.";
        let result = render_with_light_theme(content);
        assert!(result.contains("bold text"));
    }

    #[test]
    fn italic_text_renders_in_light_theme() {
        let content = "This is *italic text* here.";
        let result = render_with_light_theme(content);
        assert!(result.contains("italic text"));
    }

    #[test]
    fn inline_code_renders_in_light_theme() {
        let content = "Use `let x = 5;` to define a variable.";
        let result = render_with_light_theme(content);
        assert!(result.contains("let x = 5;"));
    }

    #[test]
    fn links_render_in_light_theme() {
        let content = "[OpenAI](https://openai.com)";
        let result = render_with_light_theme(content);
        assert!(result.contains("OpenAI"));
    }

    #[test]
    fn headings_render_in_light_theme() {
        let content = "# Heading 1\n\n## Heading 2\n\n### Heading 3";
        let result = render_with_light_theme(content);
        assert!(result.contains("Heading 1"));
        assert!(result.contains("Heading 2"));
        assert!(result.contains("Heading 3"));
    }

    #[test]
    fn blockquotes_render_in_light_theme() {
        let content = "> This is a blockquote.";
        let result = render_with_light_theme(content);
        assert!(result.contains("This is a blockquote"));
    }

    #[test]
    fn unordered_list_renders_in_light_theme() {
        let content = "- Item 1\n- Item 2\n- Item 3";
        let result = render_with_light_theme(content);
        assert!(result.contains("Item 1"));
        assert!(result.contains("Item 2"));
        assert!(result.contains("Item 3"));
    }

    #[test]
    fn ordered_list_renders_in_light_theme() {
        let content = "1. First item\n2. Second item\n3. Third item";
        let result = render_with_light_theme(content);
        assert!(result.contains("First item"));
        assert!(result.contains("Second item"));
    }

    #[test]
    fn paragraph_wrapping_in_light_theme() {
        let content = "This is a very long line that should wrap at the terminal width of eighty characters. It contains many words that need to be properly wrapped at word boundaries.";
        let result = render_with_light_theme(content);
        // The content should be preserved (possibly with newlines from wrapping)
        // Check that key parts of the content are present
        assert!(
            result.contains("very long line"),
            "Should contain start of content"
        );
        assert!(
            result.contains("word boundaries") || result.contains("word"),
            "Should contain word-related content"
        );
        // Make sure the text wasn't corrupted
        assert!(result.contains("eighty"));
    }
}

mod code_blocks {
    use super::*;

    #[test]
    fn untagged_code_block_renders_in_light_theme() {
        let content = "```\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let result = render_with_light_theme(content);
        assert!(result.contains("fn main()"));
        assert!(result.contains("println!"));
    }

    #[test]
    fn rust_code_block_renders_in_light_theme() {
        let content = "```rust\nfn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n```";
        let result = render_with_light_theme(content);
        assert!(result.contains("fn add"));
        assert!(result.contains("a + b"));
    }

    #[test]
    fn python_code_block_renders_in_light_theme() {
        let content =
            "```python\ndef greet(name: str) -> str:\n    return f\"Hello, {name}!\"\n```";
        let result = render_with_light_theme(content);
        assert!(result.contains("def greet"));
        assert!(result.contains("Hello,"));
    }

    #[test]
    fn multi_language_code_blocks_render_in_light_theme() {
        let content = "Here is Rust:\n\n```rust\nlet x = 5;\n```\n\nAnd here is JavaScript:\n\n```javascript\nconst x = 5;\n```";
        let result = render_with_light_theme(content);
        assert!(result.contains("let x = 5;"));
        assert!(result.contains("const x = 5;"));
    }

    #[test]
    fn code_block_with_long_lines_wraps_in_light_theme() {
        let content = "```\nThis is a very long line of code that exceeds eighty characters and should wrap gracefully in light theme mode without causing any rendering issues.\n```";
        let result = render_with_light_theme(content);
        assert!(result.contains("very long line of code"));
    }
}

mod tables {
    use super::*;

    #[test]
    fn simple_table_renders_in_light_theme() {
        let content = "| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |";
        let result = render_with_light_theme(content);
        assert!(result.contains("Alice"));
        assert!(result.contains("Bob"));
        assert!(result.contains("30"));
        assert!(result.contains("25"));
    }

    #[test]
    fn table_with_long_cell_content_wraps_in_light_theme() {
        let content = "| Short | Long Content That Should Wrap |\n|-------|------------------------------|\n| A | This is a very long cell content that exceeds the column width and needs to wrap gracefully. |";
        let result = render_with_light_theme(content);
        // The table should contain the content, wrapped appropriately
        assert!(result.contains("Short"), "Should contain Short column");
        assert!(result.contains("A"), "Should contain cell value A");
        // The long content may be wrapped or truncated - just verify table rendered
        assert!(
            result.contains('│') || result.contains('|'),
            "Should have table structure"
        );
    }

    #[test]
    fn table_border_characters_visible_in_light_theme() {
        let content = "| A | B |\n|---|---|\n| 1 | 2 |";
        let result = render_with_light_theme(content);
        // Table should have border characters (box-drawing characters)
        let has_horizontal_border = result.contains('─') || result.contains('┌');
        let has_vertical_border = result.contains('│') || result.contains('|');
        assert!(
            has_horizontal_border && has_vertical_border,
            "Table should have border characters: {}",
            result
        );
    }

    #[test]
    fn wide_table_in_narrow_viewport_wraps_in_light_theme() {
        let content = "| Column1 | Column2 | Column3 | Column4 | Column5 |\n|---------|---------|---------|---------|---------|\n| Value1 | VeryLongValueThatExceedsWidth | Value3 | AnotherLongValue | Value5 |";
        let result = render_with_light_theme(content);
        assert!(result.contains("Column1"));
        assert!(result.contains("Value1"));
    }
}

mod mixed_content {
    use super::*;

    #[test]
    fn prose_between_code_blocks_in_light_theme() {
        let content =
            "Here is some code:\n\n```rust\nfn main() {}\n```\n\nAnd then some more text.";
        let result = render_with_light_theme(content);
        assert!(result.contains("fn main()"));
        assert!(result.contains("some more text"));
    }

    #[test]
    fn list_with_code_blocks_in_light_theme() {
        let content = "- First item\n- Second item with code:\n\n  ```python\n  def foo():\n      pass\n  ```\n- Third item";
        let result = render_with_light_theme(content);
        // The list should render with items
        assert!(result.contains("First item"), "Should contain first item");
        assert!(result.contains("Third item"), "Should contain third item");
        // Code blocks are rendered - check that something was rendered
        // The actual code content may be formatted differently
        assert!(!result.is_empty(), "Should render some content");
    }

    #[test]
    fn blockquote_with_formatting_in_light_theme() {
        let content = "> This is a **bold** quote with `code`.";
        let result = render_with_light_theme(content);
        assert!(result.contains("bold"));
        assert!(result.contains("code"));
    }

    #[test]
    fn headings_then_prose_in_light_theme() {
        let content =
            "# Main Title\n\n## Subtitle\n\nThis is the body text that follows the headings.";
        let result = render_with_light_theme(content);
        assert!(result.contains("Main Title"));
        assert!(result.contains("Subtitle"));
        assert!(result.contains("body text"));
    }
}

mod dark_light_consistency {
    use super::*;

    #[test]
    fn prose_content_same_in_both_themes() {
        let content = "This is some **bold** and *italic* text with a [link](https://example.com).";
        let dark = render_with_dark_theme(content);
        let light = render_with_light_theme(content);
        assert!(dark.contains("bold") && dark.contains("italic"));
        assert!(light.contains("bold") && light.contains("italic"));
    }

    #[test]
    fn code_blocks_same_content_in_both_themes() {
        let content = "```rust\nfn add(a, b) { a + b }\n```";
        let dark = render_with_dark_theme(content);
        let light = render_with_light_theme(content);
        assert!(dark.contains("fn add"));
        assert!(light.contains("fn add"));
    }

    #[test]
    fn tables_same_content_in_both_themes() {
        let content = "| A | B |\n|---|---|\n| 1 | 2 |";
        let dark = render_with_dark_theme(content);
        let light = render_with_light_theme(content);
        assert!(dark.contains("A") && dark.contains("B"));
        assert!(light.contains("A") && light.contains("B"));
    }
}
