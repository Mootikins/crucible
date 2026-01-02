//! Text selection state and logic for TUI
//!
//! Manages mouse-based text selection within the conversation viewport.
//! Uses a line-based selection model with a content cache for text extraction.

/// Selection anchor point in content coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionPoint {
    /// Line index (0-based, relative to content start)
    pub line: usize,
    /// Column index (0-based, in rendered characters)
    pub col: usize,
}

impl SelectionPoint {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl PartialOrd for SelectionPoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SelectionPoint {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.line, self.col).cmp(&(other.line, other.col))
    }
}

/// Selection state tracking mouse-based text selection
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    /// Selection start anchor (where mouse was pressed)
    anchor: Option<SelectionPoint>,
    /// Selection current position (where mouse is/was released)
    cursor: Option<SelectionPoint>,
    /// Whether we're currently in a drag operation
    is_dragging: bool,
}

impl SelectionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new selection at the given point
    pub fn start(&mut self, point: SelectionPoint) {
        self.anchor = Some(point);
        self.cursor = Some(point);
        self.is_dragging = true;
    }

    /// Update selection during drag
    pub fn update(&mut self, point: SelectionPoint) {
        if self.is_dragging {
            self.cursor = Some(point);
        }
    }

    /// Complete selection (mouse released)
    pub fn complete(&mut self) {
        self.is_dragging = false;
    }

    /// Clear selection
    pub fn clear(&mut self) {
        self.anchor = None;
        self.cursor = None;
        self.is_dragging = false;
    }

    /// Check if currently dragging
    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    /// Get normalized selection range (start <= end)
    pub fn range(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        match (self.anchor, self.cursor) {
            (Some(a), Some(c)) => {
                if a <= c {
                    Some((a, c))
                } else {
                    Some((c, a))
                }
            }
            _ => None,
        }
    }

    /// Check if a point is within the selection
    pub fn contains(&self, line: usize, col: usize) -> bool {
        self.range().is_some_and(|(start, end)| {
            if line < start.line || line > end.line {
                false
            } else if line == start.line && line == end.line {
                col >= start.col && col <= end.col
            } else if line == start.line {
                col >= start.col
            } else if line == end.line {
                col <= end.col
            } else {
                true // Middle lines are fully selected
            }
        })
    }

    /// Check if selection is non-empty (start != end)
    pub fn has_selection(&self) -> bool {
        self.range().is_some_and(|(s, e)| s != e)
    }
}

/// Information about a rendered line for selection text extraction
#[derive(Debug, Clone)]
pub struct RenderedLineInfo {
    /// The actual rendered text (plain, without ANSI codes)
    pub text: String,
    /// Index of the ConversationItem this line came from
    pub item_index: usize,
    /// Whether this is from a code block (affects formatting on extraction)
    pub is_code: bool,
}

/// Content cache for text extraction from selection
///
/// Populated during rendering to enable extracting actual text content
/// (not terminal cells with padding) when the user makes a selection.
#[derive(Debug, Default)]
pub struct SelectableContentCache {
    /// Rendered lines with source info
    lines: Vec<RenderedLineInfo>,
    /// Width when cache was built (invalidate on resize)
    cached_width: u16,
}

impl SelectableContentCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Update cache with new rendered lines
    pub fn update(&mut self, lines: Vec<RenderedLineInfo>, width: u16) {
        self.lines = lines;
        self.cached_width = width;
    }

    /// Check if cache needs rebuilding
    pub fn needs_rebuild(&self, width: u16) -> bool {
        self.lines.is_empty() || self.cached_width != width
    }

    /// Invalidate cache (call on content change)
    pub fn invalidate(&mut self) {
        self.lines.clear();
    }

    /// Get number of cached lines
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Get a specific line's info
    pub fn get_line(&self, index: usize) -> Option<&RenderedLineInfo> {
        self.lines.get(index)
    }

    /// Extract plain text from selection range
    pub fn extract_text(&self, start: SelectionPoint, end: SelectionPoint) -> String {
        let mut result = String::new();

        for line_idx in start.line..=end.line {
            if let Some(line_info) = self.lines.get(line_idx) {
                let line = &line_info.text;
                let char_count = line.chars().count();

                // Calculate column bounds for this line
                let (col_start, col_end) = if line_idx == start.line && line_idx == end.line {
                    // Single line selection
                    (start.col, end.col.min(char_count.saturating_sub(1)))
                } else if line_idx == start.line {
                    // First line of multi-line selection
                    (start.col, char_count.saturating_sub(1))
                } else if line_idx == end.line {
                    // Last line of multi-line selection
                    (0, end.col.min(char_count.saturating_sub(1)))
                } else {
                    // Middle line - select all
                    (0, char_count.saturating_sub(1))
                };

                // Extract the selected portion
                if col_start <= col_end && col_start < char_count {
                    let selected: String = line
                        .chars()
                        .skip(col_start)
                        .take(col_end.saturating_sub(col_start) + 1)
                        .collect();

                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&selected);
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_point_ordering() {
        let p1 = SelectionPoint::new(1, 5);
        let p2 = SelectionPoint::new(2, 3);
        let p3 = SelectionPoint::new(1, 10);

        assert!(p1 < p2);
        assert!(p1 < p3);
        assert!(p3 < p2);
    }

    #[test]
    fn selection_range_normalized() {
        let mut sel = SelectionState::new();

        // Drag downward
        sel.start(SelectionPoint::new(2, 5));
        sel.update(SelectionPoint::new(5, 10));

        let (start, end) = sel.range().unwrap();
        assert_eq!(start, SelectionPoint::new(2, 5));
        assert_eq!(end, SelectionPoint::new(5, 10));

        // Drag upward (reversed)
        sel.start(SelectionPoint::new(5, 10));
        sel.update(SelectionPoint::new(2, 5));

        let (start, end) = sel.range().unwrap();
        assert_eq!(start, SelectionPoint::new(2, 5));
        assert_eq!(end, SelectionPoint::new(5, 10));
    }

    #[test]
    fn selection_contains_point() {
        let mut sel = SelectionState::new();
        sel.start(SelectionPoint::new(1, 5));
        sel.update(SelectionPoint::new(3, 10));

        // Middle line - any column
        assert!(sel.contains(2, 0));
        assert!(sel.contains(2, 100));

        // Start line
        assert!(sel.contains(1, 5)); // At start col
        assert!(sel.contains(1, 10)); // After start col
        assert!(!sel.contains(1, 4)); // Before start col

        // End line
        assert!(sel.contains(3, 0)); // Before end col
        assert!(sel.contains(3, 10)); // At end col
        assert!(!sel.contains(3, 11)); // After end col

        // Outside selection
        assert!(!sel.contains(0, 5));
        assert!(!sel.contains(4, 0));
    }

    #[test]
    fn selection_has_selection() {
        let mut sel = SelectionState::new();
        assert!(!sel.has_selection());

        sel.start(SelectionPoint::new(1, 5));
        assert!(!sel.has_selection()); // Same point

        sel.update(SelectionPoint::new(1, 10));
        assert!(sel.has_selection()); // Different point
    }

    #[test]
    fn content_cache_extract_single_line() {
        let mut cache = SelectableContentCache::new();
        cache.update(
            vec![RenderedLineInfo {
                text: "Hello World".to_string(),
                item_index: 0,
                is_code: false,
            }],
            80,
        );

        let text = cache.extract_text(SelectionPoint::new(0, 0), SelectionPoint::new(0, 4));
        assert_eq!(text, "Hello");

        let text = cache.extract_text(SelectionPoint::new(0, 6), SelectionPoint::new(0, 10));
        assert_eq!(text, "World");
    }

    #[test]
    fn content_cache_extract_multi_line() {
        let mut cache = SelectableContentCache::new();
        cache.update(
            vec![
                RenderedLineInfo {
                    text: "First line".to_string(),
                    item_index: 0,
                    is_code: false,
                },
                RenderedLineInfo {
                    text: "Second line".to_string(),
                    item_index: 0,
                    is_code: false,
                },
                RenderedLineInfo {
                    text: "Third line".to_string(),
                    item_index: 0,
                    is_code: false,
                },
            ],
            80,
        );

        let text = cache.extract_text(SelectionPoint::new(0, 6), SelectionPoint::new(2, 4));
        assert_eq!(text, "line\nSecond line\nThird");
    }

    #[test]
    fn content_cache_invalidation() {
        let mut cache = SelectableContentCache::new();
        assert!(cache.needs_rebuild(80));

        cache.update(
            vec![RenderedLineInfo {
                text: "test".to_string(),
                item_index: 0,
                is_code: false,
            }],
            80,
        );
        assert!(!cache.needs_rebuild(80));
        assert!(cache.needs_rebuild(100)); // Different width

        cache.invalidate();
        assert!(cache.needs_rebuild(80));
    }
}
