//! Generic popup widget with selection and fuzzy filtering
//!
//! This module provides a generic [`Popup<T>`] that can display any list of items
//! implementing the [`PopupItem`] trait. Features include:
//!
//! - Keyboard navigation (up/down, page up/down, home/end)
//! - Viewport scrolling to keep selection visible
//! - Optional fuzzy filtering via Nucleo
//! - Multi-select support
//!
//! ## Example
//!
//! ```ignore
//! use crucible_cli::tui::widgets::{Popup, PopupItem, PopupConfig};
//!
//! #[derive(Clone)]
//! struct Command {
//!     name: String,
//!     description: String,
//! }
//!
//! impl PopupItem for Command {
//!     fn match_text(&self) -> &str { &self.name }
//!     fn label(&self) -> &str { &self.name }
//!     fn description(&self) -> Option<&str> { Some(&self.description) }
//! }
//!
//! let commands = vec![
//!     Command { name: "help".into(), description: "Show help".into() },
//!     Command { name: "quit".into(), description: "Exit".into() },
//! ];
//!
//! let popup = Popup::new(commands)
//!     .with_config(PopupConfig::default().filterable(true));
//! ```

use nucleo::pattern::{CaseMatching, Normalization};
use nucleo::{Config as NucleoConfig, Matcher, Utf32String};
use std::marker::PhantomData;

// ─────────────────────────────────────────────────────────────────────────────
// PopupItem Trait
// ─────────────────────────────────────────────────────────────────────────────

/// Trait for items that can be displayed in a popup
///
/// Implement this trait to make your type displayable in a [`Popup`].
pub trait PopupItem: Clone + Send + Sync + 'static {
    /// Text to match against for fuzzy filtering
    ///
    /// This is what the fuzzy matcher scores against.
    fn match_text(&self) -> &str;

    /// Primary display text (shown prominently)
    fn label(&self) -> &str;

    /// Secondary text (shown dimmed, optional)
    fn description(&self) -> Option<&str> {
        None
    }

    /// Category/kind label (e.g., "cmd", "file", "agent")
    fn kind_label(&self) -> Option<&str> {
        None
    }

    /// Icon or prefix character
    fn icon(&self) -> Option<char> {
        None
    }

    /// Whether this item can be selected
    ///
    /// Disabled items are shown but cannot be chosen.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Token to insert when item is selected
    ///
    /// Defaults to `label()`. Override if the inserted text differs
    /// from the display text (e.g., "/help " vs "help").
    fn token(&self) -> &str {
        self.label()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scored Item (internal)
// ─────────────────────────────────────────────────────────────────────────────

/// An item with its fuzzy match score
#[derive(Clone, Debug)]
struct ScoredItem<T> {
    /// Original index in the source items
    index: usize,
    /// Fuzzy match score (higher = better match)
    score: u32,
    /// Phantom for the item type
    _phantom: PhantomData<T>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Fuzzy Matcher
// ─────────────────────────────────────────────────────────────────────────────

/// Fuzzy matcher configuration and state
pub struct FuzzyMatcher {
    matcher: Matcher,
    case_matching: CaseMatching,
    normalization: Normalization,
}

impl std::fmt::Debug for FuzzyMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FuzzyMatcher")
            .field("case_matching", &self.case_matching)
            .field("normalization", &self.normalization)
            .finish_non_exhaustive()
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self {
            matcher: Matcher::new(NucleoConfig::DEFAULT),
            case_matching: CaseMatching::Ignore,
            normalization: Normalization::Smart,
        }
    }
}

impl FuzzyMatcher {
    /// Create a new fuzzy matcher with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Score a single item against a query
    ///
    /// Returns `Some(score)` if the item matches, `None` otherwise.
    pub fn score(&mut self, query: &str, text: &str) -> Option<u32> {
        if query.is_empty() {
            // Empty query matches everything with score 0
            return Some(0);
        }

        let pattern = nucleo::pattern::Pattern::new(
            query,
            self.case_matching,
            self.normalization,
            nucleo::pattern::AtomKind::Fuzzy,
        );

        let haystack = Utf32String::from(text);
        pattern.score(haystack.slice(..), &mut self.matcher)
    }

    /// Filter and score items, returning indices sorted by score (descending)
    fn filter<T: PopupItem>(&mut self, query: &str, items: &[T]) -> Vec<ScoredItem<T>> {
        let mut scored: Vec<ScoredItem<T>> = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                self.score(query, item.match_text()).map(|score| ScoredItem {
                    index,
                    score,
                    _phantom: PhantomData,
                })
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.score.cmp(&a.score));
        scored
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Viewport State
// ─────────────────────────────────────────────────────────────────────────────

/// Viewport bounds for rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportBounds {
    /// First visible index
    pub start: usize,
    /// One past the last visible index
    pub end: usize,
}

impl ViewportBounds {
    /// Number of visible items
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Whether viewport is empty
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// Viewport state for scrolling
#[derive(Debug, Clone)]
pub struct PopupViewport {
    /// First visible item index
    offset: usize,
    /// Maximum visible items
    max_visible: usize,
}

impl PopupViewport {
    /// Create a new viewport with the given max visible items
    pub fn new(max_visible: usize) -> Self {
        Self {
            offset: 0,
            max_visible,
        }
    }

    /// Get current offset
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Get max visible items
    pub fn max_visible(&self) -> usize {
        self.max_visible
    }

    /// Update viewport to keep selection visible
    ///
    /// Returns true if viewport changed.
    pub fn follow_selection(&mut self, selected: usize, total_items: usize) -> bool {
        let old_offset = self.offset;

        // Clamp max visible to total items
        let visible = self.max_visible.min(total_items);

        // If selection is above viewport, scroll up
        if selected < self.offset {
            self.offset = selected;
        }
        // If selection is below viewport, scroll down
        else if selected >= self.offset + visible {
            self.offset = selected.saturating_sub(visible.saturating_sub(1));
        }

        // Ensure offset doesn't go past the end
        if total_items > visible {
            self.offset = self.offset.min(total_items - visible);
        } else {
            self.offset = 0;
        }

        self.offset != old_offset
    }

    /// Get visible bounds
    pub fn bounds(&self, total_items: usize) -> ViewportBounds {
        let start = self.offset;
        let end = (self.offset + self.max_visible).min(total_items);
        ViewportBounds { start, end }
    }

    /// Reset viewport to top
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// Scroll by a number of items (positive = down, negative = up)
    pub fn scroll(&mut self, delta: isize, total_items: usize) {
        if total_items == 0 {
            self.offset = 0;
            return;
        }

        let new_offset = if delta < 0 {
            self.offset.saturating_sub((-delta) as usize)
        } else {
            self.offset.saturating_add(delta as usize)
        };

        let max_offset = total_items.saturating_sub(self.max_visible);
        self.offset = new_offset.min(max_offset);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Popup Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for a popup
#[derive(Debug, Clone)]
pub struct PopupConfig {
    /// Maximum visible items (default: 10)
    pub max_visible: usize,
    /// Enable fuzzy filtering (default: true)
    pub filterable: bool,
    /// Maximum results to keep after filtering (default: 50)
    pub max_results: usize,
    /// Title for the popup
    pub title: Option<String>,
    /// Enable multi-select mode
    pub multi_select: bool,
    /// Show kind labels
    pub show_kinds: bool,
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            max_visible: 10,
            filterable: true,
            max_results: 50,
            title: None,
            multi_select: false,
            show_kinds: true,
        }
    }
}

impl PopupConfig {
    /// Set maximum visible items
    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible = n;
        self
    }

    /// Enable or disable filtering
    pub fn filterable(mut self, enabled: bool) -> Self {
        self.filterable = enabled;
        self
    }

    /// Set maximum results
    pub fn max_results(mut self, n: usize) -> Self {
        self.max_results = n;
        self
    }

    /// Set popup title
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Enable multi-select mode
    pub fn multi_select(mut self, enabled: bool) -> Self {
        self.multi_select = enabled;
        self
    }

    /// Show kind labels
    pub fn show_kinds(mut self, enabled: bool) -> Self {
        self.show_kinds = enabled;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Generic Popup
// ─────────────────────────────────────────────────────────────────────────────

/// Generic popup with selection and optional fuzzy filtering
///
/// `Popup<T>` manages a list of items of type `T` (which must implement [`PopupItem`]),
/// selection state, viewport scrolling, and optional fuzzy filtering.
///
/// The popup is decoupled from rendering — it only manages state. Use with a
/// widget renderer to display in TUI or web.
#[derive(Debug)]
pub struct Popup<T: PopupItem> {
    /// All items (unfiltered source)
    items: Vec<T>,
    /// Filtered indices (into `items`), sorted by score
    filtered: Vec<usize>,
    /// Currently selected index (into `filtered`)
    selected: usize,
    /// Multi-select: set of selected indices (into `filtered`)
    multi_selected: Vec<usize>,
    /// Current filter query
    query: String,
    /// Viewport state
    viewport: PopupViewport,
    /// Fuzzy matcher (if filtering enabled)
    matcher: Option<FuzzyMatcher>,
    /// Configuration
    config: PopupConfig,
}

impl<T: PopupItem> Popup<T> {
    /// Create a new popup with the given items
    pub fn new(items: Vec<T>) -> Self {
        let config = PopupConfig::default();
        let filtered: Vec<usize> = (0..items.len()).collect();
        let matcher = if config.filterable {
            Some(FuzzyMatcher::new())
        } else {
            None
        };

        Self {
            items,
            filtered,
            selected: 0,
            multi_selected: Vec::new(),
            query: String::new(),
            viewport: PopupViewport::new(config.max_visible),
            matcher,
            config,
        }
    }

    /// Create a new popup with custom configuration
    pub fn with_config(mut self, config: PopupConfig) -> Self {
        self.viewport = PopupViewport::new(config.max_visible);
        self.matcher = if config.filterable {
            Some(FuzzyMatcher::new())
        } else {
            None
        };
        self.config = config;
        self
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Accessors
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the configuration
    pub fn config(&self) -> &PopupConfig {
        &self.config
    }

    /// Get all items (unfiltered)
    pub fn all_items(&self) -> &[T] {
        &self.items
    }

    /// Get filtered item count
    pub fn filtered_count(&self) -> usize {
        self.filtered.len()
    }

    /// Get the current query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get the selected index (into filtered items)
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// Get the currently selected item, if any
    pub fn selected_item(&self) -> Option<&T> {
        self.filtered
            .get(self.selected)
            .and_then(|&idx| self.items.get(idx))
    }

    /// Get all selected items (for multi-select mode)
    pub fn selected_items(&self) -> Vec<&T> {
        if self.config.multi_select {
            self.multi_selected
                .iter()
                .filter_map(|&filtered_idx| {
                    self.filtered
                        .get(filtered_idx)
                        .and_then(|&idx| self.items.get(idx))
                })
                .collect()
        } else {
            self.selected_item().into_iter().collect()
        }
    }

    /// Get the viewport state
    pub fn viewport(&self) -> &PopupViewport {
        &self.viewport
    }

    /// Get visible items with their filtered indices
    ///
    /// Returns tuples of (filtered_index, &item, is_selected, is_multi_selected)
    pub fn visible_items(&self) -> Vec<(usize, &T, bool, bool)> {
        let bounds = self.viewport.bounds(self.filtered.len());
        self.filtered[bounds.start..bounds.end]
            .iter()
            .enumerate()
            .map(|(offset, &item_idx)| {
                let filtered_idx = bounds.start + offset;
                let is_selected = filtered_idx == self.selected;
                let is_multi = self.multi_selected.contains(&filtered_idx);
                (&self.items[item_idx], filtered_idx, is_selected, is_multi)
            })
            .map(|(item, idx, sel, multi)| (idx, item, sel, multi))
            .collect()
    }

    /// Check if there are items above the viewport
    pub fn has_items_above(&self) -> bool {
        self.viewport.offset() > 0
    }

    /// Check if there are items below the viewport
    pub fn has_items_below(&self) -> bool {
        let bounds = self.viewport.bounds(self.filtered.len());
        bounds.end < self.filtered.len()
    }

    /// Is the popup empty (no filtered items)?
    pub fn is_empty(&self) -> bool {
        self.filtered.is_empty()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Mutations
    // ─────────────────────────────────────────────────────────────────────────

    /// Set items (replaces all items and resets filter)
    pub fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.refilter();
    }

    /// Update the filter query
    ///
    /// This re-filters items and resets selection to first match.
    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refilter();
    }

    /// Append a character to the query
    pub fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.refilter();
    }

    /// Remove last character from query
    pub fn pop_char(&mut self) -> Option<char> {
        let c = self.query.pop();
        if c.is_some() {
            self.refilter();
        }
        c
    }

    /// Clear the query
    pub fn clear_query(&mut self) {
        if !self.query.is_empty() {
            self.query.clear();
            self.refilter();
        }
    }

    /// Re-filter items based on current query
    fn refilter(&mut self) {
        if let Some(ref mut matcher) = self.matcher {
            if self.query.is_empty() {
                // No query = show all items
                self.filtered = (0..self.items.len()).collect();
            } else {
                let scored = matcher.filter(&self.query, &self.items);
                self.filtered = scored
                    .into_iter()
                    .take(self.config.max_results)
                    .map(|s| s.index)
                    .collect();
            }
        } else {
            // No matcher = show all items
            self.filtered = (0..self.items.len()).collect();
        }

        // Reset selection and viewport
        self.selected = 0;
        self.multi_selected.clear();
        self.viewport.reset();
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Navigation
    // ─────────────────────────────────────────────────────────────────────────

    /// Move selection up by one
    pub fn move_up(&mut self) {
        self.move_selection(-1);
    }

    /// Move selection down by one
    pub fn move_down(&mut self) {
        self.move_selection(1);
    }

    /// Move selection by delta (positive = down, negative = up)
    ///
    /// Wraps around at boundaries.
    pub fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            self.selected = 0;
            return;
        }

        let len = self.filtered.len() as isize;
        let new_idx = (self.selected as isize + delta).rem_euclid(len);
        self.selected = new_idx as usize;

        // Skip disabled items
        self.skip_disabled(delta.signum());

        // Update viewport to follow selection
        self.viewport
            .follow_selection(self.selected, self.filtered.len());
    }

    /// Skip over disabled items in the given direction
    fn skip_disabled(&mut self, direction: isize) {
        if self.filtered.is_empty() {
            return;
        }

        let len = self.filtered.len();
        let mut attempts = 0;

        while attempts < len {
            if let Some(&item_idx) = self.filtered.get(self.selected) {
                if self.items[item_idx].is_enabled() {
                    return;
                }
            }

            let new_idx = (self.selected as isize + direction).rem_euclid(len as isize);
            self.selected = new_idx as usize;
            attempts += 1;
        }
    }

    /// Move selection to first item
    pub fn select_first(&mut self) {
        self.selected = 0;
        self.skip_disabled(1);
        self.viewport
            .follow_selection(self.selected, self.filtered.len());
    }

    /// Move selection to last item
    pub fn select_last(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self.filtered.len() - 1;
            self.skip_disabled(-1);
            self.viewport
                .follow_selection(self.selected, self.filtered.len());
        }
    }

    /// Page up (move by viewport height)
    pub fn page_up(&mut self) {
        let page_size = self.viewport.max_visible().max(1) as isize;
        self.move_selection(-page_size);
    }

    /// Page down (move by viewport height)
    pub fn page_down(&mut self) {
        let page_size = self.viewport.max_visible().max(1) as isize;
        self.move_selection(page_size);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Multi-Select
    // ─────────────────────────────────────────────────────────────────────────

    /// Toggle selection of current item (for multi-select mode)
    pub fn toggle_current(&mut self) {
        if !self.config.multi_select {
            return;
        }

        if let Some(pos) = self.multi_selected.iter().position(|&x| x == self.selected) {
            self.multi_selected.remove(pos);
        } else {
            // Only add if item is enabled
            if let Some(&item_idx) = self.filtered.get(self.selected) {
                if self.items[item_idx].is_enabled() {
                    self.multi_selected.push(self.selected);
                }
            }
        }
    }

    /// Select all visible items (for multi-select mode)
    pub fn select_all(&mut self) {
        if !self.config.multi_select {
            return;
        }

        self.multi_selected.clear();
        for (filtered_idx, &item_idx) in self.filtered.iter().enumerate() {
            if self.items[item_idx].is_enabled() {
                self.multi_selected.push(filtered_idx);
            }
        }
    }

    /// Clear all multi-selections
    pub fn clear_selection(&mut self) {
        self.multi_selected.clear();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct TestItem {
        name: String,
        desc: String,
        enabled: bool,
    }

    impl TestItem {
        fn new(name: &str, desc: &str) -> Self {
            Self {
                name: name.to_string(),
                desc: desc.to_string(),
                enabled: true,
            }
        }

        fn disabled(name: &str, desc: &str) -> Self {
            Self {
                name: name.to_string(),
                desc: desc.to_string(),
                enabled: false,
            }
        }
    }

    impl PopupItem for TestItem {
        fn match_text(&self) -> &str {
            &self.name
        }

        fn label(&self) -> &str {
            &self.name
        }

        fn description(&self) -> Option<&str> {
            Some(&self.desc)
        }

        fn is_enabled(&self) -> bool {
            self.enabled
        }
    }

    fn test_items() -> Vec<TestItem> {
        vec![
            TestItem::new("help", "Show help information"),
            TestItem::new("quit", "Exit the application"),
            TestItem::new("search", "Search for files"),
            TestItem::new("settings", "Open settings"),
            TestItem::new("save", "Save current file"),
        ]
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Basic Operations
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn new_popup_shows_all_items() {
        let popup = Popup::new(test_items());
        assert_eq!(popup.filtered_count(), 5);
        assert_eq!(popup.selected_index(), 0);
    }

    #[test]
    fn selected_item_returns_correct_item() {
        let popup = Popup::new(test_items());
        let item = popup.selected_item().unwrap();
        assert_eq!(item.name, "help");
    }

    #[test]
    fn empty_popup_handles_gracefully() {
        let popup: Popup<TestItem> = Popup::new(vec![]);
        assert!(popup.is_empty());
        assert!(popup.selected_item().is_none());
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Navigation
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn move_down_increments_selection() {
        let mut popup = Popup::new(test_items());
        popup.move_down();
        assert_eq!(popup.selected_index(), 1);
        assert_eq!(popup.selected_item().unwrap().name, "quit");
    }

    #[test]
    fn move_up_decrements_selection() {
        let mut popup = Popup::new(test_items());
        popup.move_down();
        popup.move_down();
        popup.move_up();
        assert_eq!(popup.selected_index(), 1);
    }

    #[test]
    fn navigation_wraps_at_end() {
        let mut popup = Popup::new(test_items());
        for _ in 0..5 {
            popup.move_down();
        }
        assert_eq!(popup.selected_index(), 0); // Wrapped to start
    }

    #[test]
    fn navigation_wraps_at_start() {
        let mut popup = Popup::new(test_items());
        popup.move_up();
        assert_eq!(popup.selected_index(), 4); // Wrapped to end
    }

    #[test]
    fn select_first_goes_to_start() {
        let mut popup = Popup::new(test_items());
        popup.move_down();
        popup.move_down();
        popup.select_first();
        assert_eq!(popup.selected_index(), 0);
    }

    #[test]
    fn select_last_goes_to_end() {
        let mut popup = Popup::new(test_items());
        popup.select_last();
        assert_eq!(popup.selected_index(), 4);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Filtering
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn set_query_filters_items() {
        let mut popup = Popup::new(test_items());
        popup.set_query("se"); // matches: search, settings, save
        assert!(popup.filtered_count() <= 5);
        // All results should contain 's' or 'e'
        for (_, item, _, _) in popup.visible_items() {
            assert!(item.name.contains('s') || item.name.contains('e'));
        }
    }

    #[test]
    fn clear_query_shows_all() {
        let mut popup = Popup::new(test_items());
        popup.set_query("se");
        let filtered = popup.filtered_count();
        popup.clear_query();
        assert_eq!(popup.filtered_count(), 5);
        assert!(popup.filtered_count() >= filtered);
    }

    #[test]
    fn push_char_updates_filter() {
        let mut popup = Popup::new(test_items());
        popup.push_char('h');
        popup.push_char('e');
        assert_eq!(popup.query(), "he");
    }

    #[test]
    fn pop_char_updates_filter() {
        let mut popup = Popup::new(test_items());
        popup.set_query("help");
        popup.pop_char();
        assert_eq!(popup.query(), "hel");
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Viewport
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn viewport_follows_selection() {
        let items: Vec<TestItem> = (0..20)
            .map(|i| TestItem::new(&format!("item{}", i), ""))
            .collect();

        let mut popup = Popup::new(items).with_config(PopupConfig::default().max_visible(5));

        // Move past viewport
        for _ in 0..7 {
            popup.move_down();
        }

        assert_eq!(popup.selected_index(), 7);
        assert!(popup.viewport().offset() > 0);
        assert!(popup.has_items_above());
    }

    #[test]
    fn viewport_bounds_correct() {
        let items: Vec<TestItem> = (0..20)
            .map(|i| TestItem::new(&format!("item{}", i), ""))
            .collect();

        let popup = Popup::new(items).with_config(PopupConfig::default().max_visible(5));

        let bounds = popup.viewport().bounds(popup.filtered_count());
        assert_eq!(bounds.start, 0);
        assert_eq!(bounds.end, 5);
        assert_eq!(bounds.len(), 5);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Disabled Items
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn navigation_skips_disabled_items() {
        let items = vec![
            TestItem::new("enabled1", ""),
            TestItem::disabled("disabled", ""),
            TestItem::new("enabled2", ""),
        ];

        let mut popup = Popup::new(items);
        assert_eq!(popup.selected_index(), 0);

        popup.move_down();
        // Should skip disabled and land on enabled2
        assert_eq!(popup.selected_index(), 2);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Multi-Select
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn multi_select_toggle() {
        let mut popup =
            Popup::new(test_items()).with_config(PopupConfig::default().multi_select(true));

        popup.toggle_current();
        assert_eq!(popup.selected_items().len(), 1);

        popup.move_down();
        popup.toggle_current();
        assert_eq!(popup.selected_items().len(), 2);

        // Toggle off first
        popup.move_up();
        popup.toggle_current();
        assert_eq!(popup.selected_items().len(), 1);
    }

    #[test]
    fn select_all_selects_enabled_only() {
        let items = vec![
            TestItem::new("enabled1", ""),
            TestItem::disabled("disabled", ""),
            TestItem::new("enabled2", ""),
        ];

        let mut popup =
            Popup::new(items).with_config(PopupConfig::default().multi_select(true));

        popup.select_all();
        assert_eq!(popup.selected_items().len(), 2);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // FuzzyMatcher
    // ─────────────────────────────────────────────────────────────────────────

    #[test]
    fn fuzzy_matcher_scores_exact_match() {
        let mut matcher = FuzzyMatcher::new();
        let score = matcher.score("help", "help");
        assert!(score.is_some());
    }

    #[test]
    fn fuzzy_matcher_scores_substring() {
        let mut matcher = FuzzyMatcher::new();
        let score = matcher.score("hlp", "help");
        assert!(score.is_some());
    }

    #[test]
    fn fuzzy_matcher_empty_query_matches_all() {
        let mut matcher = FuzzyMatcher::new();
        let score = matcher.score("", "anything");
        assert_eq!(score, Some(0));
    }

    #[test]
    fn fuzzy_matcher_no_match_returns_none() {
        let mut matcher = FuzzyMatcher::new();
        let score = matcher.score("xyz", "help");
        assert!(score.is_none());
    }
}
