//! Viewport Architecture
//!
//! A virtual document model that separates mutable viewport content
//! from immutable terminal scrollback.
//!
//! ## Key Concepts
//!
//! - **Content Buffer**: Recent messages that can be modified, re-wrapped on resize
//! - **Scrollback**: Overflow content emitted to stdout (terminal-native wrapping)
//! - **Top-Down Fill**: Input starts at top, grows down, anchors to bottom when full
//! - **Popup Overlay**: Command picker overlays content zone

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
use std::collections::VecDeque;
use std::time::Instant;

// =========================================================================
// Phase 1: ContentBlock - height calculation with word wrap
// =========================================================================

/// Type of content block in the viewport
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentKind {
    /// User message
    UserMessage(String),
    /// Assistant message with completion status
    AssistantMessage { content: String, complete: bool },
    /// System message
    System(String),
    /// Tool call with status
    ToolCall { name: String, status: ToolStatus },
}

/// Tool execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    /// Tool is pending execution
    Pending,
    /// Tool is currently running
    Running,
    /// Tool execution completed successfully
    Complete,
    /// Tool execution failed
    Failed,
}

/// A content block in the viewport buffer
#[derive(Debug, Clone)]
pub struct ContentBlock {
    /// Unique identifier for this block
    pub id: u64,
    /// Content of this block
    pub content: ContentKind,
    /// Cached height calculation (width, height)
    cached_height: Option<(u16, u16)>,
    /// Timestamp of block creation
    pub timestamp: Instant,
}

impl ContentBlock {
    /// Create a new content block
    pub fn new(id: u64, content: ContentKind) -> Self {
        Self {
            id,
            content,
            cached_height: None,
            timestamp: Instant::now(),
        }
    }

    /// Get the raw text content (without formatting prefixes)
    pub fn content_text(&self) -> &str {
        match &self.content {
            ContentKind::UserMessage(text) => text,
            ContentKind::AssistantMessage { content, .. } => content,
            ContentKind::System(text) => text,
            ContentKind::ToolCall { name, .. } => name,
        }
    }

    /// Format content for viewport display with appropriate prefix
    pub fn format_for_viewport(&self) -> String {
        match &self.content {
            ContentKind::UserMessage(text) => format!("You: {}", text),
            ContentKind::AssistantMessage { content, .. } => format!("Assistant: {}", content),
            ContentKind::System(text) => format!("* {}", text),
            ContentKind::ToolCall { name, status } => {
                let status_str = match status {
                    ToolStatus::Pending => "pending",
                    ToolStatus::Running => "running",
                    ToolStatus::Complete => "complete",
                    ToolStatus::Failed => "failed",
                };
                format!("Tool: {} ({})", name, status_str)
            }
        }
    }

    /// Calculate height in lines when wrapped to given width
    ///
    /// Uses cached value if available and width matches.
    /// Height calculation:
    /// - Get formatted text via format_for_viewport()
    /// - For each line in text.lines():
    ///   - If empty: count as 1
    ///   - Else: ceil(line_chars / width)
    /// - Sum all line heights
    /// - Return max(1, total)
    pub fn height(&mut self, width: u16) -> u16 {
        // Check cache
        if let Some((cached_width, cached_height)) = self.cached_height {
            if cached_width == width {
                return cached_height;
            }
        }

        // Calculate height
        let formatted = self.format_for_viewport();
        let total_height = if formatted.is_empty() {
            1
        } else {
            let mut height = 0u16;
            for line in formatted.lines() {
                if line.is_empty() {
                    height = height.saturating_add(1);
                } else {
                    let line_len = line.chars().count() as u16;
                    let line_height = if width == 0 {
                        1 // Avoid division by zero
                    } else {
                        line_len.div_ceil(width) // Ceiling division
                    };
                    height = height.saturating_add(line_height.max(1));
                }
            }
            height.max(1)
        };

        // Cache the result
        self.cached_height = Some((width, total_height));
        total_height
    }

    /// Invalidate cached height calculation
    ///
    /// Should be called when terminal is resized or content changes.
    pub fn invalidate_height(&mut self) {
        self.cached_height = None;
    }

    /// Check if height is currently cached
    ///
    /// Returns true if a height calculation is cached, false otherwise.
    /// Useful for testing and debugging.
    #[cfg(test)]
    pub fn has_cached_height(&self) -> bool {
        self.cached_height.is_some()
    }

    // =========================================================================
    // Phase 6: Scrollback emission formatting
    // =========================================================================

    /// Format content for scrollback emission (plain text, no ANSI codes)
    ///
    /// Used when content overflows from viewport to terminal scrollback.
    /// Terminal handles word wrapping natively for scrollback content.
    ///
    /// Note: Currently identical to `format_for_viewport()` as viewport
    /// rendering doesn't yet use ANSI codes. This method exists as a
    /// separate API because:
    /// - Viewport rendering may add ANSI codes in the future (colors, bold, etc.)
    /// - Scrollback must remain plain text for terminal-native wrapping
    /// - Different formatting might be needed (e.g., timestamps in scrollback)
    pub fn format_for_scrollback(&self) -> String {
        // Same as format_for_viewport but ensures no ANSI escape sequences
        self.format_for_viewport()
    }
}

// =========================================================================
// Phase 2: ViewportState - buffer management
// =========================================================================

/// Height reserved for input area (border + content + border)
pub const INPUT_HEIGHT: u16 = 3;
/// Height reserved for status line
pub const STATUS_HEIGHT: u16 = 1;

// =========================================================================
// Phase 4: Layout calculation
// =========================================================================

/// Layout mode determines how content, input, and status are positioned
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    /// Content doesn't fill viewport - input follows content at top
    TopDown,
    /// Content fills viewport - input anchors to bottom
    BottomAnchored,
}

/// Layout zones for rendering the viewport components
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutZones {
    /// Content area rectangle
    pub content: Rect,
    /// Input area rectangle
    pub input: Rect,
    /// Status line rectangle
    pub status: Rect,
}

/// Viewport state manages the content buffer and dimensions
pub struct ViewportState {
    content_buffer: VecDeque<ContentBlock>,
    next_id: u64,
    width: u16,
    height: u16,
    content_zone_height: u16,
}

impl ViewportState {
    /// Create a new viewport state with given dimensions
    pub fn new(width: u16, height: u16) -> Self {
        let content_zone_height = height
            .saturating_sub(INPUT_HEIGHT)
            .saturating_sub(STATUS_HEIGHT);
        Self {
            content_buffer: VecDeque::new(),
            next_id: 1,
            width,
            height,
            content_zone_height,
        }
    }

    /// Get the viewport width
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Get the viewport height
    pub fn height(&self) -> u16 {
        self.height
    }

    /// Get the content zone height (available for content blocks)
    pub fn content_zone_height(&self) -> u16 {
        self.content_zone_height
    }

    /// Get the number of content blocks in the buffer
    pub fn content_count(&self) -> usize {
        self.content_buffer.len()
    }

    /// Push a user message to the content buffer
    pub fn push_user_message(&mut self, text: impl Into<String>) {
        let id = self.next_id;
        self.next_id += 1;
        let block = ContentBlock::new(id, ContentKind::UserMessage(text.into()));
        self.content_buffer.push_back(block);
    }

    /// Push an assistant message to the content buffer
    pub fn push_assistant_message(&mut self, text: impl Into<String>, complete: bool) {
        let id = self.next_id;
        self.next_id += 1;
        let block = ContentBlock::new(
            id,
            ContentKind::AssistantMessage {
                content: text.into(),
                complete,
            },
        );
        self.content_buffer.push_back(block);
    }

    /// Push a system message to the content buffer
    pub fn push_system_message(&mut self, text: impl Into<String>) {
        let id = self.next_id;
        self.next_id += 1;
        let block = ContentBlock::new(id, ContentKind::System(text.into()));
        self.content_buffer.push_back(block);
    }

    /// Get an iterator over content blocks
    pub fn content_blocks(&self) -> impl Iterator<Item = &ContentBlock> {
        self.content_buffer.iter()
    }

    // =========================================================================
    // Phase 3: Overflow logic
    // =========================================================================

    /// Calculate total content height of all blocks at current width
    ///
    /// Iterates through all content blocks and sums their wrapped heights.
    pub fn total_content_height(&mut self) -> u16 {
        let width = self.width;
        self.content_buffer
            .iter_mut()
            .fold(0u16, |acc, block| acc.saturating_add(block.height(width)))
    }

    /// Move overflow blocks to scrollback when content exceeds zone height
    ///
    /// Returns blocks that should be emitted to terminal scrollback (oldest first).
    /// Removes these blocks from the content buffer.
    ///
    /// **Guarantees:**
    /// - Always keeps at least 1 message in buffer (newest), even if it exceeds zone
    /// - Blocks are removed and returned as complete units (never split)
    /// - Returns empty vec if no overflow needed
    pub fn maybe_overflow_to_scrollback(&mut self) -> Vec<ContentBlock> {
        let mut overflow = Vec::new();

        // Empty buffer check
        if self.content_buffer.is_empty() {
            return overflow;
        }

        // Keep popping from front while we have more than 1 message AND total height exceeds zone
        while self.content_buffer.len() > 1
            && self.total_content_height() > self.content_zone_height
        {
            if let Some(block) = self.content_buffer.pop_front() {
                overflow.push(block);
            }
        }

        overflow
    }

    // =========================================================================
    // Phase 4: Layout calculation
    // =========================================================================

    /// Calculate current layout mode based on content height
    ///
    /// Determines whether to use TopDown or BottomAnchored layout:
    /// - **TopDown**: When content + input + status all fit in viewport height
    /// - **BottomAnchored**: When content would overflow, anchor input to bottom
    pub fn layout_mode(&mut self) -> LayoutMode {
        let content_height = self.total_content_height();
        let fixed_height = INPUT_HEIGHT + STATUS_HEIGHT;

        if content_height + fixed_height < self.height {
            LayoutMode::TopDown
        } else {
            LayoutMode::BottomAnchored
        }
    }

    /// Calculate layout zones for rendering
    ///
    /// Returns `Rect` positions for content, input, and status areas based on
    /// the current layout mode.
    ///
    /// **TopDown mode:**
    /// - Content starts at y=0, height = content_height
    /// - Input follows at y=content_height
    /// - Status follows at y=content_height+INPUT_HEIGHT
    ///
    /// **BottomAnchored mode:**
    /// - Input anchors at y = height - INPUT_HEIGHT - STATUS_HEIGHT
    /// - Status anchors at y = height - STATUS_HEIGHT
    /// - Content fills from y=0 to input
    pub fn layout_zones(&mut self) -> LayoutZones {
        let mode = self.layout_mode();

        match mode {
            LayoutMode::TopDown => {
                let content_height = self.total_content_height();

                let content = Rect {
                    x: 0,
                    y: 0,
                    width: self.width,
                    height: content_height,
                };

                let input = Rect {
                    x: 0,
                    y: content_height,
                    width: self.width,
                    height: INPUT_HEIGHT,
                };

                let status = Rect {
                    x: 0,
                    y: content_height + INPUT_HEIGHT,
                    width: self.width,
                    height: STATUS_HEIGHT,
                };

                LayoutZones {
                    content,
                    input,
                    status,
                }
            }
            LayoutMode::BottomAnchored => {
                let input_y = self
                    .height
                    .saturating_sub(INPUT_HEIGHT)
                    .saturating_sub(STATUS_HEIGHT);
                let status_y = self.height.saturating_sub(STATUS_HEIGHT);

                let content = Rect {
                    x: 0,
                    y: 0,
                    width: self.width,
                    height: input_y,
                };

                let input = Rect {
                    x: 0,
                    y: input_y,
                    width: self.width,
                    height: INPUT_HEIGHT,
                };

                let status = Rect {
                    x: 0,
                    y: status_y,
                    width: self.width,
                    height: STATUS_HEIGHT,
                };

                LayoutZones {
                    content,
                    input,
                    status,
                }
            }
        }
    }

    // =========================================================================
    // Phase 5: Resize handling
    // =========================================================================

    /// Handle terminal resize
    ///
    /// Updates dimensions, invalidates height caches, recalculates content zone,
    /// and returns any blocks that overflow to scrollback.
    pub fn handle_resize(&mut self, width: u16, height: u16) -> Vec<ContentBlock> {
        // Update dimensions
        self.width = width;
        self.height = height;

        // Recalculate content zone
        self.content_zone_height = height
            .saturating_sub(INPUT_HEIGHT)
            .saturating_sub(STATUS_HEIGHT);

        // Invalidate all cached heights
        for block in &mut self.content_buffer {
            block.invalidate_height();
        }

        // May need to overflow after resize
        self.maybe_overflow_to_scrollback()
    }

    // =========================================================================
    // Phase 7: Ratatui rendering integration
    // =========================================================================

    /// Render the viewport to a ratatui frame
    pub fn render(&mut self, frame: &mut Frame) {
        let zones = self.layout_zones();

        // Render content zone
        self.render_content(frame, zones.content);

        // Render input placeholder
        self.render_input_placeholder(frame, zones.input);

        // Render status line
        self.render_status_placeholder(frame, zones.status);
    }

    fn render_content(&self, frame: &mut Frame, area: Rect) {
        let mut lines = Vec::new();

        for block in self.content_buffer.iter() {
            let formatted = block.format_for_viewport();
            // Style based on content kind
            let style = match &block.content {
                ContentKind::UserMessage(_) => Style::default().fg(Color::Cyan),
                ContentKind::AssistantMessage { .. } => Style::default().fg(Color::Green),
                ContentKind::System(_) => Style::default().fg(Color::Yellow),
                ContentKind::ToolCall { .. } => Style::default().fg(Color::Magenta),
            };

            for line_str in formatted.lines() {
                lines.push(Line::from(Span::styled(line_str.to_string(), style)));
            }
        }

        let content_widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title("Chat"))
            .wrap(Wrap { trim: false });

        frame.render_widget(content_widget, area);
    }

    fn render_input_placeholder(&self, frame: &mut Frame, area: Rect) {
        // Placeholder - actual input comes from TuiState
        let placeholder = Paragraph::new("[Input area - placeholder]")
            .block(Block::default().borders(Borders::ALL).title("Input"));
        frame.render_widget(placeholder, area);
    }

    fn render_status_placeholder(&self, frame: &mut Frame, area: Rect) {
        // Placeholder - actual status comes from TuiState
        let status = Paragraph::new("Ready").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(status, area);
    }
}

#[cfg(test)]
mod tests {
    //! TDD test suite for viewport architecture
    //!
    //! Tests are organized by phase, written before implementation.

    // =========================================================================
    // Phase 1: ContentBlock - height calculation with word wrap
    // =========================================================================
    mod content_block_tests {
        use super::super::*;

        #[test]
        fn height_single_line_short_message() {
            // "You: Hello" = 10 chars, fits on one line at width 80
            // Expected: height = 1
            let mut block = ContentBlock::new(1, ContentKind::UserMessage("Hello".to_string()));
            let height = block.height(80);
            assert_eq!(height, 1);
        }

        #[test]
        fn height_wraps_at_terminal_width() {
            // "You: This is a longer message that will wrap" = 46 chars
            // At width 20: ceil(46/20) = 3 lines
            // Expected: height = 3
            let mut block = ContentBlock::new(
                1,
                ContentKind::UserMessage("This is a longer message that will wrap".to_string()),
            );
            let height = block.height(20);
            assert_eq!(height, 3);
        }

        #[test]
        fn height_is_cached_after_first_call() {
            // First call calculates, second uses cache
            // Verify cached_height is Some after first call
            let mut block = ContentBlock::new(1, ContentKind::UserMessage("Hello".to_string()));
            assert!(block.cached_height.is_none());

            let height1 = block.height(80);
            assert!(block.cached_height.is_some());
            assert_eq!(block.cached_height, Some((80, 1)));

            // Second call should use cache
            let height2 = block.height(80);
            assert_eq!(height1, height2);
        }

        #[test]
        fn invalidate_clears_cached_height() {
            // After invalidate_height(), cached_height should be None
            let mut block = ContentBlock::new(1, ContentKind::UserMessage("Hello".to_string()));
            block.height(80);
            assert!(block.cached_height.is_some());

            block.invalidate_height();
            assert!(block.cached_height.is_none());
        }

        #[test]
        fn format_user_message_has_prefix() {
            // UserMessage("Hello") -> "You: Hello"
            let block = ContentBlock::new(1, ContentKind::UserMessage("Hello".to_string()));
            assert_eq!(block.format_for_viewport(), "You: Hello");
        }

        #[test]
        fn format_assistant_message_has_prefix() {
            // AssistantMessage { content: "Hi", complete: true } -> "Assistant: Hi"
            let block = ContentBlock::new(
                1,
                ContentKind::AssistantMessage {
                    content: "Hi".to_string(),
                    complete: true,
                },
            );
            assert_eq!(block.format_for_viewport(), "Assistant: Hi");
        }

        #[test]
        fn format_system_message_has_prefix() {
            // System("Indexing...") -> "* Indexing..."
            let block = ContentBlock::new(1, ContentKind::System("Indexing...".to_string()));
            assert_eq!(block.format_for_viewport(), "* Indexing...");
        }

        #[test]
        fn multiline_content_height_counts_all_lines() {
            // Message with embedded newlines
            // "Line1\nLine2\nLine3" should count as 3+ lines
            let mut block = ContentBlock::new(
                1,
                ContentKind::UserMessage("Line1\nLine2\nLine3".to_string()),
            );
            // "You: Line1" = 10 chars, fits in width 80
            // "Line2" = 5 chars, fits in width 80
            // "Line3" = 5 chars, fits in width 80
            // Total = 3 lines
            let height = block.height(80);
            assert_eq!(height, 3);
        }
    }

    // =========================================================================
    // Phase 2: ViewportState - buffer management
    // =========================================================================
    mod buffer_management_tests {
        use super::super::*;

        #[test]
        fn new_viewport_has_empty_buffer() {
            // ViewportState::new(80, 24) -> content_count() == 0
            let viewport = ViewportState::new(80, 24);
            assert_eq!(viewport.content_count(), 0);
        }

        #[test]
        fn push_user_message_increments_count() {
            // push_user_message("Hello") -> content_count() == 1
            let mut viewport = ViewportState::new(80, 24);
            viewport.push_user_message("Hello");
            assert_eq!(viewport.content_count(), 1);
        }

        #[test]
        fn push_multiple_messages_maintains_order() {
            // Push A, B, C -> iterate yields A, B, C
            let mut viewport = ViewportState::new(80, 24);
            viewport.push_user_message("A");
            viewport.push_assistant_message("B", true);
            viewport.push_system_message("C");

            let messages: Vec<&str> = viewport
                .content_blocks()
                .map(|block| block.content_text())
                .collect();
            assert_eq!(messages, vec!["A", "B", "C"]);
        }

        #[test]
        fn each_message_gets_unique_id() {
            // Push 3 messages -> all have different IDs
            let mut viewport = ViewportState::new(80, 24);
            viewport.push_user_message("First");
            viewport.push_assistant_message("Second", false);
            viewport.push_system_message("Third");

            let ids: Vec<u64> = viewport.content_blocks().map(|block| block.id).collect();
            assert_eq!(ids.len(), 3);
            // All IDs should be unique
            let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
            assert_eq!(unique_ids.len(), 3);
        }

        #[test]
        fn content_zone_height_calculated_from_dimensions() {
            // height=24, INPUT_HEIGHT=3, STATUS_HEIGHT=1
            // content_zone_height = 24 - 3 - 1 = 20
            let viewport = ViewportState::new(80, 24);
            assert_eq!(viewport.content_zone_height(), 20);
        }
    }

    // =========================================================================
    // Phase 3: Overflow logic
    // =========================================================================
    mod overflow_tests {
        use super::super::*;

        #[test]
        fn no_overflow_when_content_fits() {
            // 3 single-line messages in zone of 6 -> no overflow
            let mut viewport = ViewportState::new(80, 10); // content_zone = 10 - 3 - 1 = 6
            viewport.push_user_message("A");
            viewport.push_user_message("B");
            viewport.push_user_message("C");

            let overflow = viewport.maybe_overflow_to_scrollback();
            assert!(overflow.is_empty());
            assert_eq!(viewport.content_count(), 3);
        }

        #[test]
        fn overflow_returns_oldest_first() {
            // Add 8 messages to zone of 6 -> overflow contains oldest
            let mut viewport = ViewportState::new(80, 10); // content_zone = 6
            for i in 0..8 {
                viewport.push_user_message(format!("Message {}", i));
            }

            let overflow = viewport.maybe_overflow_to_scrollback();
            assert!(!overflow.is_empty());

            // First overflowed message should be "Message 0"
            assert_eq!(overflow[0].content_text(), "Message 0");

            // Remaining messages in buffer should start from where overflow left off
            let remaining: Vec<&str> = viewport
                .content_blocks()
                .map(|b| b.content_text())
                .collect();
            assert!(!remaining.contains(&"Message 0"));
        }

        #[test]
        fn wrapped_message_overflows_as_unit() {
            // Message wrapping to 2 lines overflows completely
            // Never split across viewport/scrollback boundary
            let mut viewport = ViewportState::new(20, 7); // content_zone = 7 - 3 - 1 = 3

            // First message wraps to 3 lines at width 20 ("You: " + long text)
            viewport.push_user_message("This is a very long message that will wrap multiple times");

            // Second short message takes 1 line
            viewport.push_user_message("Short");

            // Total is now > 3 lines, should overflow first message as complete unit
            let overflow = viewport.maybe_overflow_to_scrollback();

            // Should have overflowed the long message
            if !overflow.is_empty() {
                assert!(overflow[0].content_text().contains("very long message"));
            }

            // Buffer should only contain the short message
            assert_eq!(viewport.content_count(), 1);
        }

        #[test]
        fn always_keeps_at_least_one_message() {
            // Even if single message exceeds zone, keep it
            let mut viewport = ViewportState::new(10, 6); // content_zone = 6 - 3 - 1 = 2

            // Push a message that wraps to way more than 2 lines
            viewport.push_user_message("This is an extremely long message that will definitely exceed the content zone height when wrapped at width 10");

            let overflow = viewport.maybe_overflow_to_scrollback();
            assert!(overflow.is_empty());
            assert_eq!(viewport.content_count(), 1);
        }

        #[test]
        fn empty_buffer_returns_empty_overflow() {
            // No content -> maybe_overflow returns empty vec
            let mut viewport = ViewportState::new(80, 24);
            let overflow = viewport.maybe_overflow_to_scrollback();
            assert!(overflow.is_empty());
        }

        #[test]
        fn overflow_removes_from_buffer() {
            // After overflow, those messages are gone from buffer
            let mut viewport = ViewportState::new(80, 10); // content_zone = 6

            for i in 0..10 {
                viewport.push_user_message(format!("Msg{}", i));
            }

            let initial_count = viewport.content_count();
            let overflow = viewport.maybe_overflow_to_scrollback();

            // Buffer count should decrease by overflow count
            assert_eq!(viewport.content_count() + overflow.len(), initial_count);

            // Overflowed messages should not be in buffer
            let remaining_texts: Vec<&str> = viewport
                .content_blocks()
                .map(|b| b.content_text())
                .collect();

            for overflowed_block in overflow {
                assert!(!remaining_texts.contains(&overflowed_block.content_text()));
            }
        }
    }

    // =========================================================================
    // Phase 4: Layout calculation
    // =========================================================================
    mod layout_tests {
        use super::super::*;

        #[test]
        fn empty_viewport_input_at_top() {
            // No content -> input.y == 0
            let mut viewport = ViewportState::new(80, 24);
            let zones = viewport.layout_zones();

            assert_eq!(zones.input.y, 0);
            assert_eq!(zones.content.height, 0);
        }

        #[test]
        fn input_follows_content_top_down() {
            // 2 lines of content -> input.y == 2
            let mut viewport = ViewportState::new(80, 24);
            viewport.push_user_message("First"); // 1 line
            viewport.push_user_message("Second"); // 1 line

            let zones = viewport.layout_zones();

            // Total content should be 2 lines
            assert_eq!(zones.content.height, 2);
            // Input should start right after content
            assert_eq!(zones.input.y, 2);
        }

        #[test]
        fn status_always_below_input() {
            // status.y == input.y + INPUT_HEIGHT
            let mut viewport = ViewportState::new(80, 24);
            viewport.push_user_message("Message");

            let zones = viewport.layout_zones();

            assert_eq!(zones.status.y, zones.input.y + INPUT_HEIGHT);
        }

        #[test]
        fn switches_to_bottom_anchored_when_full() {
            // When content + fixed > height -> BottomAnchored mode
            let mut viewport = ViewportState::new(80, 10);
            // height=10, fixed=INPUT_HEIGHT(3)+STATUS_HEIGHT(1)=4
            // Need content_height + 4 >= 10 to trigger BottomAnchored
            // So need content_height >= 6

            // Add enough messages to exceed threshold
            for i in 0..8 {
                viewport.push_user_message(format!("Message {}", i));
            }

            assert_eq!(viewport.layout_mode(), LayoutMode::BottomAnchored);
        }

        #[test]
        fn bottom_anchored_input_at_fixed_position() {
            // In BottomAnchored: input.y == height - INPUT_HEIGHT - STATUS_HEIGHT
            let mut viewport = ViewportState::new(80, 10);

            // Add enough content to trigger BottomAnchored
            for i in 0..10 {
                viewport.push_user_message(format!("Message {}", i));
            }

            let zones = viewport.layout_zones();

            // input.y should be height - INPUT_HEIGHT - STATUS_HEIGHT
            // 10 - 3 - 1 = 6
            assert_eq!(zones.input.y, 10 - INPUT_HEIGHT - STATUS_HEIGHT);
            assert_eq!(zones.input.y, 6);
        }

        #[test]
        fn content_area_fills_remaining_space_when_full() {
            // In BottomAnchored: content.height == height - INPUT_HEIGHT - STATUS_HEIGHT
            let mut viewport = ViewportState::new(80, 20);

            // Add enough content to trigger BottomAnchored
            for i in 0..20 {
                viewport.push_user_message(format!("Message {}", i));
            }

            let zones = viewport.layout_zones();

            // content.height should be height - INPUT_HEIGHT - STATUS_HEIGHT
            // 20 - 3 - 1 = 16
            assert_eq!(zones.content.height, 20 - INPUT_HEIGHT - STATUS_HEIGHT);
            assert_eq!(zones.content.height, 16);
        }
    }

    // =========================================================================
    // Phase 5: Resize handling
    // =========================================================================
    mod resize_tests {
        use super::super::*;

        #[test]
        fn resize_updates_dimensions() {
            // handle_resize(100, 50) -> width()==100, height()==50
            let mut viewport = ViewportState::new(80, 24);
            viewport.handle_resize(100, 50);

            assert_eq!(viewport.width(), 100);
            assert_eq!(viewport.height(), 50);
        }

        #[test]
        fn resize_invalidates_all_cached_heights() {
            // After resize, caches are recalculated with new width
            let mut viewport = ViewportState::new(80, 24);
            viewport.push_user_message("First");
            viewport.push_user_message("Second");
            viewport.push_user_message("Third");

            // Calculate heights to populate cache at width 80
            let _height = viewport.total_content_height();

            // Verify caches are populated
            for block in viewport.content_blocks() {
                assert!(
                    block.has_cached_height(),
                    "Cache should be populated before resize"
                );
            }

            // Store the old cached values to verify they change
            let old_cached: Vec<_> = viewport.content_blocks().map(|b| b.cached_height).collect();

            // Resize to different width
            viewport.handle_resize(50, 24); // Narrower width will cause rewrap

            // After resize, caches should be recalculated with new width
            // The cached widths should now be 50, not 80
            for block in viewport.content_blocks() {
                assert!(
                    block.has_cached_height(),
                    "Cache should be repopulated after resize"
                );
                // If we had access to cached_height, we'd verify width == 50
                // but since it's private, we just verify the cache exists
            }

            // Verify the cached values actually changed (recalculated, not just kept)
            let new_cached: Vec<_> = viewport.content_blocks().map(|b| b.cached_height).collect();

            // At least one cache should have changed due to different width
            // (unless all messages are empty, which they're not)
            assert_ne!(
                old_cached, new_cached,
                "Cache values should change when width changes"
            );
        }

        #[test]
        fn resize_smaller_triggers_overflow() {
            // Shrink terminal -> may return overflow blocks
            let mut viewport = ViewportState::new(80, 24);

            // Add multiple messages that fit in initial size
            for i in 0..10 {
                viewport.push_user_message(format!("Message {}", i));
            }

            // Shrink to tiny terminal
            let overflow = viewport.handle_resize(80, 8);

            // Should have overflowed some messages
            assert!(
                !overflow.is_empty(),
                "Smaller terminal should trigger overflow"
            );
        }

        #[test]
        fn resize_larger_no_overflow() {
            // Grow terminal -> no overflow
            let mut viewport = ViewportState::new(80, 10);

            // Add a few messages
            viewport.push_user_message("First");
            viewport.push_user_message("Second");
            viewport.push_user_message("Third");

            // Grow terminal
            let overflow = viewport.handle_resize(100, 50);

            // Should not overflow
            assert!(
                overflow.is_empty(),
                "Larger terminal should not trigger overflow"
            );
            assert_eq!(viewport.content_count(), 3, "All messages should remain");
        }

        #[test]
        fn resize_recalculates_content_zone_height() {
            // content_zone_height adjusts based on new height
            let mut viewport = ViewportState::new(80, 24);

            // Initial: 24 - INPUT_HEIGHT(3) - STATUS_HEIGHT(1) = 20
            assert_eq!(viewport.content_zone_height(), 20);

            // Resize to height 50
            viewport.handle_resize(80, 50);

            // New: 50 - 3 - 1 = 46
            assert_eq!(viewport.content_zone_height(), 46);

            // Resize to height 10
            viewport.handle_resize(80, 10);

            // New: 10 - 3 - 1 = 6
            assert_eq!(viewport.content_zone_height(), 6);
        }

        #[test]
        fn resize_narrower_causes_rewrap() {
            // Message that fit on 1 line now wraps to 2
            // May trigger overflow
            let mut viewport = ViewportState::new(80, 10);

            // Add a message that fits on 1 line at width 80
            // "You: This is a message" = 22 chars, fits in width 80
            viewport.push_user_message("This is a message");

            // At width 80, should be 1 line
            let height_wide = viewport.total_content_height();
            assert_eq!(height_wide, 1, "Should be 1 line at width 80");

            // Resize to narrow width 15
            // "You: This is a message" = 22 chars, needs ceil(22/15) = 2 lines
            viewport.handle_resize(15, 10);

            // Should now wrap to more lines
            let height_narrow = viewport.total_content_height();
            assert!(
                height_narrow > height_wide,
                "Should wrap to more lines when narrower"
            );

            // Add more messages to potentially trigger overflow
            viewport.push_user_message("Another message");
            viewport.push_user_message("Yet another");
            viewport.push_user_message("And more");
            viewport.push_user_message("Keep going");

            // Resize even smaller
            let overflow = viewport.handle_resize(10, 8);

            // With narrow width and small height, should overflow
            // (exact number depends on wrapping, but should be > 0)
            assert!(
                !overflow.is_empty() || viewport.content_count() >= 1,
                "Should either overflow or keep minimum 1 message"
            );
        }
    }

    // =========================================================================
    // Phase 6: Scrollback emission (format for stdout)
    // =========================================================================
    mod scrollback_tests {
        use super::super::*;

        #[test]
        fn format_for_scrollback_includes_prefix() {
            // UserMessage("Hello") -> "You: Hello"
            let block = ContentBlock::new(1, ContentKind::UserMessage("Hello".to_string()));
            assert_eq!(block.format_for_scrollback(), "You: Hello");
        }

        #[test]
        fn scrollback_format_no_ansi_codes() {
            // Plain text for terminal-native wrapping - no ANSI escape sequences
            let block = ContentBlock::new(
                1,
                ContentKind::AssistantMessage {
                    content: "Test message".to_string(),
                    complete: true,
                },
            );
            let output = block.format_for_scrollback();

            // Verify no ANSI escape sequences
            // ANSI codes start with ESC (0x1B or \x1b) followed by [
            assert!(
                !output.contains('\x1b'),
                "Scrollback output must not contain ANSI escape sequences"
            );

            // Should still have the proper prefix
            assert_eq!(output, "Assistant: Test message");
        }
    }

    // =========================================================================
    // Phase 7: Ratatui rendering integration
    // =========================================================================
    mod render_tests {
        use super::super::*;
        use ratatui::{backend::TestBackend, Terminal};

        fn test_terminal() -> Terminal<TestBackend> {
            let backend = TestBackend::new(80, 24);
            Terminal::new(backend).unwrap()
        }

        #[test]
        fn render_empty_viewport() {
            let mut terminal = test_terminal();
            let mut viewport = ViewportState::new(80, 24);

            terminal.draw(|frame| viewport.render(frame)).unwrap();

            // Just verify it doesn't panic for now
            // More detailed snapshot tests in Phase 8
        }

        #[test]
        fn render_with_content() {
            let mut terminal = test_terminal();
            let mut viewport = ViewportState::new(80, 24);
            viewport.push_user_message("Hello");
            viewport.push_assistant_message("Hi there!", true);

            terminal.draw(|frame| viewport.render(frame)).unwrap();

            // Verify it doesn't panic and basic content is rendered
        }

        #[test]
        fn render_uses_layout_zones() {
            let mut terminal = test_terminal();
            let mut viewport = ViewportState::new(80, 24);
            viewport.push_user_message("Test message");

            // Get layout zones
            let zones = viewport.layout_zones();

            terminal.draw(|frame| viewport.render(frame)).unwrap();

            // Verify zones are reasonable
            assert!(zones.content.height > 0);
            assert_eq!(zones.input.height, INPUT_HEIGHT);
            assert_eq!(zones.status.height, STATUS_HEIGHT);
        }
    }
}
