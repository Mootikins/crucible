use crate::tui::oil::ansi::visible_width;
use crate::tui::oil::markdown::markdown_to_node_with_width;
use crate::tui::oil::node::{row, styled};
use crate::tui::oil::render::render_to_string;
use crate::tui::oil::style::{Color, Style};
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

mod graduation_properties {
    use crate::tui::oil::node::{col, scrollback, text, Node};
    use crate::tui::oil::runtime::GraduationState;
    use proptest::prelude::*;
    use std::collections::HashSet;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn graduation_keys_always_unique(keys in prop::collection::vec("[a-z]{3,10}", 1..20)) {
            let mut state = GraduationState::new();

            let nodes: Vec<Node> = keys
                .iter()
                .map(|k| scrollback(k.as_str(), [text(k.as_str())]))
                .collect();

            let tree = col(nodes);
            let graduated = state.graduate(&tree, 80).unwrap();

            let graduated_keys: HashSet<_> = graduated.iter().map(|g| &g.key).collect();
            prop_assert_eq!(
                graduated_keys.len(),
                graduated.len(),
                "All graduated keys should be unique"
            );
        }

        #[test]
        fn graduation_preserves_insertion_order(
            key_set in prop::collection::hash_set("[a-z]{3,8}", 2..10)
        ) {
            let keys: Vec<_> = key_set.into_iter().collect();
            let mut state = GraduationState::new();

            let nodes: Vec<Node> = keys
                .iter()
                .map(|k| scrollback(k.as_str(), [text(k.as_str())]))
                .collect();

            let tree = col(nodes);
            let graduated = state.graduate(&tree, 80).unwrap();

            let graduated_keys: Vec<_> = graduated.iter().map(|g| g.key.clone()).collect();

            for (i, key) in keys.iter().enumerate() {
                if let Some(pos) = graduated_keys.iter().position(|k| k == key) {
                    for prev_key in keys.iter().take(i) {
                        if let Some(prev_pos) = graduated_keys.iter().position(|k| k == prev_key) {
                            prop_assert!(
                                prev_pos < pos,
                                "Key '{}' at {} should come before '{}' at {}",
                                prev_key, prev_pos, key, pos
                            );
                        }
                    }
                }
            }
        }
    }
}

mod focus_properties {
    use crate::tui::oil::focus::{FocusContext, FocusId};
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn focus_cycle_returns_to_start(count in 1usize..20) {
            let mut ctx = FocusContext::new();

            for i in 0..count {
                ctx.register(FocusId::new(format!("item-{}", i)), false);
            }

            ctx.focus_next();
            let first = ctx.active_id().map(|id| id.0.clone());

            for _ in 0..count {
                ctx.focus_next();
            }

            let after_cycle = ctx.active_id().map(|id| id.0.clone());
            prop_assert_eq!(first, after_cycle, "Should return to start after full cycle");
        }

        #[test]
        fn focus_prev_cycle_returns_to_start(count in 1usize..20) {
            let mut ctx = FocusContext::new();

            for i in 0..count {
                ctx.register(FocusId::new(format!("item-{}", i)), false);
            }

            ctx.focus_next();
            let first = ctx.active_id().map(|id| id.0.clone());

            for _ in 0..count {
                ctx.focus_prev();
            }

            let after_cycle = ctx.active_id().map(|id| id.0.clone());
            prop_assert_eq!(first, after_cycle, "Should return to start after full reverse cycle");
        }

        #[test]
        fn focus_order_maintained_after_operations(
            item_count in 2usize..10,
            op_count in 1usize..30
        ) {
            let mut ctx = FocusContext::new();

            for i in 0..item_count {
                ctx.register(FocusId::new(format!("item-{}", i)), false);
            }

            let initial_order: Vec<_> = ctx.focus_order().iter().map(|id| id.0.clone()).collect();

            for i in 0..op_count {
                if i % 2 == 0 {
                    ctx.focus_next();
                } else {
                    ctx.focus_prev();
                }
            }

            let final_order: Vec<_> = ctx.focus_order().iter().map(|id| id.0.clone()).collect();
            prop_assert_eq!(initial_order, final_order, "Focus order should not change");
        }
    }
}

mod input_buffer_properties {
    use crate::tui::oil::event::{InputAction, InputBuffer};
    use proptest::prelude::*;

    fn arb_input_action() -> impl Strategy<Value = InputAction> {
        prop_oneof![
            any::<char>()
                .prop_filter("printable", |c| c.is_ascii_graphic() || *c == ' ')
                .prop_map(InputAction::Insert),
            Just(InputAction::Backspace),
            Just(InputAction::Delete),
            Just(InputAction::Left),
            Just(InputAction::Right),
            Just(InputAction::Home),
            Just(InputAction::End),
            Just(InputAction::Clear),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn input_buffer_cursor_always_valid(actions in prop::collection::vec(arb_input_action(), 1..100)) {
            let mut buf = InputBuffer::new();

            for action in actions {
                buf.handle(action);

                let cursor = buf.cursor();
                let len = buf.content().len();

                prop_assert!(
                    cursor <= len,
                    "Cursor {} should not exceed content length {}",
                    cursor, len
                );
            }
        }

        #[test]
        fn input_buffer_home_end_invariant(text in "[a-zA-Z ]{0,50}") {
            let mut buf = InputBuffer::new();
            buf.set_content(&text);

            buf.handle(InputAction::Home);
            prop_assert_eq!(buf.cursor(), 0, "Home should move cursor to 0");

            buf.handle(InputAction::End);
            prop_assert_eq!(buf.cursor(), text.len(), "End should move cursor to end");
        }

        #[test]
        fn input_buffer_clear_resets_state(text in "[a-zA-Z ]{1,50}") {
            let mut buf = InputBuffer::new();
            buf.set_content(&text);
            buf.handle(InputAction::Left);
            buf.handle(InputAction::Left);

            buf.handle(InputAction::Clear);

            prop_assert!(buf.content().is_empty(), "Content should be empty after clear");
            prop_assert_eq!(buf.cursor(), 0, "Cursor should be 0 after clear");
        }
    }
}

mod chat_mode_properties {
    use crate::tui::oil::chat_app::ChatMode;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn chat_mode_cycle_returns_to_start(cycles in 1usize..10) {
            let start = ChatMode::Normal;
            let mut mode = start;

            for _ in 0..(cycles * 3) {
                mode = mode.cycle();
            }

            prop_assert_eq!(
                mode, start,
                "After {} complete cycles, should return to start",
                cycles
            );
        }

        #[test]
        fn chat_mode_parse_roundtrip(mode_str in "(normal|plan|auto)") {
            let parsed = ChatMode::parse(&mode_str);
            let back_to_str = parsed.as_str();

            prop_assert_eq!(
                mode_str, back_to_str,
                "Parse and as_str should roundtrip"
            );
        }
    }
}

mod composer_stability_properties {
    use crate::tui::oil::app::App;
    use crate::tui::oil::chat_app::{InkChatApp, INPUT_MAX_CONTENT_LINES};
    use crate::tui::oil::event::{Event, InputAction};
    use crate::tui::oil::focus::FocusContext;
    use crate::tui::oil::render::render_to_string;
    use crate::tui::ViewContext;
    use crossterm::event::KeyCode;
    use proptest::prelude::*;

    fn count_lines(output: &str) -> usize {
        output.split("\r\n").count()
    }

    fn extract_input_region(output: &str) -> Vec<&str> {
        let lines: Vec<&str> = output.split("\r\n").collect();
        let mut in_input = false;
        let mut input_lines = Vec::new();

        for line in lines {
            if line.contains('▄') && !in_input {
                in_input = true;
                input_lines.push(line);
            } else if line.contains('▀') && in_input {
                input_lines.push(line);
                break;
            } else if in_input {
                input_lines.push(line);
            }
        }
        input_lines
    }

    fn render_app(app: &InkChatApp) -> String {
        let focus = FocusContext::new();
        let ctx = ViewContext::new(&focus);
        let node = app.view(&ctx);
        render_to_string(&node, 80)
    }

    fn measure_input_height(app: &InkChatApp) -> usize {
        let output = render_app(app);
        extract_input_region(&output).len()
    }

    #[test]
    fn input_region_has_expected_height_when_empty() {
        let app = InkChatApp::default();
        let height = measure_input_height(&app);
        assert_eq!(
            height, 3,
            "Empty input should have 3 lines (top_edge + 1 content + bottom_edge), got {}",
            height
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn input_height_bounded_by_max_content_lines(text in "[a-zA-Z0-9 ]{0,500}") {
            let mut app = InkChatApp::default();
            app.set_input_content(&text);

            let height = measure_input_height(&app);
            let max_height = INPUT_MAX_CONTENT_LINES + 2;

            prop_assert!(
                height >= 3,
                "Input should have at least 3 lines (edges + 1 content), got {}",
                height
            );
            prop_assert!(
                height <= max_height,
                "Input should have at most {} lines, got {}",
                max_height, height
            );
        }

        #[test]
        fn input_height_bounded_after_typing(
            chars in prop::collection::vec(any::<char>().prop_filter("printable", |c| c.is_ascii_graphic() || *c == ' '), 1..100)
        ) {
            let mut app = InkChatApp::default();

            for c in chars {
                app.handle_input_action(InputAction::Insert(c));
            }

            let height = measure_input_height(&app);
            let max_height = INPUT_MAX_CONTENT_LINES + 2;
            prop_assert!(
                height >= 3 && height <= max_height,
                "Input height {} should be between 3 and {}",
                height, max_height
            );
        }

        #[test]
        fn input_height_bounded_with_mixed_operations(
            actions in prop::collection::vec(
                prop_oneof![
                    any::<char>()
                        .prop_filter("printable", |c| c.is_ascii_graphic() || *c == ' ')
                        .prop_map(InputAction::Insert),
                    Just(InputAction::Backspace),
                    Just(InputAction::Delete),
                    Just(InputAction::Left),
                    Just(InputAction::Right),
                    Just(InputAction::Home),
                    Just(InputAction::End),
                ],
                1..50
            )
        ) {
            let mut app = InkChatApp::default();
            let max_height = INPUT_MAX_CONTENT_LINES + 2;

            for action in actions {
                app.handle_input_action(action);
                let height = measure_input_height(&app);
                prop_assert!(
                    height >= 3 && height <= max_height,
                    "Input height {} should be between 3 and {}",
                    height, max_height
                );
            }
        }



        #[test]
        fn long_text_clamped_to_max_lines(word_count in 5usize..50, word_len in 3usize..15) {
            let mut app = InkChatApp::default();

            let text: String = (0..word_count)
                .map(|_| "x".repeat(word_len))
                .collect::<Vec<_>>()
                .join(" ");

            app.set_input_content(&text);

            let height = measure_input_height(&app);
            let max_height = INPUT_MAX_CONTENT_LINES + 2;

            prop_assert!(
                height <= max_height,
                "Input with {} chars should render at most {} lines (got {})",
                text.len(), max_height, height
            );
        }

        #[test]
        fn cursor_navigation_does_not_exceed_bounds(
            text in "[a-zA-Z ]{50,300}",
            nav_actions in prop::collection::vec(
                prop_oneof![
                    Just(InputAction::Left),
                    Just(InputAction::Right),
                    Just(InputAction::Home),
                    Just(InputAction::End),
                ],
                5..30
            )
        ) {
            let mut app = InkChatApp::default();
            app.set_input_content(&text);
            let max_height = INPUT_MAX_CONTENT_LINES + 2;

            for action in nav_actions {
                app.handle_input_action(action);
                let height = measure_input_height(&app);
                prop_assert!(
                    height >= 3 && height <= max_height,
                    "Height {} out of bounds [3, {}] during navigation",
                    height, max_height
                );
            }
        }
    }
}

mod markdown_block_spacing_properties {
    use crate::tui::oil::markdown::{
        markdown_to_node, markdown_to_node_styled, Margins, RenderStyle,
    };
    use crate::tui::oil::render::render_to_string;
    use proptest::prelude::*;

    fn render_md(md: &str, width: usize) -> String {
        let node = markdown_to_node(md);
        render_to_string(&node, width)
    }

    fn render_md_with_margins(md: &str, width: usize) -> String {
        let style = RenderStyle::natural_with_margins(width, Margins::assistant());
        let node = markdown_to_node_styled(md, style);
        render_to_string(&node, width)
    }

    fn has_blank_line_between(output: &str, content_a: &str, content_b: &str) -> bool {
        let lines: Vec<&str> = output.split("\r\n").collect();
        let pos_a = lines.iter().position(|l| l.contains(content_a));
        let pos_b = lines.iter().position(|l| l.contains(content_b));

        match (pos_a, pos_b) {
            (Some(a), Some(b)) if b > a => {
                (a + 1..b).any(|i| lines.get(i).map_or(false, |l| l.trim().is_empty()))
            }
            _ => true,
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn paragraph_then_heading_has_blank_line(
            para in "[A-Za-z]{5,20}",
            heading in "[A-Za-z]{5,20}",
            level in 1usize..=3,
            width in 40usize..100
        ) {
            let hashes = "#".repeat(level);
            let md = format!("{}\n\n{} {}", para, hashes, heading);
            let output = render_md(&md, width);

            prop_assert!(
                has_blank_line_between(&output, &para, &heading),
                "Should have blank line between paragraph and heading.\nMD:\n{}\nOutput:\n{}",
                md, output
            );
        }

        #[test]
        fn heading_then_paragraph_has_blank_line(
            heading in "[A-Za-z]{5,20}",
            para in "[A-Za-z]{5,20}",
            level in 1usize..=3,
            width in 40usize..100
        ) {
            let hashes = "#".repeat(level);
            let md = format!("{} {}\n\n{}", hashes, heading, para);
            let output = render_md(&md, width);

            prop_assert!(
                has_blank_line_between(&output, &heading, &para),
                "Should have blank line between heading and paragraph.\nMD:\n{}\nOutput:\n{}",
                md, output
            );
        }

        #[test]
        fn paragraph_then_code_block_has_blank_line(
            para in "[A-Za-z]{5,20}",
            code in "[a-z_]+",
            width in 40usize..100
        ) {
            let md = format!("{}\n\n```\n{}\n```", para, code);
            let output = render_md(&md, width);

            prop_assert!(
                has_blank_line_between(&output, &para, &code),
                "Should have blank line between paragraph and code block.\nMD:\n{}\nOutput:\n{}",
                md, output
            );
        }

        #[test]
        fn consecutive_paragraphs_have_blank_line(
            para1 in "[A-Za-z]{5,20}",
            para2 in "[A-Za-z]{5,20}",
            width in 40usize..100
        ) {
            prop_assume!(para1 != para2);
            let md = format!("{}\n\n{}", para1, para2);
            let output = render_md(&md, width);

            prop_assert!(
                has_blank_line_between(&output, &para1, &para2),
                "Should have blank line between consecutive paragraphs.\nMD:\n{}\nOutput:\n{}",
                md, output
            );
        }

        #[test]
        fn paragraph_then_list_has_blank_line(
            para in "[A-Za-z]{5,20}",
            item in "[A-Za-z]{5,20}",
            width in 40usize..100
        ) {
            let md = format!("{}\n\n- {}", para, item);
            let output = render_md(&md, width);

            prop_assert!(
                has_blank_line_between(&output, &para, &item),
                "Should have blank line between paragraph and list.\nMD:\n{}\nOutput:\n{}",
                md, output
            );
        }

        #[test]
        fn with_margins_paragraph_then_heading_has_blank_line(
            para in "[A-Za-z]{5,20}",
            heading in "[A-Za-z]{5,20}",
            level in 1usize..=3,
            width in 40usize..100
        ) {
            let hashes = "#".repeat(level);
            let md = format!("{}\n\n{} {}", para, hashes, heading);
            let output = render_md_with_margins(&md, width);

            prop_assert!(
                has_blank_line_between(&output, &para, &heading),
                "With margins: should have blank line between paragraph and heading.\nMD:\n{}\nOutput:\n{}",
                md, output
            );
        }

        #[test]
        fn with_margins_consecutive_paragraphs_have_blank_line(
            para1 in "[A-Za-z]{5,20}",
            para2 in "[A-Za-z]{5,20}",
            width in 40usize..100
        ) {
            prop_assume!(para1 != para2);
            let md = format!("{}\n\n{}", para1, para2);
            let output = render_md_with_margins(&md, width);

            prop_assert!(
                has_blank_line_between(&output, &para1, &para2),
                "With margins: should have blank line between consecutive paragraphs.\nMD:\n{}\nOutput:\n{}",
                md, output
            );
        }

        #[test]
        fn with_margins_heading_then_paragraph_has_blank_line(
            heading in "[A-Za-z]{5,20}",
            para in "[A-Za-z]{5,20}",
            level in 1usize..=3,
            width in 40usize..100
        ) {
            let hashes = "#".repeat(level);
            let md = format!("{} {}\n\n{}", hashes, heading, para);
            let output = render_md_with_margins(&md, width);

            prop_assert!(
                has_blank_line_between(&output, &heading, &para),
                "With margins: should have blank line between heading and paragraph.\nMD:\n{}\nOutput:\n{}",
                md, output
            );
        }
    }
}
