//! Statusline notification system for file watch events.
//!
//! Accumulates file change and error events between render ticks,
//! then formats them as right-aligned notifications in the statusline.

/// Notification severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    /// Normal file changes (dim gray, 2s expiry)
    Info,
    /// Failures (red, 5s expiry)
    Error,
}

/// Manages notification state from watch events
pub struct NotificationState {
    // TODO: implement
}

impl NotificationState {
    /// Create a new empty notification state
    pub fn new() -> Self {
        Self {}
    }

    /// Check if there are no pending notifications
    pub fn is_empty(&self) -> bool {
        true
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
}
