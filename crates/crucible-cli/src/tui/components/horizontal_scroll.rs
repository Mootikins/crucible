//! Horizontal scroll container widget
//!
//! A container that allows content wider than the viewport to be scrolled
//! horizontally. Useful for tables, code blocks, and other wide content.
//!
//! # Example
//!
//! ```ignore
//! let content = "| Col1 | Col2 | Col3 | Col4 | Col5 |";
//! let scroll_state = HorizontalScrollState::new(content.len());
//!
//! let widget = HorizontalScrollWidget::new(&content, &scroll_state);
//! frame.render_widget(widget, area);
//! ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::Widget,
};

/// State for horizontal scrolling
#[derive(Debug, Clone, Default)]
pub struct HorizontalScrollState {
    /// Current horizontal scroll offset (in characters)
    pub offset: usize,
    /// Total content width (in characters)
    pub content_width: usize,
}

impl HorizontalScrollState {
    /// Create a new scroll state
    pub fn new(content_width: usize) -> Self {
        Self {
            offset: 0,
            content_width,
        }
    }

    /// Scroll left by the given amount
    pub fn scroll_left(&mut self, amount: usize) {
        use crate::tui::scroll_utils::ScrollUtils;
        self.offset = ScrollUtils::scroll_horizontal(
            self.offset,
            -(amount as isize),
            self.content_width,
            self.content_width, // Will be clamped when called with viewport
        );
    }

    /// Scroll right by the given amount
    pub fn scroll_right(&mut self, amount: usize, viewport_width: usize) {
        use crate::tui::scroll_utils::ScrollUtils;
        self.offset = ScrollUtils::scroll_horizontal(
            self.offset,
            amount as isize,
            self.content_width,
            viewport_width,
        );
    }

    /// Scroll to the beginning
    pub fn scroll_to_start(&mut self) {
        self.offset = 0;
    }

    /// Scroll to the end
    pub fn scroll_to_end(&mut self, viewport_width: usize) {
        self.offset = self.content_width.saturating_sub(viewport_width);
    }

    /// Check if content overflows the viewport
    pub fn has_overflow(&self, viewport_width: usize) -> bool {
        self.content_width > viewport_width
    }

    /// Check if scrolled to the left edge
    pub fn at_start(&self) -> bool {
        self.offset == 0
    }

    /// Check if scrolled to the right edge
    pub fn at_end(&self, viewport_width: usize) -> bool {
        self.offset >= self.content_width.saturating_sub(viewport_width)
    }
}

/// Widget that renders content with horizontal scrolling
///
/// Takes a slice of Lines and renders the visible portion based on scroll state.
/// Shows scroll indicators (◀ ▶) when content overflows.
pub struct HorizontalScrollWidget<'a> {
    /// Lines to render
    lines: &'a [Line<'a>],
    /// Scroll state
    state: &'a HorizontalScrollState,
    /// Whether to show scroll indicators
    show_indicators: bool,
    /// Style for scroll indicators
    indicator_style: Style,
}

impl<'a> HorizontalScrollWidget<'a> {
    /// Create a new horizontal scroll widget
    pub fn new(lines: &'a [Line<'a>], state: &'a HorizontalScrollState) -> Self {
        Self {
            lines,
            state,
            show_indicators: true,
            indicator_style: Style::default().fg(Color::DarkGray),
        }
    }

    /// Set whether to show scroll indicators
    pub fn show_indicators(mut self, show: bool) -> Self {
        self.show_indicators = show;
        self
    }

    /// Set the style for scroll indicators
    pub fn indicator_style(mut self, style: Style) -> Self {
        self.indicator_style = style;
        self
    }
}

impl Widget for HorizontalScrollWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let viewport_width = area.width as usize;
        let has_overflow = self.state.has_overflow(viewport_width);

        // Reserve space for indicators if showing and content overflows
        let (content_area, left_indicator, right_indicator) =
            if self.show_indicators && has_overflow {
                let left = Rect::new(area.x, area.y, 1, area.height);
                let right = Rect::new(area.x + area.width - 1, area.y, 1, area.height);
                let content = Rect::new(
                    area.x + 1,
                    area.y,
                    crate::tui::constants::UiConstants::dialog_width(area.width),
                    area.height,
                );
                (content, Some(left), Some(right))
            } else {
                (area, None, None)
            };

        let content_width = content_area.width as usize;

        // Render each line with horizontal offset
        for (i, line) in self.lines.iter().enumerate() {
            if i >= area.height as usize {
                break;
            }

            let y = content_area.y + i as u16;

            // Convert line to string, apply offset, and render
            let line_str: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            let visible_start = self.state.offset;

            if visible_start < line_str.len() {
                // Get the visible portion of the string
                let visible: String = line_str
                    .chars()
                    .skip(visible_start)
                    .take(content_width)
                    .collect();

                // Render the visible portion
                buf.set_string(content_area.x, y, &visible, Style::default());
            }
        }

        // Render scroll indicators
        if let Some(left_area) = left_indicator {
            let indicator = if self.state.at_start() { " " } else { "◀" };
            for y in left_area.y..left_area.y + left_area.height.min(self.lines.len() as u16) {
                buf.set_string(left_area.x, y, indicator, self.indicator_style);
            }
        }

        if let Some(right_area) = right_indicator {
            let indicator = if self.state.at_end(viewport_width) {
                " "
            } else {
                "▶"
            };
            for y in right_area.y..right_area.y + right_area.height.min(self.lines.len() as u16) {
                buf.set_string(right_area.x, y, indicator, self.indicator_style);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

    #[test]
    fn scroll_state_defaults() {
        let state = HorizontalScrollState::default();
        assert_eq!(state.offset, 0);
        assert_eq!(state.content_width, 0);
    }

    #[test]
    fn scroll_state_new() {
        let state = HorizontalScrollState::new(100);
        assert_eq!(state.offset, 0);
        assert_eq!(state.content_width, 100);
    }

    #[test]
    fn scroll_left_clamps_at_zero() {
        let mut state = HorizontalScrollState::new(100);
        state.offset = 5;
        state.scroll_left(10);
        assert_eq!(state.offset, 0);
    }

    #[test]
    fn scroll_right_clamps_at_max() {
        let mut state = HorizontalScrollState::new(100);
        state.scroll_right(200, 40); // viewport width 40
        assert_eq!(state.offset, 60); // 100 - 40 = 60 max
    }

    #[test]
    fn has_overflow_when_content_wider() {
        let state = HorizontalScrollState::new(100);
        assert!(state.has_overflow(80));
        assert!(!state.has_overflow(100));
        assert!(!state.has_overflow(120));
    }

    #[test]
    fn at_start_and_end() {
        let mut state = HorizontalScrollState::new(100);
        assert!(state.at_start());
        assert!(!state.at_end(40));

        state.scroll_to_end(40);
        assert!(!state.at_start());
        assert!(state.at_end(40));

        state.scroll_to_start();
        assert!(state.at_start());
    }

    #[test]
    fn widget_creation() {
        let lines = vec![Line::from("Hello world")];
        let state = HorizontalScrollState::new(11);
        let widget = HorizontalScrollWidget::new(&lines, &state);
        assert!(widget.show_indicators);
    }

    #[test]
    fn widget_builder_methods() {
        let lines = vec![Line::from("Test")];
        let state = HorizontalScrollState::new(4);
        let widget = HorizontalScrollWidget::new(&lines, &state)
            .show_indicators(false)
            .indicator_style(Style::default().fg(Color::Red));
        assert!(!widget.show_indicators);
    }
}
