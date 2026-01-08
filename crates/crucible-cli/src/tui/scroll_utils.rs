//! Scroll calculation utilities
//!
//! This module provides reusable functions for common scrolling operations
//! such as calculating scroll bounds, clamping scroll offsets, and line counting.

/// Scroll calculation utilities
pub struct ScrollUtils;

impl ScrollUtils {
    /// Calculate the maximum scroll offset for bottom-aligned scrolling
    ///
    /// For bottom-aligned content (like chat), max_scroll represents how far
    /// up you can scroll from the bottom. Returns 0 if content fits in viewport.
    ///
    /// # Arguments
    /// * `content_height` - Total height of the content
    /// * `viewport_height` - Height of the visible area
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::ScrollUtils;
    ///
    /// // Content fits entirely - no scrolling needed
    /// assert_eq!(ScrollUtils::max_scroll(10, 20), 0);
    ///
    /// // Content overflows by 5 lines
    /// assert_eq!(ScrollUtils::max_scroll(25, 20), 5);
    ///
    /// // Large overflow
    /// assert_eq!(ScrollUtils::max_scroll(100, 20), 80);
    /// ```
    #[inline]
    pub const fn max_scroll(content_height: usize, viewport_height: usize) -> usize {
        content_height.saturating_sub(viewport_height)
    }

    /// Clamp scroll offset to valid range [0, max_scroll]
    ///
    /// Ensures scroll_offset doesn't exceed content bounds.
    ///
    /// # Arguments
    /// * `scroll_offset` - Current scroll offset
    /// * `content_height` - Total height of the content
    /// * `viewport_height` - Height of the visible area
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::ScrollUtils;
    ///
    /// // Normal scroll position
    /// assert_eq!(ScrollUtils::clamp_scroll(10, 100, 20), 10);
    ///
    /// // Scroll beyond content - clamp to max
    /// assert_eq!(ScrollUtils::clamp_scroll(90, 100, 20), 80);
    ///
    /// // Negative scroll - clamp to 0
    /// assert_eq!(ScrollUtils::clamp_scroll(0, 100, 20), 0);
    /// ```
    #[inline]
    pub const fn clamp_scroll(
        scroll_offset: usize,
        content_height: usize,
        viewport_height: usize,
    ) -> usize {
        let max = Self::max_scroll(content_height, viewport_height);
        if scroll_offset > max {
            max
        } else {
            scroll_offset
        }
    }

    /// Calculate effective scroll offset (bottom-aligned)
    ///
    /// For bottom-aligned scrolling where 0 = bottom of content.
    /// Returns the scroll offset constrained to valid range.
    ///
    /// # Arguments
    /// * `scroll_offset` - Desired scroll offset from bottom
    /// * `content_height` - Total height of the content
    /// * `viewport_height` - Height of the visible area
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::ScrollUtils;
    ///
    /// // View at bottom (scroll_offset = 0)
    /// assert_eq!(ScrollUtils::effective_scroll(0, 100, 20), 0);
    ///
    /// // Scrolled up 10 lines
    /// assert_eq!(ScrollUtils::effective_scroll(10, 100, 20), 10);
    ///
    /// // Attempt to scroll beyond content
    /// assert_eq!(ScrollUtils::effective_scroll(100, 100, 20), 80);
    /// ```
    #[inline]
    pub const fn effective_scroll(
        scroll_offset: usize,
        content_height: usize,
        viewport_height: usize,
    ) -> usize {
        Self::clamp_scroll(scroll_offset, content_height, viewport_height)
    }

    /// Calculate horizontal scroll offset with clamping
    ///
    /// Used for wide content (tables, code blocks) that need horizontal scrolling.
    ///
    /// # Arguments
    /// * `current_offset` - Current horizontal scroll offset
    /// * `content_width` - Total width of the content
    /// * `viewport_width` - Width of the visible area
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::ScrollUtils;
    ///
    /// // Content fits - no scroll
    /// assert_eq!(ScrollUtils::clamp_horizontal_scroll(0, 80, 100), 0);
    ///
    /// // Scrolled right
    /// assert_eq!(ScrollUtils::clamp_horizontal_scroll(20, 120, 100), 20);
    ///
    /// // Scroll beyond content
    /// assert_eq!(ScrollUtils::clamp_horizontal_scroll(50, 120, 100), 20);
    /// ```
    #[inline]
    pub const fn clamp_horizontal_scroll(
        current_offset: usize,
        content_width: usize,
        viewport_width: usize,
    ) -> usize {
        if content_width <= viewport_width {
            0
        } else {
            let max_offset = content_width - viewport_width;
            if current_offset > max_offset {
                max_offset
            } else {
                current_offset
            }
        }
    }

    /// Calculate new horizontal scroll offset after scrolling by given amount
    ///
    /// # Arguments
    /// * `current_offset` - Current horizontal scroll offset
    /// * `delta` - Amount to scroll (positive = right, negative = left)
    /// * `content_width` - Total width of the content
    /// * `viewport_width` - Width of the visible area
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::ScrollUtils;
    ///
    /// // Scroll right
    /// assert_eq!(ScrollUtils::scroll_horizontal(0, 10, 120, 100), 10);
    ///
    /// // Scroll left from offset
    /// assert_eq!(ScrollUtils::scroll_horizontal(20, -5, 120, 100), 15);
    ///
    /// // Clamp at bounds
    /// assert_eq!(ScrollUtils::scroll_horizontal(0, -10, 120, 100), 0);
    /// assert_eq!(ScrollUtils::scroll_horizontal(0, 30, 120, 100), 20);
    /// ```
    #[inline]
    pub fn scroll_horizontal(
        current_offset: usize,
        delta: isize,
        content_width: usize,
        viewport_width: usize,
    ) -> usize {
        if delta >= 0 {
            let new_offset = current_offset.saturating_add(delta as usize);
            Self::clamp_horizontal_scroll(new_offset, content_width, viewport_width)
        } else {
            let scroll_back = delta.unsigned_abs();
            let new_offset = current_offset.saturating_sub(scroll_back);
            Self::clamp_horizontal_scroll(new_offset, content_width, viewport_width)
        }
    }
}

/// Line counting utilities
pub struct LineCount;

impl LineCount {
    /// Count the number of lines in a string
    ///
    /// Returns at least 1, even for empty strings (consistent with editor behavior).
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::LineCount;
    ///
    /// assert_eq!(LineCount::count(""), 1);
    /// assert_eq!(LineCount::count("single line"), 1);
    /// assert_eq!(LineCount::count("line1\nline2"), 2);
    /// assert_eq!(LineCount::count("line1\nline2\nline3"), 3);
    /// ```
    #[inline]
    pub fn count(text: &str) -> usize {
        text.lines().count().max(1)
    }

    /// Count lines with a minimum value
    ///
    /// Useful for reserving space even for empty content.
    ///
    /// # Arguments
    /// * `text` - The text to count lines in
    /// * `min` - Minimum line count to return
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::LineCount;
    ///
    /// assert_eq!(LineCount::count_min("", 3), 3);
    /// assert_eq!(LineCount::count_min("line1", 3), 3);
    /// assert_eq!(LineCount::count_min("line1\nline2\nline3\nline4", 3), 4);
    /// ```
    #[inline]
    pub fn count_min(text: &str, min: usize) -> usize {
        Self::count(text).max(min)
    }

    /// Count lines with a maximum value
    ///
    /// Useful for limiting display area for large content.
    ///
    /// # Arguments
    /// * `text` - The text to count lines in
    /// * `max` - Maximum line count to return
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::LineCount;
    ///
    /// assert_eq!(LineCount::count_max("", 5), 1);
    /// assert_eq!(LineCount::count_max("line1\nline2", 5), 2);
    /// assert_eq!(LineCount::count_max("line1\nline2\nline3\nline4\nline5\nline6", 5), 5);
    /// ```
    #[inline]
    pub fn count_max(text: &str, max: usize) -> usize {
        Self::count(text).min(max)
    }

    /// Count lines constrained to a range
    ///
    /// # Arguments
    /// * `text` - The text to count lines in
    /// * `min` - Minimum line count to return
    /// * `max` - Maximum line count to return
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::scroll_utils::LineCount;
    ///
    /// assert_eq!(LineCount::count_range("", 3, 10), 3);
    /// assert_eq!(LineCount::count_range("line1", 3, 10), 3);
    /// assert_eq!(LineCount::count_range("line1\nline2\nline3", 3, 10), 3);
    /// assert_eq!(LineCount::count_range("line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12", 3, 10), 10);
    /// ```
    #[inline]
    pub fn count_range(text: &str, min: usize, max: usize) -> usize {
        Self::count(text).clamp(min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ScrollUtils tests
    #[test]
    fn test_max_scroll() {
        // Content fits
        assert_eq!(ScrollUtils::max_scroll(10, 20), 0);
        assert_eq!(ScrollUtils::max_scroll(20, 20), 0);

        // Content overflows
        assert_eq!(ScrollUtils::max_scroll(25, 20), 5);
        assert_eq!(ScrollUtils::max_scroll(100, 20), 80);
        assert_eq!(ScrollUtils::max_scroll(1000, 10), 990);
    }

    #[test]
    fn test_clamp_scroll() {
        // Normal case
        assert_eq!(ScrollUtils::clamp_scroll(10, 100, 20), 10);
        assert_eq!(ScrollUtils::clamp_scroll(50, 100, 20), 50);

        // Clamp to max
        assert_eq!(ScrollUtils::clamp_scroll(90, 100, 20), 80);
        assert_eq!(ScrollUtils::clamp_scroll(100, 100, 20), 80);

        // Content fits (max = 0)
        assert_eq!(ScrollUtils::clamp_scroll(0, 10, 20), 0);
    }

    #[test]
    fn test_effective_scroll() {
        // At bottom
        assert_eq!(ScrollUtils::effective_scroll(0, 100, 20), 0);

        // Scrolled up
        assert_eq!(ScrollUtils::effective_scroll(10, 100, 20), 10);
        assert_eq!(ScrollUtils::effective_scroll(50, 100, 20), 50);

        // Clamped
        assert_eq!(ScrollUtils::effective_scroll(100, 100, 20), 80);

        // Content fits
        assert_eq!(ScrollUtils::effective_scroll(0, 10, 20), 0);
    }

    #[test]
    fn test_clamp_horizontal_scroll() {
        // Content fits - no scroll
        assert_eq!(ScrollUtils::clamp_horizontal_scroll(0, 80, 100), 0);
        assert_eq!(ScrollUtils::clamp_horizontal_scroll(10, 80, 100), 0);

        // Content wider
        assert_eq!(ScrollUtils::clamp_horizontal_scroll(0, 120, 100), 0);
        assert_eq!(ScrollUtils::clamp_horizontal_scroll(10, 120, 100), 10);
        assert_eq!(ScrollUtils::clamp_horizontal_scroll(20, 120, 100), 20);

        // Clamp at max
        assert_eq!(ScrollUtils::clamp_horizontal_scroll(30, 120, 100), 20);
        assert_eq!(ScrollUtils::clamp_horizontal_scroll(50, 120, 100), 20);
    }

    #[test]
    fn test_scroll_horizontal() {
        // Scroll right
        assert_eq!(ScrollUtils::scroll_horizontal(0, 10, 120, 100), 10);
        assert_eq!(ScrollUtils::scroll_horizontal(10, 10, 120, 100), 20);

        // Scroll left
        assert_eq!(ScrollUtils::scroll_horizontal(20, -5, 120, 100), 15);
        assert_eq!(ScrollUtils::scroll_horizontal(10, -10, 120, 100), 0);

        // Clamp at bounds
        assert_eq!(ScrollUtils::scroll_horizontal(0, -10, 120, 100), 0);
        assert_eq!(ScrollUtils::scroll_horizontal(0, 30, 120, 100), 20);

        // Content fits - can't scroll
        assert_eq!(ScrollUtils::scroll_horizontal(0, 10, 80, 100), 0);
    }

    // LineCount tests
    #[test]
    fn test_line_count() {
        assert_eq!(LineCount::count(""), 1);
        assert_eq!(LineCount::count("single line"), 1);
        assert_eq!(LineCount::count("line1\nline2"), 2);
        assert_eq!(LineCount::count("line1\nline2\nline3"), 3);
    }

    #[test]
    fn test_count_min() {
        assert_eq!(LineCount::count_min("", 3), 3);
        assert_eq!(LineCount::count_min("line1", 3), 3);
        assert_eq!(LineCount::count_min("line1\nline2\nline3", 3), 3);
        assert_eq!(LineCount::count_min("line1\nline2\nline3\nline4", 3), 4);
    }

    #[test]
    fn test_count_max() {
        assert_eq!(LineCount::count_max("", 5), 1);
        assert_eq!(LineCount::count_max("line1", 5), 1);
        assert_eq!(LineCount::count_max("line1\nline2", 5), 2);
        assert_eq!(LineCount::count_max("line1\nline2\nline3\nline4\nline5", 5), 5);
        assert_eq!(
            LineCount::count_max("line1\nline2\nline3\nline4\nline5\nline6", 5),
            5
        );
    }

    #[test]
    fn test_count_range() {
        assert_eq!(LineCount::count_range("", 3, 10), 3);
        assert_eq!(LineCount::count_range("line1", 3, 10), 3);
        assert_eq!(LineCount::count_range("line1\nline2\nline3", 3, 10), 3);
        assert_eq!(
            LineCount::count_range("line1\nline2\nline3\nline4\nline5\nline6\nline7\nline8\nline9\nline10\nline11\nline12", 3, 10),
            10
        );
    }
}
