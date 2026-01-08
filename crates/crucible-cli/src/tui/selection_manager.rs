//! Selection subsystem manager
//!
//! Manages text selection, clipboard operations, and mouse mode.

/// Manages text selection state
#[derive(Debug, Clone, Default)]
pub struct SelectionManager {
    /// Start position of selection (byte offset)
    pub selection_start: Option<usize>,
    /// End position of selection (byte offset)
    pub selection_end: Option<usize>,
    /// Clipboard content
    pub clipboard: Option<String>,
    /// Whether mouse mode is active
    pub mouse_mode: bool,
}

impl SelectionManager {
    pub fn new() -> Self {
        Self {
            selection_start: None,
            selection_end: None,
            clipboard: None,
            mouse_mode: false,
        }
    }

    /// Start a new selection
    pub fn start_selection(&mut self, pos: usize) {
        self.selection_start = Some(pos);
        self.selection_end = Some(pos);
    }

    /// Update selection end position
    pub fn update_selection(&mut self, pos: usize) {
        self.selection_end = Some(pos);
    }

    /// Clear the selection
    pub fn clear_selection(&mut self) {
        self.selection_start = None;
        self.selection_end = None;
    }

    /// Check if there's an active selection
    pub fn has_selection(&self) -> bool {
        self.selection_start.is_some() && self.selection_end.is_some()
    }

    /// Get the selected range (start, end)
    pub fn selection_range(&self) -> Option<(usize, usize)> {
        if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
            let (s, e) = if start < end { (start, end) } else { (end, start) };
            Some((s, e))
        } else {
            None
        }
    }

    /// Copy text to clipboard
    pub fn copy(&mut self, text: String) {
        self.clipboard = Some(text);
    }

    /// Get clipboard content
    pub fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
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
}
