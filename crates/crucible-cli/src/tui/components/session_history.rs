//! Session history widget for displaying conversation messages
//!
//! This widget renders the conversation history with support for scrolling
//! and interactive navigation through messages.

use crate::tui::{
    components::{InteractiveWidget, WidgetAction, WidgetEventResult},
    conversation::{render_item_to_lines, ConversationItem, ConversationState},
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Widget},
};

/// Widget that renders conversation history with scrolling support
///
/// # State
///
/// - `state`: Reference to the conversation state containing all messages
/// - `scroll_offset`: Number of lines scrolled up from bottom (0 = at bottom)
/// - `viewport_height`: Height of the visible area in lines
///
/// # Scrolling Behavior
///
/// - `scroll_offset = 0`: Bottom of content is visible (newest messages)
/// - `scroll_offset = N`: Scrolled N lines up from bottom
/// - Content shorter than viewport is bottom-anchored with empty space at top
pub struct SessionHistoryWidget<'a> {
    state: &'a ConversationState,
    scroll_offset: usize,
    viewport_height: u16,
}

impl<'a> SessionHistoryWidget<'a> {
    /// Create a new session history widget
    pub fn new(state: &'a ConversationState) -> Self {
        Self {
            state,
            scroll_offset: 0,
            viewport_height: 0,
        }
    }

    /// Set the scroll offset (lines from bottom)
    pub fn scroll_offset(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    /// Set the viewport height (for scroll bounds checking)
    pub fn viewport_height(mut self, height: u16) -> Self {
        self.viewport_height = height;
        self
    }

    /// Render conversation items to lines with context-aware spacing.
    ///
    /// Tool calls no longer include their own leading blank line, so we add spacing
    /// here - but skip it between consecutive tool calls to group them visually.
    fn render_to_lines(&self, width: usize) -> Vec<Line<'static>> {
        let mut all_lines = Vec::new();
        let items = self.state.items();

        for (i, item) in items.iter().enumerate() {
            // Add blank line before tool calls, but skip between consecutive tools
            if matches!(item, ConversationItem::ToolCall(_)) {
                let prev_was_tool =
                    i > 0 && matches!(items.get(i - 1), Some(ConversationItem::ToolCall(_)));

                if !prev_was_tool {
                    all_lines.push(Line::from(""));
                }
            }

            all_lines.extend(render_item_to_lines(item, width));
        }

        all_lines
    }

    /// Calculate total content height
    fn content_height(&self, width: usize) -> usize {
        self.state
            .items()
            .iter()
            .map(|item| render_item_to_lines(item, width).len())
            .sum()
    }

    /// Scroll up by the given number of lines
    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        // Clamp to content bounds if viewport_height is set
        if self.viewport_height > 0 {
            let content_width = (80usize).saturating_sub(4); // Default width minus margins
            let max_scroll = self
                .content_height(content_width)
                .saturating_sub(self.viewport_height as usize);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
        }
    }

    /// Scroll down by the given number of lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll to top of content
    pub fn scroll_to_top(&mut self) {
        if self.viewport_height > 0 {
            let content_width = (80usize).saturating_sub(4);
            let max_scroll = self
                .content_height(content_width)
                .saturating_sub(self.viewport_height as usize);
            self.scroll_offset = max_scroll;
        }
    }

    /// Scroll to bottom of content
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

impl Widget for SessionHistoryWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Content width minus prefix (" ● " = 3 chars) and right margin (1 char)
        let content_width = (area.width as usize).saturating_sub(4);
        let lines = self.render_to_lines(content_width);
        let content_height = lines.len();
        let viewport_height = area.height as usize;

        if content_height == 0 {
            return;
        }

        // Calculate the scroll position
        // scroll_offset = 0 means at bottom (newest content visible)
        // scroll_offset = N means N lines scrolled up from bottom

        if content_height <= viewport_height {
            // Content fits in viewport - render at bottom
            let empty_space = viewport_height - content_height;
            let offset_area = Rect {
                x: area.x,
                y: area.y + empty_space as u16,
                width: area.width,
                height: content_height as u16,
            };
            // No Wrap needed - termimad pre-wraps at word boundaries
            let paragraph = Paragraph::new(lines);
            paragraph.render(offset_area, buf);
        } else {
            // Content exceeds viewport - apply scroll
            // scroll_offset = 0: show last viewport_height lines
            // scroll_offset = N: show lines from (content - viewport - N) to (content - N)
            let max_scroll = content_height - viewport_height;
            let effective_scroll = self.scroll_offset.min(max_scroll);

            // Convert bottom-relative to top-relative scroll
            let top_scroll = max_scroll - effective_scroll;

            // No Wrap needed - termimad pre-wraps at word boundaries
            let paragraph = Paragraph::new(lines).scroll((top_scroll as u16, 0));
            paragraph.render(area, buf);
        }
    }
}

impl InteractiveWidget for SessionHistoryWidget<'_> {
    fn handle_event(&mut self, event: &Event) -> WidgetEventResult {
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event
        {
            match (*code, *modifiers) {
                // Ctrl+Up/Down - single line scroll
                (KeyCode::Up, KeyModifiers::CONTROL) => {
                    return WidgetEventResult::Action(WidgetAction::Scroll(1));
                }
                (KeyCode::Down, KeyModifiers::CONTROL) => {
                    return WidgetEventResult::Action(WidgetAction::Scroll(-1));
                }
                // Page Up/Down
                (KeyCode::PageUp, _) => {
                    let page_lines = self.viewport_height.saturating_sub(2) as isize;
                    return WidgetEventResult::Action(WidgetAction::Scroll(page_lines));
                }
                (KeyCode::PageDown, _) => {
                    let page_lines = self.viewport_height.saturating_sub(2) as isize;
                    return WidgetEventResult::Action(WidgetAction::Scroll(-page_lines));
                }
                // Home/End - scroll to top/bottom
                (KeyCode::Home, KeyModifiers::NONE) => {
                    return WidgetEventResult::Action(WidgetAction::ScrollTo(usize::MAX));
                }
                (KeyCode::End, KeyModifiers::NONE) => {
                    return WidgetEventResult::Action(WidgetAction::ScrollTo(0));
                }
                _ => {}
            }
        }
        WidgetEventResult::Ignored
    }

    fn focusable(&self) -> bool {
        // History widget can receive focus for scrolling
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::conversation::ConversationItem;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_widget_creation() {
        let state = ConversationState::new();
        let widget = SessionHistoryWidget::new(&state);
        assert_eq!(widget.scroll_offset, 0);
        assert_eq!(widget.viewport_height, 0);
    }

    #[test]
    fn test_scroll_offset_builder() {
        let state = ConversationState::new();
        let widget = SessionHistoryWidget::new(&state).scroll_offset(10);
        assert_eq!(widget.scroll_offset, 10);
    }

    #[test]
    fn test_viewport_height_builder() {
        let state = ConversationState::new();
        let widget = SessionHistoryWidget::new(&state).viewport_height(24);
        assert_eq!(widget.viewport_height, 24);
    }

    #[test]
    fn test_scroll_up_down() {
        // Create a conversation with enough content to allow scrolling
        let mut state = ConversationState::new();
        for i in 0..20 {
            state.push_user_message(format!("Message {}", i));
        }
        let mut widget = SessionHistoryWidget::new(&state).viewport_height(24);

        widget.scroll_up(5);
        assert!(
            widget.scroll_offset >= 5,
            "scroll_offset should be at least 5"
        );

        let prev_offset = widget.scroll_offset;
        widget.scroll_down(3);
        assert_eq!(widget.scroll_offset, prev_offset.saturating_sub(3));

        widget.scroll_down(100); // Saturating sub to 0
        assert_eq!(widget.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state).scroll_offset(100);

        widget.scroll_to_bottom();
        assert_eq!(widget.scroll_offset, 0);
    }

    #[test]
    fn test_empty_conversation_renders() {
        let state = ConversationState::new();
        let widget = SessionHistoryWidget::new(&state);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                f.render_widget(widget, area);
            })
            .unwrap();

        // Should render without panicking
    }

    #[test]
    fn test_render_with_messages() {
        let mut state = ConversationState::new();
        state.push_user_message("Hello");
        state.push_assistant_message("Hi there!");

        let widget = SessionHistoryWidget::new(&state);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                f.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().height)
            .flat_map(|y| {
                (0..buffer.area().width)
                    .map(move |x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
            })
            .collect();

        // Should contain user and assistant messages
        assert!(content.contains("Hello"));
        assert!(content.contains("Hi there!"));
    }

    #[test]
    fn test_bottom_anchored_rendering() {
        let mut state = ConversationState::new();
        state.push_user_message("Test");

        let widget = SessionHistoryWidget::new(&state);

        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                f.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();

        // Content should be at bottom, not top
        let top_line: String = (0..80)
            .map(|x| buffer.cell((x, 0)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();

        // Top line should be mostly empty
        assert!(
            top_line.trim().is_empty(),
            "Expected top line to be empty for short content"
        );

        // Bottom area should have content
        let has_content = (15..20).any(|y| {
            let line: String = (0..80)
                .map(|x| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                .collect();
            line.contains("Test")
        });

        assert!(has_content, "Expected content near bottom of viewport");
    }

    #[test]
    fn test_focusable() {
        let state = ConversationState::new();
        let widget = SessionHistoryWidget::new(&state);
        assert!(widget.focusable());
    }

    #[test]
    fn test_handle_ctrl_up_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::CONTROL));
        let result = widget.handle_event(&event);

        assert_eq!(result, WidgetEventResult::Action(WidgetAction::Scroll(1)));
    }

    #[test]
    fn test_handle_ctrl_down_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL));
        let result = widget.handle_event(&event);

        assert_eq!(result, WidgetEventResult::Action(WidgetAction::Scroll(-1)));
    }

    #[test]
    fn test_handle_page_up_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state).viewport_height(24);

        let event = Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        let result = widget.handle_event(&event);

        // Page size should be viewport_height - 2 = 22
        assert_eq!(result, WidgetEventResult::Action(WidgetAction::Scroll(22)));
    }

    #[test]
    fn test_handle_home_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        let result = widget.handle_event(&event);

        assert_eq!(
            result,
            WidgetEventResult::Action(WidgetAction::ScrollTo(usize::MAX))
        );
    }

    #[test]
    fn test_handle_end_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        let result = widget.handle_event(&event);

        assert_eq!(result, WidgetEventResult::Action(WidgetAction::ScrollTo(0)));
    }

    #[test]
    fn test_handle_unrelated_event_ignored() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let result = widget.handle_event(&event);

        assert_eq!(result, WidgetEventResult::Ignored);
    }

    // =============================================================================
    // Snapshot Tests
    // =============================================================================

    mod snapshot_tests {
        use super::*;
        use crate::tui::content_block::StreamBlock;
        use insta::assert_snapshot;

        const TEST_WIDTH: u16 = 80;
        const TEST_HEIGHT: u16 = 24;

        fn test_terminal() -> Terminal<TestBackend> {
            Terminal::new(TestBackend::new(TEST_WIDTH, TEST_HEIGHT)).unwrap()
        }

        fn render_widget(state: &ConversationState, scroll_offset: usize) -> Terminal<TestBackend> {
            let mut terminal = test_terminal();
            terminal
                .draw(|f| {
                    let widget = SessionHistoryWidget::new(state)
                        .scroll_offset(scroll_offset)
                        .viewport_height(f.area().height);
                    f.render_widget(widget, f.area());
                })
                .unwrap();
            terminal
        }

        #[test]
        fn empty_conversation() {
            let state = ConversationState::new();
            let terminal = render_widget(&state, 0);
            assert_snapshot!("session_history_empty", terminal.backend());
        }

        #[test]
        fn interleaved_prose_and_tool_calls() {
            let mut state = ConversationState::new();
            state.push_user_message("Read my note");

            state.start_assistant_streaming();
            state.append_or_create_prose("Let me read that for you.\n");
            state.complete_last_block();

            state.push_tool_running("read", serde_json::json!({"path": "note.md"}));
            state.update_tool_output("read", "Note contents here");
            state.complete_tool("read", Some("success".into()));

            state.append_or_create_prose("I've read the note.");
            state.complete_streaming();

            let terminal = render_widget(&state, 0);
            assert_snapshot!("session_history_interleaved", terminal.backend());
        }

        #[test]
        fn multiple_tool_calls_sequential() {
            let mut state = ConversationState::new();
            state.push_user_message("Search the codebase");

            state.push_tool_running("grep", serde_json::json!({"pattern": "search"}));
            state.update_tool_output("grep", "match 1\nmatch 2");
            state.complete_tool("grep", Some("2 matches".into()));

            state.push_tool_running("read", serde_json::json!({"path": "missing.txt"}));
            state.error_tool("read", "file not found");

            let terminal = render_widget(&state, 0);
            assert_snapshot!("session_history_multiple_tools", terminal.backend());
        }

        #[test]
        fn long_message_wrapping() {
            let mut state = ConversationState::new();
            state.push_assistant_message(
                "This is a very long message that should wrap across multiple lines. \
                The widget needs to handle word wrapping correctly to ensure that \
                content is displayed properly within the viewport width constraints.",
            );

            let terminal = render_widget(&state, 0);
            assert_snapshot!("session_history_long_wrap", terminal.backend());
        }

        #[test]
        fn scroll_offset_at_bottom() {
            let mut state = ConversationState::new();
            for i in 0..10 {
                state.push_user_message(format!("Message {}", i));
            }

            let terminal = render_widget(&state, 0);
            assert_snapshot!("session_history_scroll_bottom", terminal.backend());
        }

        #[test]
        fn scroll_offset_middle() {
            let mut state = ConversationState::new();
            for i in 0..15 {
                state.push_user_message(format!("Message {}", i));
            }

            let terminal = render_widget(&state, 10);
            assert_snapshot!("session_history_scroll_middle", terminal.backend());
        }

        #[test]
        fn streaming_partial_prose() {
            let mut state = ConversationState::new();
            state.push_user_message("Explain");
            state.start_assistant_streaming();
            state.append_streaming_blocks(vec![StreamBlock::prose_partial(
                "I'm thinking about your questio",
            )]);

            let terminal = render_widget(&state, 0);
            assert_snapshot!("session_history_streaming_partial", terminal.backend());
        }

        #[test]
        fn streaming_with_code() {
            let mut state = ConversationState::new();
            state.start_assistant_streaming();
            state.append_streaming_blocks(vec![
                StreamBlock::prose("Here's code:"),
                StreamBlock::code_partial(Some("rust".into()), "fn main() {\n    // incomplete"),
            ]);

            let terminal = render_widget(&state, 0);
            assert_snapshot!("session_history_streaming_code", terminal.backend());
        }
    }
}
