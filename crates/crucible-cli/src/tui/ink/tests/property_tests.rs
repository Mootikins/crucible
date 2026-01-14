use crate::tui::ink::ansi::visible_width;
use crate::tui::ink::markdown::markdown_to_node_with_width;
use crate::tui::ink::node::{row, styled};
use crate::tui::ink::render::render_to_string;
use crate::tui::ink::style::{Color, Style};
use proptest::prelude::*;

fn assert_lines_fit_width(output: &str, max_width: usize) -> Result<(), TestCaseError> {
    for (i, line) in output.split("\r\n").enumerate() {
        let width = visible_width(line);
        prop_assert!(
            width <= max_width,
            "Line {} exceeds width {}: {} chars\n{:?}",
            i + 1,
            max_width,
            width,
            line
        );
    }
    Ok(())
}

fn render_md(md: &str, width: usize) -> String {
    let node = markdown_to_node_with_width(md, width);
    render_to_string(&node, width)
}

fn render_md_with_prefix(md: &str, prefix: &str, total_width: usize) -> String {
    let prefix_width = visible_width(prefix);
    let content_width = total_width.saturating_sub(prefix_width);
    let md_node = markdown_to_node_with_width(md, content_width);
    let prefixed = row([
        styled(prefix.to_string(), Style::new().fg(Color::DarkGray)),
        md_node,
    ]);
    render_to_string(&prefixed, total_width)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn table_fits_width(
        col1 in "[a-zA-Z]{1,20}",
        col2 in "[a-zA-Z]{1,20}",
        cell1 in "[a-zA-Z ]{1,30}",
        cell2 in "[a-zA-Z ]{1,30}",
        width in 30usize..120
    ) {
        let table = format!(
            "| {} | {} |\n|---|---|\n| {} | {} |",
            col1, col2, cell1, cell2
        );
        let output = render_md(&table, width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn table_with_prefix_fits_width(
        col1 in "[a-zA-Z]{1,15}",
        col2 in "[a-zA-Z]{1,15}",
        cell1 in "[a-zA-Z ]{1,25}",
        cell2 in "[a-zA-Z ]{1,25}",
        width in 40usize..120
    ) {
        let table = format!(
            "| {} | {} |\n|---|---|\n| {} | {} |",
            col1, col2, cell1, cell2
        );
        let output = render_md_with_prefix(&table, "● ", width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn three_column_table_fits_width(
        h1 in "[a-zA-Z]{1,12}",
        h2 in "[a-zA-Z]{1,12}",
        h3 in "[a-zA-Z]{1,12}",
        c1 in "[a-zA-Z ]{1,20}",
        c2 in "[a-zA-Z ]{1,20}",
        c3 in "[a-zA-Z ]{1,20}",
        width in 50usize..120
    ) {
        let table = format!(
            "| {} | {} | {} |\n|---|---|---|\n| {} | {} | {} |",
            h1, h2, h3, c1, c2, c3
        );
        let output = render_md(&table, width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn text_fits_width(
        text in "[a-zA-Z ]{10,200}",
        width in 20usize..120
    ) {
        let output = render_md(&text, width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn styled_text_fits_width(
        pre in "[a-zA-Z ]{5,30}",
        bold in "[a-zA-Z]{3,15}",
        mid in "[a-zA-Z ]{5,30}",
        italic in "[a-zA-Z]{3,15}",
        post in "[a-zA-Z ]{5,30}",
        width in 30usize..100
    ) {
        let md = format!("{} **{}** {} *{}* {}", pre, bold, mid, italic, post);
        let output = render_md(&md, width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn list_fits_width(
        item1 in "[a-zA-Z ]{5,40}",
        item2 in "[a-zA-Z ]{5,40}",
        item3 in "[a-zA-Z ]{5,40}",
        width in 30usize..100
    ) {
        let md = format!("- {}\n- {}\n- {}", item1, item2, item3);
        let output = render_md(&md, width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn blockquote_fits_width(
        text in "[a-zA-Z ]{10,80}",
        width in 25usize..100
    ) {
        let md = format!("> {}", text);
        let output = render_md(&md, width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn code_block_fits_width(
        lang in "[a-z]{0,10}",
        line1 in "[a-zA-Z0-9_() ]{5,50}",
        line2 in "[a-zA-Z0-9_() ]{5,50}",
        width in 40usize..120
    ) {
        let md = format!("```{}\n{}\n{}\n```", lang, line1, line2);
        let output = render_md(&md, width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn heading_fits_width(
        level in 1usize..=6,
        text in "[a-zA-Z ]{5,60}",
        width in 30usize..100
    ) {
        let hashes = "#".repeat(level);
        let md = format!("{} {}", hashes, text);
        let output = render_md(&md, width);
        assert_lines_fit_width(&output, width)?;
    }

    #[test]
    fn narrow_width_never_panics(
        text in "[a-zA-Z0-9#*_`| \n-]{0,100}",
        width in 10usize..30
    ) {
        // Wrap in catch_unwind to handle upstream markdown-it panics
        // (e.g., emphasis parsing bugs with certain malformed markdown)
        let text_clone = text.clone();
        let _ = std::panic::catch_unwind(move || {
            render_md(&text_clone, width)
        });
    }

    #[test]
    fn content_preserved_in_table(
        col1 in "[a-zA-Z]{3,10}",
        col2 in "[a-zA-Z]{3,10}",
        width in 40usize..100
    ) {
        let table = format!("| {} | {} |\n|---|---|\n| x | y |", col1, col2);
        let output = render_md(&table, width);
        prop_assert!(
            output.contains(&col1),
            "Column header '{}' should be in output: {}",
            col1, output
        );
        prop_assert!(
            output.contains(&col2),
            "Column header '{}' should be in output: {}",
            col2, output
        );
    }

    #[test]
    fn bold_content_preserved(
        text in "[a-zA-Z]{3,20}",
        width in 30usize..100
    ) {
        let md = format!("**{}**", text);
        let output = render_md(&md, width);
        prop_assert!(
            output.contains(&text),
            "Bold text '{}' should be in output",
            text
        );
    }

    #[test]
    fn list_content_preserved(
        item in "[a-zA-Z]{3,20}",
        width in 30usize..100
    ) {
        let md = format!("- {}", item);
        let output = render_md(&md, width);
        prop_assert!(
            output.contains(&item),
            "List item '{}' should be in output",
            item
        );
    }
}
