//! Selection subsystem manager
//!
//! Manages text selection, clipboard operations, and mouse mode.
//!
//! All fields are private; use the provided accessor methods.

use crate::tui::selection::{
    RenderedLineInfo, SelectableContentCache, SelectionPoint, SelectionState,
};

/// Manages text selection state
///
/// Owns all selection-related state:
/// - `selection`: Current selection range/state
/// - `clipboard`: Copied text content
/// - `mouse_mode`: Whether mouse capture is enabled
/// - `selection_cache`: Cached content for text extraction
#[derive(Debug)]
pub struct SelectionManager {
    /// Selection state (private - use accessor methods)
    selection: SelectionState,
    /// Clipboard content (private - use accessor methods)
    clipboard: Option<String>,
    /// Whether mouse mode is active (private - use accessor methods)
    mouse_mode: bool,
    /// Content cache for text extraction (private - use accessor methods)
    selection_cache: SelectableContentCache,
}

impl Default for SelectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionManager {
    pub fn new() -> Self {
        Self {
            selection: SelectionState::new(),
            clipboard: None,
            mouse_mode: true, // Enable by default for scroll support
            selection_cache: SelectableContentCache::new(),
        }
    }

    /// Check if mouse mode is enabled
    pub fn is_mouse_capture_enabled(&self) -> bool {
        self.mouse_mode
    }

    /// Set mouse mode
    pub fn set_mouse_mode(&mut self, enabled: bool) {
        self.mouse_mode = enabled;
    }

    /// Toggle mouse mode
    pub fn toggle_mouse_mode(&mut self) -> bool {
        self.mouse_mode = !self.mouse_mode;
        self.mouse_mode
    }

    /// Invalidate the selection cache
    pub fn invalidate_cache(&mut self) {
        self.selection_cache.invalidate();
    }

    /// Copy text to clipboard
    pub fn copy(&mut self, text: String) {
        self.clipboard = Some(text);
    }

    /// Get clipboard content
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    // Delegate methods to SelectionState
    /// Start a new selection (delegates to SelectionState)
    pub fn start_selection(&mut self, point: SelectionPoint) {
        self.selection.start(point);
    }

    /// Update selection (delegates to SelectionState)
    pub fn update_selection(&mut self, point: SelectionPoint) {
        self.selection.update(point);
    }

    /// Complete selection (delegates to SelectionState)
    pub fn complete_selection(&mut self) {
        self.selection.complete();
    }

    /// Check if selection has a range (delegates to SelectionState)
    pub fn has_selection(&self) -> bool {
        self.selection.has_selection()
    }

    /// Get selection range (delegates to SelectionState)
    pub fn selection_range(&self) -> Option<(SelectionPoint, SelectionPoint)> {
        self.selection.range()
    }

    /// Clear selection (delegates to SelectionState)
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    // =========================================================================
    // Selection state accessor
    // =========================================================================

    /// Get reference to selection state (for rendering)
    ///
    /// Used by `apply_selection_highlight` which needs the full state.
    pub fn selection(&self) -> &SelectionState {
        &self.selection
    }

    // =========================================================================
    // Cache delegation methods
    // =========================================================================

    /// Check if the selection cache needs rebuilding
    pub fn cache_needs_rebuild(&self, width: u16) -> bool {
        self.selection_cache.needs_rebuild(width)
    }

    /// Update the selection cache with new content data
    pub fn update_cache(&mut self, cache_data: Vec<RenderedLineInfo>, width: u16) {
        self.selection_cache.update(cache_data, width);
    }

    /// Extract text from the selection cache for the given range
    pub fn extract_text(&self, start: SelectionPoint, end: SelectionPoint) -> String {
        self.selection_cache.extract_text(start, end)
    }
}
