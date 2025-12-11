//! Completion popup widget
//!
//! Renders the fuzzy completion popup overlay.

use crate::chat_tui::completion::CompletionState;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem},
    Frame,
};

/// Maximum height of the popup (including borders)
const MAX_POPUP_HEIGHT: u16 = 10;

/// Default popup width
const POPUP_WIDTH: u16 = 40;

/// Calculate the area for the completion popup
///
/// Positions the popup above the input area, accounting for:
/// - Number of items to display
/// - Available screen space
/// - Trigger column position
///
/// # Arguments
/// * `state` - The completion state containing items and trigger position
/// * `area` - The full terminal area
///
/// # Returns
/// A `Rect` representing the popup area, or a zero-sized rect if no items
pub fn calculate_popup_area(state: &CompletionState, area: Rect) -> Rect {
    if state.filtered_items.is_empty() {
        return Rect::default();
    }

    // Calculate popup dimensions
    let popup_height = (state.filtered_items.len() as u16).min(MAX_POPUP_HEIGHT - 2) + 2; // +2 for borders
    let popup_width = POPUP_WIDTH.min(area.width.saturating_sub(state.trigger_column));

    // Position popup above the input area
    Rect {
        x: state.trigger_column,
        y: area.y.saturating_sub(popup_height),
        width: popup_width,
        height: popup_height,
    }
}

/// Render the completion popup overlay
pub fn render_completion_popup(frame: &mut Frame, area: Rect, state: &CompletionState) {
    if state.filtered_items.is_empty() {
        return;
    }

    let popup_area = calculate_popup_area(state, area);

    // Clear the background
    frame.render_widget(Clear, popup_area);

    // Build list items
    let items: Vec<ListItem> = state
        .filtered_items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let checkbox = if state.multi_select {
                if state.is_selected(idx) {
                    "[x] "
                } else {
                    "[ ] "
                }
            } else {
                ""
            };

            let highlight = if idx == state.selected_index {
                "> "
            } else {
                "  "
            };

            let content = format!("{}{}{}", highlight, checkbox, item.text);

            let style = if idx == state.selected_index {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(format!(" {} ", state.query))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(list, popup_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_tui::completion::{CompletionItem, CompletionType};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    fn test_items() -> Vec<CompletionItem> {
        vec![
            CompletionItem::new("search", Some("Search notes".into()), CompletionType::Command),
            CompletionItem::new("session", Some("Session management".into()), CompletionType::Command),
            CompletionItem::new("clear", Some("Clear context".into()), CompletionType::Command),
        ]
    }

    #[test]
    fn test_popup_position_calculation() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.trigger_column = 5;

        let area = Rect {
            x: 0,
            y: 20,
            width: 80,
            height: 24,
        };

        let popup_area = calculate_popup_area(&state, area);

        // Popup should be positioned at trigger column
        assert_eq!(popup_area.x, 5, "Popup should start at trigger column");

        // Popup should be above the input area
        assert!(popup_area.y < area.y, "Popup should be above input area");

        // Popup height should be items + 2 for borders
        let expected_height = 3 + 2; // 3 items + 2 borders
        assert_eq!(popup_area.height, expected_height, "Popup height should include borders");

        // Popup width should be constrained by remaining space
        assert!(popup_area.width <= area.width - state.trigger_column);
        assert_eq!(popup_area.width, POPUP_WIDTH, "Popup width should be default when space available");
    }

    #[test]
    fn test_popup_position_calculation_with_many_items() {
        let many_items: Vec<CompletionItem> = (0..20)
            .map(|i| CompletionItem::new(format!("item{}", i), None, CompletionType::Command))
            .collect();

        let mut state = CompletionState::new(many_items, CompletionType::Command);
        state.trigger_column = 0;

        let area = Rect {
            x: 0,
            y: 20,
            width: 80,
            height: 24,
        };

        let popup_area = calculate_popup_area(&state, area);

        // Popup height should be capped at MAX_POPUP_HEIGHT
        assert_eq!(popup_area.height, MAX_POPUP_HEIGHT, "Popup height should be capped at maximum");
    }

    #[test]
    fn test_popup_position_calculation_constrained_width() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.trigger_column = 60; // Near the right edge

        let area = Rect {
            x: 0,
            y: 20,
            width: 80,
            height: 24,
        };

        let popup_area = calculate_popup_area(&state, area);

        // Popup width should be constrained by available space
        let max_available_width = area.width - state.trigger_column;
        assert_eq!(popup_area.width, max_available_width, "Popup width should be constrained");
        assert!(popup_area.width < POPUP_WIDTH, "Popup should be narrower than default");
    }

    #[test]
    fn test_popup_empty_state() {
        let state = CompletionState::new(vec![], CompletionType::Command);

        let area = Rect {
            x: 0,
            y: 20,
            width: 80,
            height: 24,
        };

        let popup_area = calculate_popup_area(&state, area);

        // Empty state should return zero-sized rect
        assert_eq!(popup_area, Rect::default(), "Empty state should return default rect");

        // Render should not panic with empty state
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        // Verify nothing was rendered (buffer should be empty)
        let buffer = terminal.backend().buffer().clone();
        // With empty items, the popup should not modify the buffer
        let empty_buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
        assert_eq!(buffer, empty_buffer, "Buffer should be empty when no items");
    }

    #[test]
    fn test_popup_checkbox_display() {
        let mut state = CompletionState::new_multi(test_items(), CompletionType::File);
        state.trigger_column = 0;

        // Select first and third items
        state.toggle_selection();
        state.select_next();
        state.select_next();
        state.toggle_selection();
        state.select_next(); // Move away so we can see checkbox without highlight

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        // Find the popup area in the buffer
        let popup_area = calculate_popup_area(&state, area);

        // Check for checkboxes in the rendered content
        // First item should have [x]
        let mut found_checked = false;
        let mut found_unchecked = false;

        for y in popup_area.y..popup_area.y + popup_area.height {
            let line: String = (popup_area.x..popup_area.x + popup_area.width)
                .map(|x| buffer.get(x, y).symbol())
                .collect();

            if line.contains("[x]") {
                found_checked = true;
            }
            if line.contains("[ ]") {
                found_unchecked = true;
            }
        }

        assert!(found_checked, "Should find checked checkbox [x] in multi-select mode");
        assert!(found_unchecked, "Should find unchecked checkbox [ ] in multi-select mode");
    }

    #[test]
    fn test_popup_checkbox_display_single_select() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.trigger_column = 0;

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        // Check that no checkboxes appear in single-select mode
        for y in popup_area.y..popup_area.y + popup_area.height {
            let line: String = (popup_area.x..popup_area.x + popup_area.width)
                .map(|x| buffer.get(x, y).symbol())
                .collect();

            assert!(!line.contains("[x]"), "Should not find checkboxes in single-select mode");
            assert!(!line.contains("[ ]"), "Should not find checkboxes in single-select mode");
        }
    }

    #[test]
    fn test_popup_selection_highlight() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.trigger_column = 0;

        // Select the second item
        state.select_next();

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        // Check for the selection indicator ">"
        let mut found_highlight = false;
        let mut highlight_count = 0;

        for y in popup_area.y..popup_area.y + popup_area.height {
            let line: String = (popup_area.x..popup_area.x + popup_area.width)
                .map(|x| buffer.get(x, y).symbol())
                .collect();

            if line.contains("> ") {
                found_highlight = true;
                highlight_count += 1;
            }
        }

        assert!(found_highlight, "Should find selection highlight '>' indicator");
        assert_eq!(highlight_count, 1, "Should have exactly one highlighted item");

        // Verify that the highlighted item has the correct style (DarkGray background)
        // Note: TestBackend doesn't provide easy style inspection, so we verify the indicator instead
    }

    #[test]
    fn test_popup_renders_border_and_title() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.query = "test".to_string();
        state.trigger_column = 0;

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        // Check for border characters (ratatui uses box drawing characters)
        // Top-left corner should have a border character
        let top_left = buffer.get(popup_area.x, popup_area.y);
        assert_ne!(top_left.symbol(), " ", "Should have border at top-left");

        // Check for title in the rendered content
        let title_line: String = (popup_area.x..popup_area.x + popup_area.width)
            .map(|x| buffer.get(x, popup_area.y).symbol())
            .collect();

        assert!(title_line.contains("test"), "Title should contain query text");
    }

    #[test]
    fn test_popup_renders_item_text() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.trigger_column = 0;

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        // Collect all text from the popup area
        let mut popup_text = String::new();
        for y in popup_area.y..popup_area.y + popup_area.height {
            for x in popup_area.x..popup_area.x + popup_area.width {
                popup_text.push_str(buffer.get(x, y).symbol());
            }
        }

        // Verify that item text appears in the popup
        assert!(popup_text.contains("search"), "Popup should contain 'search' item");
        assert!(popup_text.contains("session"), "Popup should contain 'session' item");
        assert!(popup_text.contains("clear"), "Popup should contain 'clear' item");
    }

    #[test]
    fn test_popup_does_not_panic_with_zero_area() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.trigger_column = 0;

        // Create a zero-sized area
        let area = Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        // Should not panic
        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();
    }

    #[test]
    fn test_popup_navigation_updates_highlight() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.trigger_column = 0;

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        // Initial state: first item selected
        assert_eq!(state.selected_index, 0);

        // Move to second item
        state.select_next();
        assert_eq!(state.selected_index, 1);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        // The render should complete without panic and show the second item highlighted
        // (detailed verification is covered in test_popup_selection_highlight)
    }

    // Enhanced checkbox visual tests (T12)

    #[test]
    fn test_popup_checkbox_checked_display() {
        let mut state = CompletionState::new_multi(test_items(), CompletionType::File);
        state.trigger_column = 0;

        // Select first and third items
        state.toggle_selection(); // Select index 0
        state.select_next(); // Move to index 1
        state.select_next(); // Move to index 2
        state.toggle_selection(); // Select index 2

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        // Collect all lines in the popup
        let mut lines = Vec::new();
        for y in popup_area.y..popup_area.y + popup_area.height {
            let line: String = (popup_area.x..popup_area.x + popup_area.width)
                .map(|x| buffer.get(x, y).symbol())
                .collect();
            lines.push(line);
        }

        // Verify checkboxes appear
        let all_text = lines.join("\n");
        assert!(
            all_text.contains("[x]"),
            "Should render checked checkbox [x] for selected items"
        );
        assert!(
            all_text.contains("[ ]"),
            "Should render unchecked checkbox [ ] for unselected items"
        );

        // Count the checkboxes (should have 3 total: 2 checked, 1 unchecked)
        let checked_count = all_text.matches("[x]").count();
        let unchecked_count = all_text.matches("[ ]").count();
        assert_eq!(checked_count, 2, "Should have 2 checked checkboxes");
        assert_eq!(unchecked_count, 1, "Should have 1 unchecked checkbox");
    }

    #[test]
    fn test_popup_checkbox_not_shown_single_select() {
        let mut state = CompletionState::new(test_items(), CompletionType::Command);
        state.trigger_column = 0;
        // Ensure this is NOT multi-select
        assert!(!state.multi_select, "Test requires single-select mode");

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        // Check that NO checkboxes appear in single-select mode
        for y in popup_area.y..popup_area.y + popup_area.height {
            let line: String = (popup_area.x..popup_area.x + popup_area.width)
                .map(|x| buffer.get(x, y).symbol())
                .collect();

            assert!(
                !line.contains("[x]"),
                "Single-select mode should not show checked checkboxes: '{}'",
                line
            );
            assert!(
                !line.contains("[ ]"),
                "Single-select mode should not show unchecked checkboxes: '{}'",
                line
            );
        }

        // Verify items are still rendered, just without checkboxes
        let mut found_items = false;
        for y in popup_area.y..popup_area.y + popup_area.height {
            let line: String = (popup_area.x..popup_area.x + popup_area.width)
                .map(|x| buffer.get(x, y).symbol())
                .collect();

            if line.contains("search") || line.contains("session") || line.contains("clear") {
                found_items = true;
            }
        }
        assert!(found_items, "Items should still be rendered in single-select mode");
    }

    #[test]
    fn test_popup_checkbox_with_highlight() {
        let mut state = CompletionState::new_multi(test_items(), CompletionType::File);
        state.trigger_column = 0;

        // Select the first item
        state.toggle_selection();

        // Move to second item (which will be highlighted but not checked)
        state.select_next();

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        // Find the highlighted line (should contain ">")
        let mut highlighted_line = String::new();
        let mut checked_line = String::new();

        for y in popup_area.y..popup_area.y + popup_area.height {
            let line: String = (popup_area.x..popup_area.x + popup_area.width)
                .map(|x| buffer.get(x, y).symbol())
                .collect();

            if line.contains("> ") {
                highlighted_line = line.clone();
            }
            if line.contains("[x]") {
                checked_line = line.clone();
            }
        }

        // The highlighted line should have ">" but NOT be checked (it's the second item)
        assert!(
            highlighted_line.contains("> "),
            "Should find highlighted line with '>'"
        );
        assert!(
            highlighted_line.contains("[ ]"),
            "Highlighted line should have unchecked box (second item not selected)"
        );

        // The checked line should NOT have ">" (it's the first item, not highlighted)
        assert!(checked_line.contains("[x]"), "Should find checked line with '[x]'");
        assert!(
            !checked_line.contains("> "),
            "Checked line should not have '>' (first item not highlighted)"
        );
    }

    #[test]
    fn test_popup_all_items_checked() {
        let mut state = CompletionState::new_multi(test_items(), CompletionType::File);
        state.trigger_column = 0;

        // Select all items
        state.toggle_selection(); // Select index 0
        state.select_next();
        state.toggle_selection(); // Select index 1
        state.select_next();
        state.toggle_selection(); // Select index 2

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        let all_text: String = (popup_area.y..popup_area.y + popup_area.height)
            .map(|y| {
                (popup_area.x..popup_area.x + popup_area.width)
                    .map(|x| buffer.get(x, y).symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        // All items should be checked
        let checked_count = all_text.matches("[x]").count();
        let unchecked_count = all_text.matches("[ ]").count();
        assert_eq!(
            checked_count, 3,
            "All 3 items should be checked: {}",
            all_text
        );
        assert_eq!(unchecked_count, 0, "No items should be unchecked");
    }

    #[test]
    fn test_popup_no_items_checked() {
        let mut state = CompletionState::new_multi(test_items(), CompletionType::File);
        state.trigger_column = 0;

        // Don't select any items

        let area = Rect {
            x: 0,
            y: 10,
            width: 80,
            height: 24,
        };

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                render_completion_popup(frame, area, &state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let popup_area = calculate_popup_area(&state, area);

        let all_text: String = (popup_area.y..popup_area.y + popup_area.height)
            .map(|y| {
                (popup_area.x..popup_area.x + popup_area.width)
                    .map(|x| buffer.get(x, y).symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");

        // No items should be checked
        let checked_count = all_text.matches("[x]").count();
        let unchecked_count = all_text.matches("[ ]").count();
        assert_eq!(checked_count, 0, "No items should be checked");
        assert_eq!(
            unchecked_count, 3,
            "All 3 items should be unchecked: {}",
            all_text
        );
    }
}
