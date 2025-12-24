//! Statusline notification system for file watch events.
//!
//! Accumulates file change and error events between render ticks,
//! then formats them as right-aligned notifications in the statusline.

use std::path::PathBuf;
use std::time::{Duration, Instant};

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
#[derive(Debug)]
pub struct NotificationState {
    /// Pending file change paths (accumulated between ticks)
    pending_changes: Vec<PathBuf>,
    /// Pending error messages (accumulated between ticks)
    pending_errors: Vec<String>,
    /// Current notification message (owned)
    current_message: String,
    /// Current notification level
    current_level: NotificationLevel,
    /// When the current notification expires
    expires_at: Option<Instant>,
}

impl NotificationState {
    /// Create a new empty notification state
    pub fn new() -> Self {
        Self {
            pending_changes: Vec::new(),
            pending_errors: Vec::new(),
            current_message: String::new(),
            current_level: NotificationLevel::Info,
            expires_at: None,
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

    /// Get current notification without draining pending events
    ///
    /// This is a non-mutating read for rendering. Call `tick()` before
    /// rendering to update state from pending events.
    pub fn current(&self) -> Option<(&str, NotificationLevel)> {
        if !self.current_message.is_empty() {
            Some((&self.current_message, self.current_level))
        } else {
            None
        }
    }

    /// Update state from pending events and check expiry
    ///
    /// Call this once per render cycle to drain pending events and
    /// update the current notification. Then use `current()` to read it.
    pub fn tick(&mut self) {
        // Check if current notification has expired
        if let Some(expires) = self.expires_at {
            if Instant::now() >= expires {
                self.expires_at = None;
                self.current_message.clear();
            }
        }

        // If there are new events, drain and format them
        if !self.pending_changes.is_empty() || !self.pending_errors.is_empty() {
            // Errors take priority - clear pending changes
            if !self.pending_errors.is_empty() {
                self.pending_changes.clear();
                let count = self.pending_errors.len();
                self.current_message = if count == 1 {
                    format!("✗ error: {}", self.pending_errors.remove(0))
                } else {
                    format!("✗ {} errors", count)
                };
                self.pending_errors.clear();
                self.current_level = NotificationLevel::Error;
                self.expires_at = Some(Instant::now() + Duration::from_secs(5));
            }
            // Process changes
            else if !self.pending_changes.is_empty() {
                let count = self.pending_changes.len();
                self.current_message = if count == 1 {
                    let path = self.pending_changes.remove(0);
                    let filename = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown");

                    // Truncate long filenames (max 25 chars for filename portion)
                    // Note: Could distinguish created vs modified if we changed push_change
                    // to accept FileChangeKind, but that requires API changes. Skipping for now.
                    let display_name = if filename.len() > 25 {
                        format!("{}...", &filename[..22])
                    } else {
                        filename.to_string()
                    };

                    format!("{} modified", display_name)
                } else {
                    format!("{} files modified", count)
                };
                self.pending_changes.clear();
                self.current_level = NotificationLevel::Info;
                self.expires_at = Some(Instant::now() + Duration::from_secs(2));
            }
        }
    }

    /// Drain pending events and format a notification
    ///
    /// Returns None if no pending events or notification expired, otherwise
    /// returns a reference to the current message and its level.
    /// Errors take priority over file changes.
    pub fn render_tick(&mut self) -> Option<(&str, NotificationLevel)> {
        // Check if current notification has expired
        if let Some(expires) = self.expires_at {
            if Instant::now() >= expires {
                self.expires_at = None;
                self.current_message.clear();
            }
        }

        // If there are new events, drain and format them
        if !self.pending_changes.is_empty() || !self.pending_errors.is_empty() {
            // Errors take priority - clear pending changes
            if !self.pending_errors.is_empty() {
                self.pending_changes.clear();
                let count = self.pending_errors.len();
                self.current_message = if count == 1 {
                    format!("✗ error: {}", self.pending_errors.remove(0))
                } else {
                    format!("✗ {} errors", count)
                };
                self.pending_errors.clear();
                self.current_level = NotificationLevel::Error;
                self.expires_at = Some(Instant::now() + Duration::from_secs(5));
            }
            // Process changes
            else if !self.pending_changes.is_empty() {
                let count = self.pending_changes.len();
                self.current_message = if count == 1 {
                    let path = self.pending_changes.remove(0);
                    let filename = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown");

                    // Truncate long filenames (max 25 chars for filename portion)
                    let display_name = if filename.len() > 25 {
                        format!("{}...", &filename[..22])
                    } else {
                        filename.to_string()
                    };

                    format!("{} modified", display_name)
                } else {
                    format!("{} files modified", count)
                };
                self.pending_changes.clear();
                self.current_level = NotificationLevel::Info;
                self.expires_at = Some(Instant::now() + Duration::from_secs(2));
            }
        }

        // Return current message if not expired
        if !self.current_message.is_empty() {
            Some((&self.current_message, self.current_level))
        } else {
            None
        }
    }

    /// Force expiry of current notification (for testing)
    #[cfg(test)]
    pub fn force_expire_for_testing(&mut self) {
        self.expires_at = Some(Instant::now() - Duration::from_secs(1));
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

    #[test]
    fn test_notification_expires() {
        let mut state = NotificationState::new();
        state.push_change(PathBuf::from("/notes/a.md"));
        assert!(state.render_tick().is_some());
        state.force_expire_for_testing();
        assert!(state.render_tick().is_none());
    }

    #[test]
    fn test_long_filename_truncated() {
        let mut state = NotificationState::new();
        state.push_change(PathBuf::from(
            "/notes/this-is-a-very-long-filename-that-should-be-truncated.md",
        ));

        let (msg, _) = state.render_tick().unwrap();
        assert!(msg.len() <= 40, "Message should be truncated: {}", msg);
        assert!(
            msg.contains("..."),
            "Truncated message should contain ellipsis"
        );
        assert!(
            msg.contains("modified"),
            "Message should contain 'modified'"
        );
    }

    #[test]
    fn test_short_filename_not_truncated() {
        let mut state = NotificationState::new();
        state.push_change(PathBuf::from("/notes/short.md"));

        let (msg, _) = state.render_tick().unwrap();
        assert_eq!(msg, "short.md modified");
        assert!(
            !msg.contains("..."),
            "Short filename should not be truncated"
        );
    }

    #[test]
    fn test_exactly_25_chars_not_truncated() {
        let mut state = NotificationState::new();
        // Create a filename that's exactly 25 chars
        let filename = "1234567890123456789012345"; // 25 chars
        state.push_change(PathBuf::from(format!("/notes/{}", filename)));

        let (msg, _) = state.render_tick().unwrap();
        assert_eq!(msg, format!("{} modified", filename));
        assert!(!msg.contains("..."));
    }

    #[test]
    fn test_26_chars_gets_truncated() {
        let mut state = NotificationState::new();
        // Create a filename that's 26 chars
        let filename = "12345678901234567890123456"; // 26 chars
        state.push_change(PathBuf::from(format!("/notes/{}", filename)));

        let (msg, _) = state.render_tick().unwrap();
        assert!(msg.contains("..."), "26-char filename should be truncated");
        // Should be first 22 chars + "..."
        assert!(msg.starts_with("1234567890123456789012"));
    }
}
