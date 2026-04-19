use super::*;
use crucible_oil::ansi::visible_width;
use crucible_oil::render::render_to_string;

#[test]
fn test_plain_text() {
    let node = markdown_to_node("Hello world");
    let output = render_to_string(&node, 80);
    assert!(output.contains("Hello world"));
}

#[test]
fn test_bold_text() {
    let node = markdown_to_node("This is **bold** text");
    let output = render_to_string(&node, 80);
    assert!(output.contains("bold"));
}

#[test]
fn test_heading() {
    let node = markdown_to_node("# Heading 1\n\nSome text");
    let output = render_to_string(&node, 80);
    assert!(output.contains("Heading 1"));
}

#[test]
fn test_code_block() {
    let node = markdown_to_node("```rust\nfn main() {}\n```");
    let output = render_to_string(&node, 80);
    assert!(output.contains("fn") && output.contains("main"));
}

#[test]
fn test_bullet_list() {
    let node = markdown_to_node("- Item 1\n- Item 2\n- Item 3");
    let output = render_to_string(&node, 80);
    assert!(output.contains("Item 1"));
    assert!(output.contains("•"));
}

#[test]
fn test_blockquote() {
    let node = markdown_to_node("> This is a quote");
    let output = render_to_string(&node, 80);
    assert!(output.contains("quote"));
    assert!(output.contains("│"));
}

#[test]
fn test_table() {
    let node = markdown_to_node("| A | B |\n|---|---|\n| 1 | 2 |");
    let output = render_to_string(&node, 80);
    assert!(output.contains("A"));
    assert!(output.contains("B"));
    assert!(output.contains("1"));
    assert!(output.contains("2"));
    assert!(output.contains("┌"), "Should have top-left corner");
    assert!(output.contains("┐"), "Should have top-right corner");
    assert!(output.contains("└"), "Should have bottom-left corner");
    assert!(output.contains("┘"), "Should have bottom-right corner");
}

#[test]
fn test_blank_lines_between_blocks() {
    let md = "# Heading\n\nParagraph one.\n\nParagraph two.";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();
    assert!(
        lines.len() >= 4,
        "Should have blank lines between blocks: {:?}",
        lines
    );
    assert!(
        lines.iter().any(|l| l.is_empty()),
        "Should have empty lines: {:?}",
        lines
    );
}

#[test]
fn test_table_followed_by_paragraph() {
    let md = "| A | B |\n|---|---|\n| 1 | 2 |\n\nSome text after table.";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    assert!(output.contains("Some text after table"));
    let lines: Vec<&str> = output.split("\r\n").collect();
    let table_end = lines.iter().position(|l| l.contains("└")).unwrap();
    let text_pos = lines.iter().position(|l| l.contains("Some text")).unwrap();
    assert!(
        text_pos > table_end + 1,
        "Should have blank line between table and paragraph"
    );
}

#[test]
fn test_link() {
    let node = markdown_to_node("[click here](https://example.com)");
    let output = render_to_string(&node, 80);
    assert!(output.contains("click here"));
}

#[test]
fn test_nested_list_not_duplicated() {
    let md = "- Parent item\n  - Nested item one\n  - Nested item two";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);

    let count = output.matches("Nested item one").count();
    assert_eq!(
        count, 1,
        "Nested item should appear exactly once, but appeared {} times.\nOutput:\n{}",
        count, output
    );
}

#[test]
fn test_nested_list_no_blank_line_after_parent() {
    let md = "- Parent item\n  - Nested item";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();

    let parent_idx = lines.iter().position(|l| l.contains("Parent")).unwrap();
    let nested_idx = lines.iter().position(|l| l.contains("Nested")).unwrap();

    assert_eq!(
        nested_idx,
        parent_idx + 1,
        "Nested list should immediately follow parent item (no blank line).\nLines: {:?}",
        lines
    );
}

#[test]
fn test_paragraph_then_heading_has_blank_line() {
    let md = "Some paragraph.\n\n## A Heading";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();

    let para_idx = lines.iter().position(|l| l.contains("paragraph")).unwrap();
    let heading_idx = lines.iter().position(|l| l.contains("Heading")).unwrap();

    assert!(
        heading_idx > para_idx + 1,
        "Should have blank line between paragraph and heading.\nLines: {:?}",
        lines
    );
}

#[test]
fn test_paragraph_then_heading_with_margins() {
    let md = "Some paragraph.\n\n## A Heading";
    let style = RenderStyle::natural_with_margins(80, Margins::assistant());
    let node = markdown_to_node_styled(md, style);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();

    let para_idx = lines.iter().position(|l| l.contains("paragraph")).unwrap();
    let heading_idx = lines.iter().position(|l| l.contains("Heading")).unwrap();

    assert!(
        heading_idx > para_idx + 1,
        "With margins: should have blank line between paragraph and heading.\nLines: {:?}",
        lines
    );
}

#[test]
fn ordered_list_then_paragraph_has_blank_line() {
    let md = "1. Item one\n2. Item two\n\nFinal paragraph.";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();

    let last_item = lines.iter().rposition(|l| l.contains("Item two")).unwrap();
    let para = lines
        .iter()
        .position(|l| l.contains("Final paragraph"))
        .unwrap();

    assert!(
        para > last_item + 1,
        "Should have blank line between ordered list and paragraph.\nLines: {:?}",
        lines
    );
}

#[test]
fn ordered_list_then_paragraph_has_blank_line_with_margins() {
    let md = "1. Item one\n2. Item two\n\nFinal paragraph.";
    let style = RenderStyle::natural_with_margins(80, Margins::assistant());
    let node = markdown_to_node_styled(md, style);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();

    let last_item = lines.iter().rposition(|l| l.contains("Item two")).unwrap();
    let para = lines
        .iter()
        .position(|l| l.contains("Final paragraph"))
        .unwrap();

    assert!(
        para > last_item + 1,
        "With margins: should have blank line between ordered list and paragraph.\nLines: {:?}",
        lines
    );
}

#[test]
fn ordered_list_renders_incrementing_numbers() {
    let md = "1. Alpha\n2. Beta\n3. Gamma";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    assert!(output.contains("1. "), "Should contain '1. '");
    assert!(output.contains("2. "), "Should contain '2. '");
    assert!(output.contains("3. "), "Should contain '3. '");
}

#[test]
fn ordered_list_lazy_numbering_renders_incrementing() {
    let md = "1. Alpha\n1. Beta\n1. Gamma";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    assert!(output.contains("1. "), "Should contain '1. '");
    assert!(output.contains("2. "), "Should contain '2. '");
    assert!(output.contains("3. "), "Should contain '3. '");
}

#[test]
fn ordered_list_no_blank_lines_between_items() {
    let md = "1. First\n2. Second\n3. Third";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();

    let first_idx = lines.iter().position(|l| l.contains("First")).unwrap();
    let second_idx = lines.iter().position(|l| l.contains("Second")).unwrap();
    let third_idx = lines.iter().position(|l| l.contains("Third")).unwrap();

    assert_eq!(
        second_idx,
        first_idx + 1,
        "No blank line between First and Second.\nLines: {:?}",
        lines
    );
    assert_eq!(
        third_idx,
        second_idx + 1,
        "No blank line between Second and Third.\nLines: {:?}",
        lines
    );
}

#[test]
fn two_ordered_lists_separated_by_paragraph() {
    let md = "1. A\n2. B\n\nSome paragraph\n\n1. X\n2. Y";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();

    // First list: 1. A, 2. B
    assert!(
        lines.iter().any(|l| l.contains("1.") && l.contains("A")),
        "First list item A"
    );
    assert!(
        lines.iter().any(|l| l.contains("2.") && l.contains("B")),
        "First list item B"
    );

    // Paragraph between lists
    assert!(
        lines.iter().any(|l| l.contains("Some paragraph")),
        "Paragraph between lists"
    );

    // Second list: 1. X, 2. Y (renumbered from 1)
    assert!(
        lines.iter().any(|l| l.contains("1.") && l.contains("X")),
        "Second list item X"
    );
    assert!(
        lines.iter().any(|l| l.contains("2.") && l.contains("Y")),
        "Second list item Y"
    );
}

#[test]
fn loose_ordered_list_no_extra_spacing() {
    let md = "1. A\n\n2. B\n\n3. C";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();

    // Should render as 1, 2, 3 with no blank lines between
    assert!(
        lines.iter().any(|l| l.contains("1.") && l.contains("A")),
        "Item 1"
    );
    assert!(
        lines.iter().any(|l| l.contains("2.") && l.contains("B")),
        "Item 2"
    );
    assert!(
        lines.iter().any(|l| l.contains("3.") && l.contains("C")),
        "Item 3"
    );

    let a_idx = lines.iter().position(|l| l.contains("A")).unwrap();
    let b_idx = lines.iter().position(|l| l.contains("B")).unwrap();
    let c_idx = lines.iter().position(|l| l.contains("C")).unwrap();

    assert_eq!(
        b_idx,
        a_idx + 1,
        "No blank line between A and B.\nLines: {:?}",
        lines
    );
    assert_eq!(
        c_idx,
        b_idx + 1,
        "No blank line between B and C.\nLines: {:?}",
        lines
    );
}

fn assert_lines_fit_width(output: &str, max_width: usize) {
    for (i, line) in output.split("\r\n").enumerate() {
        let width = visible_width(line);
        assert!(
            width <= max_width,
            "Line {} exceeds width {}: {} chars\n{:?}",
            i + 1,
            max_width,
            width,
            line
        );
    }
}

#[test]
fn table_respects_width_constraint() {
    let wide_table = r#"| Very Long Header Column | Another Long Header Column |
|---|---|
| This cell has lots of content | More content in this cell too |"#;

    let node = markdown_to_node_with_width(wide_table, 50);
    let output = render_to_string(&node, 50);

    assert_lines_fit_width(&output, 50);
    assert!(output.contains("┌"), "Should have table border");
    assert!(output.contains("┘"), "Should have table border");
}

#[test]
fn table_with_very_narrow_width() {
    let table = "| Header | Data |\n|---|---|\n| Content | Value |";
    let node = markdown_to_node_with_width(table, 25);
    let output = render_to_string(&node, 25);

    assert_lines_fit_width(&output, 25);
}

#[test]
fn table_multicolumn_fits_width() {
    let table = r#"| Col1 | Col2 | Col3 | Col4 |
|---|---|---|---|
| A | B | C | D |
| Long content here | More long content | Even more | Final |"#;

    let node = markdown_to_node_with_width(table, 60);
    let output = render_to_string(&node, 60);

    assert_lines_fit_width(&output, 60);
}

#[test]
fn text_wraps_at_width_boundary() {
    let long_text = "This is a very long sentence that should wrap at the specified width boundary without breaking words unnecessarily.";
    let node = markdown_to_node_with_width(long_text, 40);
    let output = render_to_string(&node, 40);

    assert_lines_fit_width(&output, 40);

    let lines: Vec<&str> = output.split("\r\n").collect();
    assert!(lines.len() > 1, "Long text should wrap to multiple lines");
}

#[test]
fn text_does_not_wrap_when_fits() {
    let short_text = "Short text that fits.";
    let node = markdown_to_node_with_width(short_text, 80);
    let output = render_to_string(&node, 80);

    let lines: Vec<&str> = output.split("\r\n").collect();
    assert_eq!(lines.len(), 1, "Short text should not wrap");
    assert!(output.contains("Short text that fits."));
}

#[test]
fn styled_text_wraps_correctly() {
    let md = "This has **bold text** and *italic text* mixed together in a long sentence that needs to wrap.";
    let node = markdown_to_node_with_width(md, 40);
    let output = render_to_string(&node, 40);

    assert_lines_fit_width(&output, 40);
    assert!(output.contains("bold text"));
    assert!(output.contains("italic text"));
    assert!(output.contains("\x1b[1m"), "Should have bold ANSI code");
    assert!(output.contains("\x1b[3m"), "Should have italic ANSI code");
}

#[test]
fn list_items_wrap_within_width() {
    let md = r#"- This is a very long list item that should wrap properly within the specified width constraint
- Another long item with lots of text that also needs to wrap correctly"#;

    let node = markdown_to_node_with_width(md, 50);
    let output = render_to_string(&node, 50);

    assert_lines_fit_width(&output, 50);
    assert!(output.contains("•"));
}

#[test]
fn blockquote_wraps_within_width() {
    let md = "> This is a long blockquote that should wrap properly within the width constraint while maintaining the quote prefix.";
    let node = markdown_to_node_with_width(md, 40);
    let output = render_to_string(&node, 40);

    assert_lines_fit_width(&output, 40);
    assert!(output.contains("│"));
}

#[test]
fn words_not_broken_unnecessarily() {
    let md = "Pneumonoultramicroscopicsilicovolcanoconiosis is a long word.";
    let node = markdown_to_node_with_width(md, 60);
    let output = render_to_string(&node, 60);

    assert!(
        output.contains("Pneumonoultramicroscopicsilicovolcanoconiosis"),
        "Long word should not be broken when it fits on a line"
    );
}

#[test]
fn table_at_67_columns() {
    let table = r#"| Feature | Rust | Go |
|---------|------|-----|
| Memory | Safe | GC |
| Speed | Fast | Fast |"#;

    let node = markdown_to_node_with_width(table, 67);
    let output = render_to_string(&node, 67);
    assert_lines_fit_width(&output, 67);
}

#[test]
fn table_forces_cell_wrap_at_67_columns() {
    let table = r#"| Command | Description | Example Usage |
|---------|-------------|---------------|
| search | Search through notes semantically | crucible search "query" |
| semantic | Semantic search with embeddings enabled | crucible semantic "concept" |
| note create | Create a new note in kiln | crucible note create path.md |"#;

    let node = markdown_to_node_with_width(table, 67);
    let output = render_to_string(&node, 67);
    assert_lines_fit_width(&output, 67);
}

#[test]
fn table_with_bullet_prefix_at_67_columns() {
    use crucible_oil::node::{row, styled};
    use crucible_oil::style::{Color, Style};

    let table = r#"| Command | Description | Example Usage |
|---------|-------------|---------------|
| search | Search through notes semantically | crucible search "query" |
| semantic | Semantic search with embeddings enabled | crucible semantic "concept" |"#;

    let prefix_width = 2;
    let content_width = 67 - prefix_width;
    let md_node = markdown_to_node_with_width(table, content_width);
    let with_prefix = row([
        styled("● ".to_string(), Style::new().fg(Color::DarkGray)),
        md_node,
    ]);
    let output = render_to_string(&with_prefix, 67);
    assert_lines_fit_width(&output, 67);
}

#[test]
fn br_tag_converts_to_newline() {
    let md = "Line one<br>Line two";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let lines: Vec<&str> = output.split("\r\n").collect();
    assert!(lines.len() >= 2, "Should have multiple lines: {:?}", lines);
    assert!(output.contains("Line one"));
    assert!(output.contains("Line two"));
}

#[test]
fn br_tag_variants_all_work() {
    for variant in ["<br>", "<br/>", "<br />", "<BR>", "<BR/>", "<Br />"] {
        let md = format!("A{}B", variant);
        let node = markdown_to_node(&md);
        let output = render_to_string(&node, 80);
        let lines: Vec<&str> = output.split("\r\n").collect();
        assert!(
            lines.len() >= 2,
            "Variant {} should create newline: {:?}",
            variant,
            lines
        );
    }
}

#[test]
fn br_tag_in_table_cell() {
    let table = "| Header |\n|---|\n| First<br>Second |";
    let node = markdown_to_node(table);
    let output = render_to_string(&node, 80);
    assert!(output.contains("First"));
    assert!(output.contains("Second"));
}

#[test]
fn normalize_br_tags_function() {
    use super::render::normalize_br_tags;
    assert_eq!(normalize_br_tags("a<br>b"), "a  \nb");
    assert_eq!(normalize_br_tags("a<br/>b"), "a  \nb");
    assert_eq!(normalize_br_tags("a<br />b"), "a  \nb");
    assert_eq!(normalize_br_tags("a<BR>b"), "a  \nb");
    assert_eq!(normalize_br_tags("no tags here"), "no tags here");
    assert_eq!(
        normalize_br_tags("multi<br>line<br>text"),
        "multi  \nline  \ntext"
    );
}

#[test]
fn table_uses_table_width_not_text_width() {
    let table = "| A | B | C |\n|---|---|---|\n| 1 | 2 | 3 |";

    let node = markdown_to_node_with_widths(table, 10000, 50);
    let output = render_to_string(&node, 50);

    assert_lines_fit_width(&output, 50);
    assert!(output.contains("┌"), "Should have table border");
}

#[test]
fn text_ignores_table_width() {
    let long_text = "This is text that should use the large text_width and not wrap early.";

    let node = markdown_to_node_with_widths(long_text, 10000, 20);
    let output = render_to_string(&node, 200);

    let lines: Vec<&str> = output.split("\r\n").collect();
    assert_eq!(lines.len(), 1, "Text should not wrap (text_width=10000)");
}

mod render_style_tests {
    use super::*;
    use crate::tui::oil::markdown::{markdown_to_node_styled, RenderStyle};

    #[test]
    fn render_style_viewport_widths() {
        let style = RenderStyle::viewport(80);
        assert_eq!(style.text_width(), 80);
        assert_eq!(style.table_width(), 80);
    }

    #[test]
    fn render_style_natural_widths() {
        let style = RenderStyle::natural(80);
        assert_eq!(style.text_width(), 80);
        assert_eq!(style.table_width(), 80);
    }

    #[test]
    fn viewport_style_wraps_text_at_width() {
        let long_text = "This is a long paragraph that should wrap at the viewport width boundary when using viewport style rendering.";
        let style = RenderStyle::viewport(40);
        let node = markdown_to_node_styled(long_text, style);
        let output = render_to_string(&node, 40);

        assert_lines_fit_width(&output, 40);
        let lines: Vec<&str> = output.split("\r\n").collect();
        assert!(lines.len() > 1, "Viewport style should wrap text");
    }

    #[test]
    fn natural_style_wraps_text_at_terminal_width() {
        let long_text = "This is a long paragraph that should wrap when using natural style rendering for consistent left edge alignment.";
        let style = RenderStyle::natural(40);
        let node = markdown_to_node_styled(long_text, style);
        let output = render_to_string(&node, 40);

        let lines: Vec<&str> = output.split("\r\n").collect();
        assert!(lines.len() > 1, "Natural style should now wrap text");
    }

    #[test]
    fn viewport_style_table_fits_width() {
        let table = "| Header A | Header B | Header C |\n|----------|----------|----------|\n| Cell 1   | Cell 2   | Cell 3   |";
        let style = RenderStyle::viewport(50);
        let node = markdown_to_node_styled(table, style);
        let output = render_to_string(&node, 50);

        assert_lines_fit_width(&output, 50);
    }

    #[test]
    fn natural_style_table_fits_terminal_width() {
        let table = "| Header A | Header B | Header C |\n|----------|----------|----------|\n| Cell 1   | Cell 2   | Cell 3   |";
        let style = RenderStyle::natural(50);
        let node = markdown_to_node_styled(table, style);
        let output = render_to_string(&node, 50);

        assert_lines_fit_width(&output, 50);
    }

    #[test]
    fn natural_style_mixed_content_table_fits() {
        let md = "This paragraph uses natural text width.\n\n| A | B |\n|---|---|\n| 1 | 2 |";
        let style = RenderStyle::natural(40);
        let node = markdown_to_node_styled(md, style);
        let output = render_to_string(&node, 200);

        for line in output.lines() {
            if line.contains('┌') || line.contains('│') || line.contains('└') {
                let width = visible_width(line);
                assert!(
                    width <= 40,
                    "Table line should fit terminal width: {} > 40",
                    width
                );
            }
        }

        assert!(
            output.contains("natural text width"),
            "Paragraph content should be present"
        );
    }

    #[test]
    fn constructor_helpers_work() {
        let v = RenderStyle::viewport(100);
        let n = RenderStyle::natural(100);

        assert!(matches!(v, RenderStyle::Viewport { width: 100, .. }));
        assert!(matches!(
            n,
            RenderStyle::Natural {
                terminal_width: 100,
                ..
            }
        ));
    }

    #[test]
    fn render_style_equality() {
        assert_eq!(RenderStyle::viewport(80), RenderStyle::viewport(80));
        assert_ne!(RenderStyle::viewport(80), RenderStyle::viewport(100));
        assert_ne!(RenderStyle::viewport(80), RenderStyle::natural(80));
        assert_eq!(RenderStyle::natural(80), RenderStyle::natural(80));
    }
}

mod syntax_highlighting {
    use super::*;

    #[test]
    fn rust_code_block_has_multiple_colors() {
        let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);

        // Syntax highlighting should produce multiple distinct RGB color codes
        // RGB escape sequence pattern: \x1b[38;2;R;G;B;m
        let rgb_pattern = regex::Regex::new(r"\x1b\[38;2;\d+;\d+;\d+m").unwrap();
        let matches: Vec<_> = rgb_pattern.find_iter(&output).collect();

        assert!(
            matches.len() >= 3,
            "Rust code should have at least 3 different color codes for keywords, \
             strings, and identifiers. Got {} matches in output:\n{}",
            matches.len(),
            output.escape_debug()
        );
    }

    #[test]
    fn python_code_block_has_highlighting() {
        let md = "```python\ndef hello():\n    print(\"world\")\n```";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);

        let rgb_pattern = regex::Regex::new(r"\x1b\[38;2;\d+;\d+;\d+m").unwrap();
        let has_rgb_colors = rgb_pattern.is_match(&output);

        assert!(
            has_rgb_colors,
            "Python code should have RGB syntax colors. Output:\n{}",
            output.escape_debug()
        );
    }

    #[test]
    fn unknown_language_still_renders() {
        let md = "```unknownlang\nsome code here\n```";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);

        assert!(output.contains("some code here"));
    }

    #[test]
    fn code_block_without_language_renders() {
        let md = "```\nplain code\n```";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);

        assert!(output.contains("plain code"));
    }

    #[test]
    fn javascript_code_block_has_highlighting() {
        let md = "```js\nconst x = 42;\nfunction test() { return x; }\n```";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);

        let rgb_pattern = regex::Regex::new(r"\x1b\[38;2;\d+;\d+;\d+m").unwrap();
        assert!(
            rgb_pattern.is_match(&output),
            "JavaScript code should have syntax highlighting"
        );
    }

    #[test]
    fn highlighted_code_preserves_content() {
        let md = "```rust\nlet answer = 42;\n```";
        let node = markdown_to_node(md);
        let output = render_to_string(&node, 80);

        assert!(output.contains("let"), "Should contain 'let' keyword");
        assert!(output.contains("answer"), "Should contain variable name");
        assert!(output.contains("42"), "Should contain number literal");
    }
}

#[test]
fn markdown_with_bullet_and_list_does_not_panic() {
    let md = "● I don't have the ability to look directly into your local file system, but I can definitely help you understand the structure and contents of a repo if you share them with me. Here's what you can do:\n\n   1. Copy the folder tree";

    let result = std::panic::catch_unwind(|| markdown_to_node(md));

    assert!(
        result.is_ok(),
        "Should not panic on bullet with apostrophe and list"
    );
}

#[test]
fn code_block_fence_markers_not_duplicated() {
    // Complete code block - fence markers should appear exactly twice (open + close)
    let md = "```bash\ngit clone repo\n```";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let backtick_count = output.matches("```").count();
    assert_eq!(
        backtick_count, 2,
        "Expected exactly 2 fence markers (open + close), got {}. Output:\n{}",
        backtick_count, output
    );
}

#[test]
fn code_block_with_language_renders_fence_correctly() {
    let md = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let backtick_count = output.matches("```").count();
    assert_eq!(
        backtick_count, 2,
        "Expected exactly 2 fence markers, got {}. Output:\n{}",
        backtick_count, output
    );
    assert!(output.contains("rust"), "Should contain language tag");
}

#[test]
fn unclosed_code_fence_no_extra_markers() {
    // Simulates streaming: opening fence received but no closing fence yet
    let md = "```bash\ngit clone repo";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let backtick_count = output.matches("```").count();
    // Unclosed fence: markdown-it auto-closes it, so we expect 2 (open + close)
    // OR if it doesn't parse as a fence at all, 0
    assert!(
        backtick_count <= 2,
        "Unclosed fence should not produce more than 2 markers, got {}. Output:\n{}",
        backtick_count,
        output
    );
}

#[test]
fn two_consecutive_code_blocks_no_tripled_fences() {
    // Two code blocks back-to-back
    let md = "```bash\nls -la\n```\n\n```bash\ncat file\n```";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let backtick_count = output.matches("```").count();
    assert_eq!(
        backtick_count, 4,
        "Two code blocks should have 4 fence markers, got {}. Output:\n{}",
        backtick_count, output
    );
}

#[test]
fn code_block_with_blank_line_inside() {
    // Code block containing a blank line (which is also the \n\n block separator)
    let md = "```\nline1\n\nline3\n```";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let backtick_count = output.matches("```").count();
    assert_eq!(
        backtick_count, 2,
        "Code block with blank line should still have exactly 2 fence markers, got {}. Output:\n{}",
        backtick_count, output
    );
}

#[test]
fn adjacent_code_blocks_no_separator() {
    // Adjacent code blocks with only a single newline between them (no \n\n)
    let md = "```\nblock1\n```\n```\nblock2\n```";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let backtick_count = output.matches("```").count();
    assert_eq!(
        backtick_count, 4,
        "Adjacent code blocks should have 4 fence markers, got {}. Output:\n{}",
        backtick_count, output
    );
}
#[test]
fn code_blocks_with_text_between_render_all_fences() {
    // Multiple code blocks separated by paragraph text — each should render with fences
    let md = "## Quick Commands\n\n\
               ```bash\n# Chat\ncru chat\n```\n\n\
               Chat with Claude Code\n\n\
               ```bash\ncru chat -a claude\n```\n\n\
               Start MCP server\n\n\
               ```bash\ncru mcp\n```";
    let node = markdown_to_node(md);
    let output = render_to_string(&node, 80);
    let backtick_count = output.matches("```").count();
    assert_eq!(
        backtick_count, 6,
        "Three code blocks should have 6 fence markers, got {}. Output:\n{}",
        backtick_count, output
    );
}

#[test]
fn table_respects_terminal_width() {
    let table = r#"| Column One | Column Two | Column Three | Column Four | Column Five |
|------------|------------|--------------|-------------|-------------|
| Data A     | Data B     | Data C       | Data D      | Data E      |
| More data  | More data  | More data    | More data   | More data   |"#;

    let style = RenderStyle::viewport_with_margins(60, Margins::assistant());
    let node = markdown_to_node_styled(table, style);
    let output = render_to_string(&node, 60);

    for (i, line) in output.lines().enumerate() {
        let width = visible_width(line);
        assert!(
            width <= 60,
            "Line {} exceeds width 60: {} chars\n{:?}",
            i + 1,
            width,
            line
        );
    }
}

#[test]
fn table_with_cjk_content_respects_width() {
    let table = r#"| 名前 | 説明 | 値 |
|------|------|-----|
| テスト | これはテストです | 123 |
| データ | サンプルデータ | 456 |"#;

    let style = RenderStyle::viewport_with_margins(60, Margins::assistant());
    let node = markdown_to_node_styled(table, style);
    let output = render_to_string(&node, 60);

    // Should not panic
    for (i, line) in output.lines().enumerate() {
        let width = visible_width(line);
        assert!(
            width <= 60,
            "CJK table line {} exceeds width 60: {} chars\n{:?}",
            i + 1,
            width,
            line
        );
    }
}

#[test]
fn table_at_narrow_width_has_complete_box_drawing() {
    let table = r#"| Column A | Column B |
|----------|----------|
| Data 1   | Data 2   |"#;

    let style = RenderStyle::viewport_with_margins(40, Margins::assistant());
    let node = markdown_to_node_styled(table, style);
    let output = render_to_string(&node, 40);

    // Check for complete box-drawing characters
    let has_top_left = output.contains('┌');
    let has_top_right = output.contains('┐');
    let has_bottom_left = output.contains('└');
    let has_bottom_right = output.contains('┘');
    let has_vertical = output.contains('│');
    let has_horizontal = output.contains('─');

    assert!(has_top_left, "Missing top-left corner ┌");
    assert!(has_top_right, "Missing top-right corner ┐");
    assert!(has_bottom_left, "Missing bottom-left corner └");
    assert!(has_bottom_right, "Missing bottom-right corner ┘");
    assert!(has_vertical, "Missing vertical line │");
    assert!(has_horizontal, "Missing horizontal line ─");

    // Verify all lines fit within width
    for (i, line) in output.lines().enumerate() {
        let width = visible_width(line);
        assert!(
            width <= 40,
            "Line {} exceeds width 40: {} chars\n{:?}",
            i + 1,
            width,
            line
        );
    }
}
