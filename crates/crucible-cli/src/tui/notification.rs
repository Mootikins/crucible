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

    /// Drain pending events and format a notification
    ///
    /// Returns None if no pending events, otherwise returns a message and level.
    /// Errors take priority over file changes.
    pub fn render_tick(&mut self) -> Option<(String, NotificationLevel)> {
        if self.pending_changes.is_empty() && self.pending_errors.is_empty() {
            return None;
        }

        // Errors take priority - clear pending changes
        if !self.pending_errors.is_empty() {
            self.pending_changes.clear();
            let count = self.pending_errors.len();
            let message = if count == 1 {
                format!("✗ error: {}", self.pending_errors.remove(0))
            } else {
                format!("✗ {} errors", count)
            };
            self.pending_errors.clear();
            return Some((message, NotificationLevel::Error));
        }

        // Process changes
        if !self.pending_changes.is_empty() {
            let count = self.pending_changes.len();
            let message = if count == 1 {
                let path = self.pending_changes.remove(0);
                let filename = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");
                format!("{} modified", filename)
            } else {
                format!("{} files modified", count)
            };
            self.pending_changes.clear();

            return Some((message, NotificationLevel::Info));
        }

        None
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

    #[test]
    fn test_render_tick_single_file() {
        let mut state = NotificationState::new();
        state.push_change(PathBuf::from("/notes/todo.md"));
        let result = state.render_tick();
        assert!(result.is_some());
        let (msg, level) = result.unwrap();
        assert_eq!(msg, "todo.md modified");
        assert_eq!(level, NotificationLevel::Info);
    }

    #[test]
    fn test_render_tick_multiple_files() {
        let mut state = NotificationState::new();
        state.push_change(PathBuf::from("/notes/a.md"));
        state.push_change(PathBuf::from("/notes/b.md"));
        state.push_change(PathBuf::from("/notes/c.md"));
        let (msg, _) = state.render_tick().unwrap();
        assert_eq!(msg, "3 files modified");
    }

    #[test]
    fn test_errors_take_priority() {
        let mut state = NotificationState::new();
        state.push_change(PathBuf::from("/notes/a.md"));
        state.push_error("parse failed".into());
        let (msg, level) = state.render_tick().unwrap();
        assert!(msg.contains("error") || msg.contains("✗"));
        assert_eq!(level, NotificationLevel::Error);
    }
}
