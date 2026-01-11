//! Session history widget for displaying conversation messages
//!
//! This widget renders the conversation history with support for scrolling
//! and interactive navigation through messages.

use crate::tui::{
    components::InteractiveWidget,
    conversation::{render_item_to_lines, ConversationItem, ConversationState},
    event_result::{EventResult, TuiAction},
    selection::RenderedLineInfo,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Widget},
};

/// Extract plain text from a ratatui Line (stripping ANSI styles).
fn extract_plain_text(line: &Line) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

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
    viewport_width: u16,
    /// Horizontal scroll offset for wide content (tables, code blocks)
    horizontal_offset: usize,
}

impl<'a> SessionHistoryWidget<'a> {
    /// Create a new session history widget
    pub fn new(state: &'a ConversationState) -> Self {
        Self {
            state,
            scroll_offset: 0,
            viewport_height: 0,
            viewport_width: 80, // Default width for tests
            horizontal_offset: 0,
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

    /// Set the viewport width (for scroll bounds checking)
    ///
    /// Used by `scroll_up()` and `scroll_to_top()` to calculate content height.
    /// Defaults to 80 if not set.
    pub fn viewport_width(mut self, width: u16) -> Self {
        self.viewport_width = width;
        self
    }

    /// Set the horizontal scroll offset (for wide content)
    pub fn horizontal_offset(mut self, offset: usize) -> Self {
        self.horizontal_offset = offset;
        self
    }

    /// Render conversation items to lines with context-aware spacing.
    ///
    /// Uses per-item caching to avoid re-parsing markdown on every frame.
    /// Streaming items are never cached since they change frequently.
    ///
    /// Captures ALL rendered lines (including graduated ones) and stores them
    /// for graduation without re-rendering. Returns only the visible lines
    /// (after skipping already-graduated lines).
    fn render_to_lines(&self, width: usize) -> Vec<Line<'static>> {
        // Check if width changed - invalidates all caches
        self.state.check_width(width);

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

            // Check if this item is streaming (don't cache streaming content)
            let is_streaming = matches!(
                item,
                ConversationItem::AssistantMessage {
                    is_streaming: true,
                    ..
                } | ConversationItem::Status(_)
            );

            // Render the item's lines
            let item_lines = if !is_streaming {
                // Try to get from cache
                if let Some(cached_lines) = self.state.get_cached(i, width) {
                    cached_lines
                } else {
                    // Render fresh and cache
                    let lines = render_item_to_lines(item, width);
                    self.state.store_cached(i, width, lines.clone());
                    lines
                }
            } else {
                // Don't cache streaming content
                render_item_to_lines(item, width)
            };

            all_lines.extend(item_lines);
        }

        // Store ALL lines for graduation (before skipping graduated ones).
        // This ensures graduation uses the exact lines that were rendered.
        self.state.capture_rendered_lines(all_lines.clone());

        // Skip already-graduated lines for display
        let skip_count = self.state.graduated_line_count();
        if skip_count > 0 && skip_count < all_lines.len() {
            all_lines.drain(0..skip_count);
        } else if skip_count >= all_lines.len() {
            all_lines.clear();
        }

        all_lines
    }

    /// Render conversation items to lines and build selection cache info.
    ///
    /// Returns both the display lines and cache data for text extraction.
    /// Uses per-item caching to avoid re-parsing markdown.
    pub fn render_to_lines_with_cache(
        &self,
        width: usize,
    ) -> (Vec<Line<'static>>, Vec<RenderedLineInfo>) {
        // Check if width changed - invalidates all caches
        self.state.check_width(width);

        let mut all_lines = Vec::new();
        let mut cache_info = Vec::new();
        let items = self.state.items();

        for (item_index, item) in items.iter().enumerate() {
            // Add blank line before tool calls, but skip between consecutive tools
            if matches!(item, ConversationItem::ToolCall(_)) {
                let prev_was_tool = item_index > 0
                    && matches!(
                        items.get(item_index - 1),
                        Some(ConversationItem::ToolCall(_))
                    );

                if !prev_was_tool {
                    all_lines.push(Line::from(""));
                    cache_info.push(RenderedLineInfo {
                        text: String::new(),
                        item_index,
                        is_code: false,
                    });
                }
            }

            let is_code = matches!(item, ConversationItem::ToolCall(_));

            // Check if this item is streaming (don't cache streaming content)
            let is_streaming = matches!(
                item,
                ConversationItem::AssistantMessage {
                    is_streaming: true,
                    ..
                } | ConversationItem::Status(_)
            );

            // Try to get from cache (skip for streaming items)
            let item_lines = if !is_streaming {
                if let Some(cached) = self.state.get_cached(item_index, width) {
                    cached
                } else {
                    let lines = render_item_to_lines(item, width);
                    self.state.store_cached(item_index, width, lines.clone());
                    lines
                }
            } else {
                render_item_to_lines(item, width)
            };

            for line in &item_lines {
                // Extract plain text from the Line's spans
                let plain_text = extract_plain_text(line);
                cache_info.push(RenderedLineInfo {
                    text: plain_text,
                    item_index,
                    is_code,
                });
            }

            all_lines.extend(item_lines);
        }

        (all_lines, cache_info)
    }

    /// Calculate total content height
    ///
    /// Uses cached total if available, otherwise computes from per-item heights.
    fn content_height(&self, width: usize) -> usize {
        // Check if width changed
        self.state.check_width(width);

        // Try to use cached total height
        if let Some(total) = self.state.get_total_height() {
            return total;
        }

        // Calculate and cache total height
        let items = self.state.items();
        let total: usize = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                // Try cached height first
                if let Some(height) = self.state.get_cached_height(i) {
                    return height;
                }

                // Check if streaming - don't cache streaming
                let is_streaming = matches!(
                    item,
                    ConversationItem::AssistantMessage {
                        is_streaming: true,
                        ..
                    } | ConversationItem::Status(_)
                );

                let lines = render_item_to_lines(item, width);
                let height = lines.len();
                if !is_streaming {
                    self.state.store_cached(i, width, lines);
                }
                height
            })
            .sum();

        self.state.set_total_height(total);
        total
    }

    /// Get item height, using cache if available
    fn item_height(&self, index: usize, item: &ConversationItem, width: usize) -> usize {
        // Try cached height first
        if let Some(height) = self.state.get_cached_height(index) {
            return height;
        }

        // Check if streaming - don't cache streaming
        let is_streaming = matches!(
            item,
            ConversationItem::AssistantMessage {
                is_streaming: true,
                ..
            } | ConversationItem::Status(_)
        );

        let lines = render_item_to_lines(item, width);
        let height = lines.len();
        if !is_streaming {
            self.state.store_cached(index, width, lines);
        }
        height
    }

    /// Calculate the maximum content width across all lines
    ///
    /// Used for horizontal scroll bounds. Returns the width of the widest line
    /// in the rendered content.
    pub fn max_content_width(&self, render_width: usize) -> usize {
        let lines = self.render_to_lines(render_width);
        lines
            .iter()
            .map(|line| {
                // Calculate actual displayed width of the line
                line.spans
                    .iter()
                    .map(|span| span.content.chars().count())
                    .sum()
            })
            .max()
            .unwrap_or(0)
    }

    /// Scroll up by the given number of lines
    pub fn scroll_up(&mut self, lines: usize) {
        use crate::tui::constants::UiConstants;
        use crate::tui::scroll_utils::ScrollUtils;
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        // Clamp to content bounds if viewport_height is set
        if self.viewport_height > 0 {
            let content_width = UiConstants::content_width(self.viewport_width);
            let content_height = self.content_height(content_width);
            self.scroll_offset = ScrollUtils::clamp_scroll(
                self.scroll_offset,
                content_height,
                self.viewport_height as usize,
            );
        }
    }

    /// Scroll down by the given number of lines
    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Scroll to top of content
    pub fn scroll_to_top(&mut self) {
        use crate::tui::constants::UiConstants;
        use crate::tui::scroll_utils::ScrollUtils;
        if self.viewport_height > 0 {
            let content_width = UiConstants::content_width(self.viewport_width);
            let content_height = self.content_height(content_width);
            self.scroll_offset =
                ScrollUtils::max_scroll(content_height, self.viewport_height as usize);
        }
    }

    /// Scroll to bottom of content
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

impl Widget for SessionHistoryWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        use crate::tui::constants::UiConstants;
        // Content width minus prefix (" ● " = 3 chars) and right margin (1 char)
        let content_width = UiConstants::content_width(area.width);
        // render_to_lines already skips graduated lines from first item
        let lines = self.render_to_lines(content_width);

        let content_height = lines.len();
        let viewport_height = area.height as usize;

        if content_height == 0 {
            return;
        }

        // Horizontal scroll offset (clamped to u16 for Paragraph::scroll)
        let h_scroll = self.horizontal_offset.min(u16::MAX as usize) as u16;

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
            // No Wrap needed - ratatui markdown renderer pre-wraps at word boundaries
            // Apply horizontal scroll (vertical is 0 since content fits)
            let paragraph = Paragraph::new(lines).scroll((0, h_scroll));
            paragraph.render(offset_area, buf);
        } else {
            // Content exceeds viewport - apply scroll
            // scroll_offset = 0: show last viewport_height lines
            // scroll_offset = N: show lines from (content - viewport - N) to (content - N)
            use crate::tui::scroll_utils::ScrollUtils;
            let max_scroll = ScrollUtils::max_scroll(content_height, viewport_height);
            let effective_scroll =
                ScrollUtils::effective_scroll(self.scroll_offset, content_height, viewport_height);

            // Convert bottom-relative to top-relative scroll
            let top_scroll = max_scroll - effective_scroll;

            // No Wrap needed - ratatui markdown renderer pre-wraps at word boundaries
            // Apply both vertical and horizontal scroll
            let paragraph = Paragraph::new(lines).scroll((top_scroll as u16, h_scroll));
            paragraph.render(area, buf);
        }
    }
}

impl InteractiveWidget for SessionHistoryWidget<'_> {
    fn handle_event(&mut self, event: &Event) -> EventResult {
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event
        {
            match (*code, *modifiers) {
                // Ctrl+Up/Down - single line scroll
                (KeyCode::Up, KeyModifiers::CONTROL) => {
                    return EventResult::Action(TuiAction::ScrollLines(1));
                }
                (KeyCode::Down, KeyModifiers::CONTROL) => {
                    return EventResult::Action(TuiAction::ScrollLines(-1));
                }
                // Page Up/Down
                (KeyCode::PageUp, _) => {
                    return EventResult::Action(TuiAction::ScrollPage(
                        crate::tui::event_result::ScrollDirection::Up,
                    ));
                }
                (KeyCode::PageDown, _) => {
                    return EventResult::Action(TuiAction::ScrollPage(
                        crate::tui::event_result::ScrollDirection::Down,
                    ));
                }
                // Home/End - scroll to top/bottom
                (KeyCode::Home, KeyModifiers::NONE) => {
                    return EventResult::Action(TuiAction::ScrollTo(usize::MAX));
                }
                (KeyCode::End, KeyModifiers::NONE) => {
                    return EventResult::Action(TuiAction::ScrollTo(0));
                }
                _ => {}
            }
        }
        EventResult::Ignored
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
    fn test_viewport_width_builder() {
        let state = ConversationState::new();
        let widget = SessionHistoryWidget::new(&state).viewport_width(120);
        assert_eq!(widget.viewport_width, 120);
    }

    #[test]
    fn test_scroll_uses_viewport_width() {
        // Create a conversation with content
        let mut state = ConversationState::new();
        for i in 0..10 {
            state.push_user_message(format!("Message {}", i));
        }

        // With narrow width, same content wraps to more lines
        let mut narrow = SessionHistoryWidget::new(&state)
            .viewport_height(10)
            .viewport_width(40);
        let mut wide = SessionHistoryWidget::new(&state)
            .viewport_height(10)
            .viewport_width(120);

        narrow.scroll_to_top();
        wide.scroll_to_top();

        // Narrow viewport should have higher scroll offset due to wrapping
        assert!(
            narrow.scroll_offset >= wide.scroll_offset,
            "Narrow viewport ({}) should scroll at least as much as wide ({})",
            narrow.scroll_offset,
            wide.scroll_offset
        );
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

        assert_eq!(result, EventResult::Action(TuiAction::ScrollLines(1)));
    }

    #[test]
    fn test_handle_ctrl_down_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::CONTROL));
        let result = widget.handle_event(&event);

        assert_eq!(result, EventResult::Action(TuiAction::ScrollLines(-1)));
    }

    #[test]
    fn test_handle_page_up_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state).viewport_height(24);

        let event = Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        let result = widget.handle_event(&event);

        assert_eq!(
            result,
            EventResult::Action(TuiAction::ScrollPage(
                crate::tui::event_result::ScrollDirection::Up
            ))
        );
    }

    #[test]
    fn test_handle_home_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
        let result = widget.handle_event(&event);

        assert_eq!(result, EventResult::Action(TuiAction::ScrollTo(usize::MAX)));
    }

    #[test]
    fn test_handle_end_event() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
        let result = widget.handle_event(&event);

        assert_eq!(result, EventResult::Action(TuiAction::ScrollTo(0)));
    }

    #[test]
    fn test_handle_unrelated_event_ignored() {
        let state = ConversationState::new();
        let mut widget = SessionHistoryWidget::new(&state);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        let result = widget.handle_event(&event);

        assert_eq!(result, EventResult::Ignored);
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
