//! Notification area component for displaying toast, progress, and warning notifications.
//!
//! The notification area renders as a popup overlay anchored to the bottom-right,
//! using block characters to create a floating card effect above the statusline.
//!
//! Visual design:
//! ```text
//!                                                         ▗▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
//!                                                         ▌ ✓ Session saved
//! ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▌ ⏳ Indexing... 45%
//!  > input here                                           ▌ ⚠ Context at 85%
//! ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▘ message 4
//! NORMAL | model                                            message 5
//! ```

use crate::tui::oil::component::Component;
use crate::tui::oil::node::{col, overlay_from_bottom_right, row, styled, text, Node};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::{colors, styles};
use crate::tui::oil::ViewContext;
use crucible_core::types::{Notification, NotificationKind};
use std::time::{Duration, Instant};

/// Default auto-dismiss timeout for toast notifications (3 seconds).
const TOAST_TIMEOUT: Duration = Duration::from_secs(3);

/// Maximum number of notifications to display.
pub const MAX_VISIBLE_NOTIFICATIONS: usize = 5;

/// Block characters for the floating card effect.
mod block_chars {
    /// Top-left corner (rounded effect)
    pub const TOP_LEFT: char = '▗';
    /// Top edge
    pub const TOP_EDGE: char = '▄';
    /// Left border
    pub const LEFT_BORDER: char = '▌';
    /// Notch where input meets popup (bottom-left of popup area)
    pub const NOTCH: char = '▘';
}

/// Icons for different notification types.
mod icons {
    /// Toast/success icon
    pub const TOAST: &str = "✓";
    /// Progress icon
    pub const PROGRESS: &str = "⏳";
    /// Warning icon
    pub const WARNING: &str = "⚠";
}

/// A notification area that displays as a popup overlay.
///
/// Manages a collection of notifications with auto-dismiss for toasts
/// and persistent display for progress/warning notifications.
#[derive(Debug, Clone)]
pub struct NotificationArea {
    notifications: Vec<(Notification, Instant)>,
    visible: bool,
    max_visible: usize,
    width: usize,
}

impl Default for NotificationArea {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationArea {
    /// Create a new empty notification area.
    pub fn new() -> Self {
        Self {
            notifications: Vec::new(),
            visible: false,
            max_visible: MAX_VISIBLE_NOTIFICATIONS,
            width: 30,
        }
    }

    /// Set visibility of the notification area.
    #[must_use]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Set the maximum number of visible notifications.
    #[must_use]
    pub fn max_visible(mut self, max: usize) -> Self {
        self.max_visible = max;
        self
    }

    /// Set the width of the notification area.
    #[must_use]
    pub fn width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Show the notification area.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the notification area.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if the notification area is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Add a notification to the area.
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

    fn render_notification(&self, notification: &Notification) -> Node {
        let (icon, icon_style) = match &notification.kind {
            NotificationKind::Toast => (icons::TOAST, styles::success()),
            NotificationKind::Progress { .. } => (icons::PROGRESS, styles::info()),
            NotificationKind::Warning => (icons::WARNING, styles::warning()),
        };

        let mut items = vec![
            styled(
                format!("{} ", block_chars::LEFT_BORDER),
                Style::new().fg(colors::BORDER),
            ),
            styled(format!("{} ", icon), icon_style),
            styled(
                notification.message.clone(),
                Style::new().fg(colors::TEXT_PRIMARY),
            ),
        ];

        // Add progress bar for Progress notifications
        if let NotificationKind::Progress { current, total } = notification.kind {
            let percent = if total > 0 {
                (current as f64 / total as f64 * 100.0).round() as usize
            } else {
                0
            };
            items.push(styled(format!(" {}%", percent), styles::muted()));
        }

        row(items)
    }

    fn render_top_border(&self, content_width: usize) -> Node {
        let border_chars = format!(
            "{}{}",
            block_chars::TOP_LEFT,
            block_chars::TOP_EDGE.to_string().repeat(content_width)
        );
        styled(border_chars, Style::new().fg(colors::BORDER))
    }

    fn render_bottom_notch(&self) -> Node {
        styled(
            format!("{}", block_chars::NOTCH),
            Style::new().fg(colors::BORDER),
        )
    }
}

impl Component for NotificationArea {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        // Don't render if not visible or no notifications
        if !self.visible || self.notifications.is_empty() {
            return Node::Empty;
        }

        // Get notifications to display (most recent first, limited)
        let display_notifications: Vec<_> = self
            .notifications
            .iter()
            .rev()
            .take(self.max_visible)
            .collect();

        if display_notifications.is_empty() {
            return Node::Empty;
        }

        // Calculate content width based on longest message
        let content_width = display_notifications
            .iter()
            .map(|(n, _)| n.message.len() + 4) // +4 for icon and spacing
            .max()
            .unwrap_or(20)
            .min(self.width)
            .max(15);

        // Build the notification card
        let mut rows = vec![self.render_top_border(content_width)];

        for (notification, _) in display_notifications.iter().rev() {
            rows.push(self.render_notification(notification));
        }

        // Add bottom notch where it meets the input area
        rows.push(self.render_bottom_notch());

        let card = col(rows);

        // Position from bottom-right (offset = 1, above statusline)
        overlay_from_bottom_right(card, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crate::tui::oil::render::render_to_plain_text;

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
    fn empty_area_returns_empty_node() {
        let area = NotificationArea::new().visible(true);
        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn hidden_area_returns_empty_node() {
        let mut area = NotificationArea::new();
        area.add(sample_toast());
        // visible is false by default
        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn visible_area_with_toast_renders() {
        let mut area = NotificationArea::new().visible(true);
        area.add(sample_toast());
        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Session saved"));
        assert!(plain.contains(icons::TOAST));
    }

    #[test]
    fn progress_notification_shows_percentage() {
        let mut area = NotificationArea::new().visible(true);
        area.add(sample_progress());
        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Indexing files"));
        assert!(plain.contains("45%"));
        assert!(plain.contains(icons::PROGRESS));
    }

    #[test]
    fn warning_notification_shows_icon() {
        let mut area = NotificationArea::new().visible(true);
        area.add(sample_warning());
        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Context at 85%"));
        assert!(plain.contains(icons::WARNING));
    }

    #[test]
    fn multiple_notifications_stack() {
        let mut area = NotificationArea::new().visible(true);
        area.add(sample_toast());
        area.add(sample_progress());
        area.add(sample_warning());

        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);

        assert!(plain.contains("Session saved"));
        assert!(plain.contains("Indexing files"));
        assert!(plain.contains("Context at 85%"));
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
    fn max_visible_limits_display() {
        let mut area = NotificationArea::new().visible(true).max_visible(2);
        area.add(Notification::toast("First"));
        area.add(Notification::toast("Second"));
        area.add(Notification::toast("Third"));

        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        let plain = render_to_plain_text(&node, 80);

        // Should only show the 2 most recent (Second and Third)
        assert!(!plain.contains("First"));
        assert!(plain.contains("Second"));
        assert!(plain.contains("Third"));
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
    fn builder_methods() {
        let area = NotificationArea::new()
            .visible(true)
            .max_visible(3)
            .width(40);

        assert!(area.is_visible());
        assert_eq!(area.max_visible, 3);
        assert_eq!(area.width, 40);
    }

    #[test]
    fn notification_uses_right_aligned_overlay() {
        use crate::tui::oil::node::Node;
        use crate::tui::oil::overlay::OverlayAnchor;

        let mut area = NotificationArea::new().visible(true).width(25);
        area.add(Notification::toast("Test"));
        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));

        match node {
            Node::Overlay(overlay) => {
                assert!(
                    matches!(overlay.anchor, OverlayAnchor::FromBottomRight(_)),
                    "Notification should use FromBottomRight anchor"
                );
            }
            _ => panic!("Expected Overlay node"),
        }
    }
}
