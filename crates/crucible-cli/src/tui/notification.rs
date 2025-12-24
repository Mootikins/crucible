//! Statusline notification system for file watch events.
//!
//! Accumulates file change and error events between render ticks,
//! then formats them as right-aligned notifications in the statusline.

use std::path::PathBuf;

/// Notification severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    /// Normal file changes (dim gray, 2s expiry)
    Info,
    /// Failures (red, 5s expiry)
    Error,
}

/// Manages notification state from watch events
///
/// Events are accumulated between render ticks. On each tick, pending
/// events are drained and formatted into a notification string.
pub struct NotificationState {
    /// Pending file change paths (accumulated between ticks)
    pending_changes: Vec<PathBuf>,
    /// Pending error messages (accumulated between ticks)
    pending_errors: Vec<String>,
}

impl NotificationState {
    /// Create a new empty notification state
    pub fn new() -> Self {
        Self {
            pending_changes: Vec::new(),
            pending_errors: Vec::new(),
        }
    }

    /// Check if there are no pending notifications
    pub fn is_empty(&self) -> bool {
        self.pending_changes.is_empty() && self.pending_errors.is_empty()
    }

    /// Accumulate a file change event
    pub fn push_change(&mut self, path: PathBuf) {
        self.pending_changes.push(path);
    }

    /// Accumulate an error event
    pub fn push_error(&mut self, message: String) {
        self.pending_errors.push(message);
    }
}

impl Default for NotificationState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_notification_module_exists() {
        let state = NotificationState::new();
        assert!(state.is_empty());
    }

    #[test]
    fn test_notification_level_variants() {
        let info = NotificationLevel::Info;
        let error = NotificationLevel::Error;
        assert!(!matches!(info, NotificationLevel::Error));
        assert!(matches!(error, NotificationLevel::Error));
    }

    #[test]
    fn test_push_change_accumulates() {
        let mut state = NotificationState::new();
        assert!(state.is_empty());

        state.push_change(PathBuf::from("/notes/a.md"));
        assert!(!state.is_empty());

        state.push_change(PathBuf::from("/notes/b.md"));
        assert!(!state.is_empty());
    }

    #[test]
    fn test_push_error_accumulates() {
        let mut state = NotificationState::new();
        state.push_error("parse failed".into());
        assert!(!state.is_empty());
    }
}
