//! Notification store for managing toast, progress, and warning notifications.
//!
//! This module provides a data store for notifications. Rendering is handled
//! by the StatusBar (toast/counts) and a future notification drawer.

use super::status_bar::NotificationToastKind;
use crucible_core::types::{Notification, NotificationKind};
use std::time::{Duration, Instant};

/// Default auto-dismiss timeout for toast notifications (3 seconds).
const TOAST_TIMEOUT: Duration = Duration::from_secs(3);

/// A notification store that manages notification lifecycle.
///
/// Stores notifications with auto-dismiss for toasts and persistent
/// display for progress/warning notifications. Provides data for
/// StatusBar toast/count display and future drawer UI.
#[derive(Debug, Clone)]
pub struct NotificationArea {
    notifications: Vec<(Notification, Instant)>,
    visible: bool,
}

impl Default for NotificationArea {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationArea {
    /// Create a new empty notification store.
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            visible: false,
        }
    }

    /// Toggle visibility (for future drawer).
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Show the notification area (for future drawer).
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the notification area (for future drawer).
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if the notification area is visible (for future drawer).
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Add a notification to the store.
    pub fn add(&mut self, notification: Notification) {
        self.notifications.push((notification, Instant::now()));
    }

    /// Dismiss a notification by ID.
    pub fn dismiss(&mut self, id: &str) -> bool {
        if let Some(pos) = self.notifications.iter().position(|(n, _)| n.id == id) {
            self.notifications.remove(pos);
            true
        } else {
            false
        }
    }

    /// Clear all notifications.
    pub fn clear(&mut self) {
        self.notifications.clear();
    }

    /// Remove expired toast notifications.
    ///
    /// Returns the number of notifications removed.
    pub fn expire_toasts(&mut self) -> usize {
        let initial_len = self.notifications.len();
        self.notifications.retain(|(n, added_at)| match n.kind {
            NotificationKind::Toast => added_at.elapsed() < TOAST_TIMEOUT,
            NotificationKind::Progress { .. } | NotificationKind::Warning => true,
        });
        initial_len - self.notifications.len()
    }

    /// Get the number of active notifications.
    pub fn len(&self) -> usize {
        self.notifications.len()
    }

    /// Check if there are no notifications.
    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }

    /// Get the count of unread/active notifications (for badge display).
    pub fn unread_count(&self) -> usize {
        self.notifications.len()
    }

    /// Get the most recent notification as a toast for StatusBar display,
    /// only if it was added within `TOAST_TIMEOUT`.
    ///
    /// All notification types fade from the toast after the timeout.
    /// Warnings remain in the store for count badges but stop showing as toast text.
    pub fn active_toast(&self) -> Option<(&str, NotificationToastKind)> {
        let (n, instant) = self.notifications.last()?;
        if instant.elapsed() >= TOAST_TIMEOUT {
            return None;
        }
        let kind = match &n.kind {
            NotificationKind::Toast => NotificationToastKind::Info,
            NotificationKind::Progress { .. } => NotificationToastKind::Info,
            NotificationKind::Warning => NotificationToastKind::Warning,
        };
        Some((n.message.as_str(), kind))
    }

    pub fn warning_counts(&self) -> Vec<(NotificationToastKind, usize)> {
        let mut warn_count = 0usize;
        for (n, _) in &self.notifications {
            if matches!(n.kind, NotificationKind::Warning) {
                warn_count += 1;
            }
        }
        let mut counts = Vec::new();
        if warn_count > 0 {
            counts.push((NotificationToastKind::Warning, warn_count));
        }
        counts
    }

    /// Get notification history for future drawer use.
    pub fn history(&self) -> &[(Notification, Instant)] {
        &self.notifications
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_toast() -> Notification {
        Notification::toast("Session saved")
    }

    fn sample_progress() -> Notification {
        Notification::progress(45, 100, "Indexing files")
    }

    fn sample_warning() -> Notification {
        Notification::warning("Context at 85%")
    }

    #[test]
    fn add_and_dismiss_notifications() {
        let mut area = NotificationArea::new();
        let notif = sample_toast();
        let id = notif.id.clone();

        area.add(notif);
        assert_eq!(area.len(), 1);

        assert!(area.dismiss(&id));
        assert_eq!(area.len(), 0);

        // Dismissing non-existent returns false
        assert!(!area.dismiss("nonexistent"));
    }

    #[test]
    fn toggle_visibility() {
        let mut area = NotificationArea::new();
        assert!(!area.is_visible());

        area.toggle();
        assert!(area.is_visible());

        area.toggle();
        assert!(!area.is_visible());
    }

    #[test]
    fn clear_removes_all() {
        let mut area = NotificationArea::new();
        area.add(sample_toast());
        area.add(sample_warning());
        assert_eq!(area.len(), 2);

        area.clear();
        assert!(area.is_empty());
    }

    #[test]
    fn unread_count_reflects_total() {
        let mut area = NotificationArea::new();
        assert_eq!(area.unread_count(), 0);

        area.add(sample_toast());
        area.add(sample_warning());
        assert_eq!(area.unread_count(), 2);
    }

    #[test]
    fn active_toast_returns_most_recent() {
        let mut area = NotificationArea::new();
        assert!(area.active_toast().is_none());

        area.add(sample_toast());
        let (msg, kind) = area.active_toast().unwrap();
        assert_eq!(msg, "Session saved");
        assert_eq!(kind, NotificationToastKind::Info);

        area.add(sample_warning());
        let (msg, kind) = area.active_toast().unwrap();
        assert_eq!(msg, "Context at 85%");
        assert_eq!(kind, NotificationToastKind::Warning);
    }

    #[test]
    fn active_toast_maps_progress_to_info() {
        let mut area = NotificationArea::new();
        area.add(sample_progress());
        let (_, kind) = area.active_toast().unwrap();
        assert_eq!(kind, NotificationToastKind::Info);
    }

    #[test]
    fn warning_counts_returns_nonzero_only() {
        let mut area = NotificationArea::new();
        assert!(area.warning_counts().is_empty());

        area.add(sample_toast());
        assert!(area.warning_counts().is_empty());

        area.add(sample_warning());
        area.add(sample_warning());
        let counts = area.warning_counts();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0], (NotificationToastKind::Warning, 2));
    }

    #[test]
    fn history_returns_all_notifications() {
        let mut area = NotificationArea::new();
        area.add(sample_toast());
        area.add(sample_progress());
        area.add(sample_warning());
        assert_eq!(area.history().len(), 3);
    }
}
