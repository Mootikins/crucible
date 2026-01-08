//! Selection subsystem manager
//!
//! Manages text selection, clipboard operations, and mouse mode.

use crate::tui::selection::{SelectionState, SelectableContentCache};

/// Manages text selection state
#[derive(Debug)]
pub struct SelectionManager {
    /// Selection state
    pub selection: SelectionState,
    /// Clipboard content
    pub clipboard: Option<String>,
    /// Whether mouse mode is active
    pub mouse_mode: bool,
    /// Content cache for text extraction
    pub selection_cache: SelectableContentCache,
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
    pub fn start_selection(&mut self, point: crate::tui::selection::SelectionPoint) {
        self.selection.start(point);
    }

    /// Update selection (delegates to SelectionState)
    pub fn update_selection(&mut self, point: crate::tui::selection::SelectionPoint) {
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
    pub fn selection_range(&self) -> Option<(crate::tui::selection::SelectionPoint, crate::tui::selection::SelectionPoint)> {
        self.selection.range()
    }

    /// Clear selection (delegates to SelectionState)
    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }
}
