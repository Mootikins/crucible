//! Notification types for TUI and daemon communication.
//!
//! Notifications represent transient UI messages that can be displayed to users
//! across different interfaces (TUI, web, etc.). They are designed to be
//! serializable for RPC transport between daemon and clients.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Instant;

/// A notification message with metadata.
///
/// Notifications are identified by a unique ID and carry a kind that determines
/// their display behavior and lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: String,
    pub kind: NotificationKind,
    pub message: String,
    #[allow(dead_code)]
    pub(crate) created_at: Instant,
}

impl Serialize for Notification {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Notification", 3)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("kind", &self.kind)?;
        state.serialize_field("message", &self.message)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Notification {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct NotificationData {
            id: String,
            kind: NotificationKind,
            message: String,
        }

        let data = NotificationData::deserialize(deserializer)?;
        Ok(Notification {
            id: data.id,
            kind: data.kind,
            message: data.message,
            created_at: Instant::now(),
        })
    }
}

impl Notification {
    /// Create a new notification with a generated ID.
    pub fn new(kind: NotificationKind, message: impl Into<String>) -> Self {
        Self {
            id: generate_notification_id(),
            kind,
            message: message.into(),
            created_at: Instant::now(),
        }
    }

    /// Create a toast notification (auto-dismiss).
    pub fn toast(message: impl Into<String>) -> Self {
        Self::new(NotificationKind::Toast, message)
    }

    /// Create a progress notification.
    pub fn progress(current: usize, total: usize, message: impl Into<String>) -> Self {
        Self::new(NotificationKind::Progress { current, total }, message)
    }

    /// Create a warning notification (persistent).
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(NotificationKind::Warning, message)
    }
}

/// The kind of notification, determining display and lifecycle behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    /// Auto-dismissing informational message
    Toast,
    /// Progress indicator with current/total counts
    Progress { current: usize, total: usize },
    /// Persistent warning that requires user acknowledgment
    Warning,
}

/// A queue of notifications with expiration and dismissal support.
///
/// Manages a collection of notifications with FIFO ordering and provides
/// methods for adding, dismissing, and expiring old notifications.
#[derive(Debug, Default)]
pub struct NotificationQueue {
    notifications: VecDeque<Notification>,
}

impl NotificationQueue {
    /// Create a new empty notification queue.
    pub fn new() -> Self {
        Self {
            notifications: VecDeque::new(),
        }
    }

    /// Add a notification to the queue.
    pub fn add(&mut self, notification: Notification) {
        self.notifications.push_back(notification);
    }

    /// Dismiss a notification by ID.
    ///
    /// Returns `true` if a notification was dismissed, `false` if not found.
    pub fn dismiss(&mut self, id: &str) -> bool {
        if let Some(pos) = self.notifications.iter().position(|n| n.id == id) {
            self.notifications.remove(pos);
            true
        } else {
            false
        }
    }

    /// Remove notifications older than the given duration.
    ///
    /// Returns the number of notifications expired.
    pub fn expire_old(&mut self, max_age: std::time::Duration) -> usize {
        let now = Instant::now();
        let initial_len = self.notifications.len();

        self.notifications
            .retain(|n| now.duration_since(n.created_at) < max_age);

        initial_len - self.notifications.len()
    }

    /// Get all current notifications.
    pub fn notifications(&self) -> &VecDeque<Notification> {
        &self.notifications
    }

    /// Get the number of notifications in the queue.
    pub fn len(&self) -> usize {
        self.notifications.len()
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.notifications.is_empty()
    }

    /// Clear all notifications.
    pub fn clear(&mut self) {
        self.notifications.clear();
    }
}

/// Generate a unique notification ID.
fn generate_notification_id() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let random: String = (0..8)
        .map(|_| {
            let idx: u8 = rng.random_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + (idx - 10)) as char
            }
        })
        .collect();
    format!("notif-{}", random)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_serialization() {
        let notif = Notification::toast("Test message");

        let json = serde_json::to_string(&notif).unwrap();
        assert!(json.contains("\"kind\":\"toast\""));
        assert!(json.contains("\"message\":\"Test message\""));
        assert!(json.contains("\"id\":\"notif-"));

        let parsed: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kind, NotificationKind::Toast);
        assert_eq!(parsed.message, "Test message");
        assert_eq!(parsed.id, notif.id);
    }

    #[test]
    fn test_notification_kind_variants_serialize() {
        // Toast
        let toast = NotificationKind::Toast;
        let json = serde_json::to_string(&toast).unwrap();
        assert_eq!(json, "\"toast\"");
        let parsed: NotificationKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, NotificationKind::Toast);

        // Progress
        let progress = NotificationKind::Progress {
            current: 5,
            total: 10,
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"progress\""));
        assert!(json.contains("\"current\":5"));
        assert!(json.contains("\"total\":10"));
        let parsed: NotificationKind = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed,
            NotificationKind::Progress {
                current: 5,
                total: 10
            }
        );

        // Warning
        let warning = NotificationKind::Warning;
        let json = serde_json::to_string(&warning).unwrap();
        assert_eq!(json, "\"warning\"");
        let parsed: NotificationKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, NotificationKind::Warning);
    }

    #[test]
    fn test_notification_queue_add_and_dismiss() {
        let mut queue = NotificationQueue::new();
        assert!(queue.is_empty());

        let notif1 = Notification::toast("First");
        let notif2 = Notification::warning("Second");
        let id1 = notif1.id.clone();
        let id2 = notif2.id.clone();

        queue.add(notif1);
        queue.add(notif2);
        assert_eq!(queue.len(), 2);

        // Dismiss first notification
        assert!(queue.dismiss(&id1));
        assert_eq!(queue.len(), 1);

        // Try to dismiss again (should fail)
        assert!(!queue.dismiss(&id1));
        assert_eq!(queue.len(), 1);

        // Dismiss second notification
        assert!(queue.dismiss(&id2));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_notification_queue_expire_old() {
        use std::time::Duration;

        let mut queue = NotificationQueue::new();

        // Create notifications with different ages
        let mut old_notif = Notification::toast("Old");
        old_notif.created_at = Instant::now() - Duration::from_secs(10);

        let recent_notif = Notification::toast("Recent");

        queue.add(old_notif);
        queue.add(recent_notif);
        assert_eq!(queue.len(), 2);

        // Expire notifications older than 5 seconds
        let expired = queue.expire_old(Duration::from_secs(5));
        assert_eq!(expired, 1);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.notifications()[0].message, "Recent");
    }

    #[test]
    fn test_notification_queue_clear() {
        let mut queue = NotificationQueue::new();
        queue.add(Notification::toast("One"));
        queue.add(Notification::toast("Two"));
        assert_eq!(queue.len(), 2);

        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_notification_constructors() {
        let toast = Notification::toast("Toast message");
        assert_eq!(toast.kind, NotificationKind::Toast);
        assert_eq!(toast.message, "Toast message");

        let progress = Notification::progress(3, 10, "Progress message");
        assert_eq!(
            progress.kind,
            NotificationKind::Progress {
                current: 3,
                total: 10
            }
        );
        assert_eq!(progress.message, "Progress message");

        let warning = Notification::warning("Warning message");
        assert_eq!(warning.kind, NotificationKind::Warning);
        assert_eq!(warning.message, "Warning message");
    }

    #[test]
    fn test_notification_id_uniqueness() {
        let notif1 = Notification::toast("One");
        let notif2 = Notification::toast("Two");
        assert_ne!(notif1.id, notif2.id);
    }
}
