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
    /// Top-left corner (three quadrants filled - missing lower-right)
    pub const TOP_LEFT: char = '▛';
    /// Top edge
    pub const TOP_EDGE: char = '▀';
    /// Left border
    pub const LEFT_BORDER: char = '▌';
    /// Notch where input meets popup (bottom-left of popup area)
    pub const NOTCH: char = '▘';
    /// Connection point on input box (three quadrants filled - missing upper-right)
    pub const CONNECTION: char = '▙';
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

    /// Calculate the column where the notification card ends (for input connection point).
    /// Returns None if no notifications are visible.
    pub fn card_end_column(&self, terminal_width: usize) -> Option<usize> {
        if !self.visible || self.notifications.is_empty() {
            return None;
        }

        let display_notifications: Vec<_> = self
            .notifications
            .iter()
            .rev()
            .take(self.max_visible)
            .collect();

        if display_notifications.is_empty() {
            return None;
        }

        let max_msg_len = display_notifications
            .iter()
            .map(|(n, _)| n.message.len())
            .max()
            .unwrap_or(0);

        let card_width = max_msg_len + 5;
        Some(terminal_width.saturating_sub(card_width))
    }

    fn render_notification(&self, notification: &Notification, max_message_len: usize) -> Node {
        use unicode_width::UnicodeWidthStr;

        let (icon, icon_style) = match &notification.kind {
            NotificationKind::Toast => (icons::TOAST, styles::success()),
            NotificationKind::Progress { .. } => (icons::PROGRESS, styles::info()),
            NotificationKind::Warning => (icons::WARNING, styles::warning()),
        };

        // Detect icon width and add padding for single-width icons
        let icon_width = UnicodeWidthStr::width(icon);
        let padded_icon = if icon_width == 1 {
            // Single-width icon (✓, ⚠) → add space padding: " ✓ "
            format!(" {} ", icon)
        } else {
            // Double-width icon (⏳) → single space: "⏳ "
            format!("{} ", icon)
        };

        // Pad message to max_message_len with trailing space for right-side padding
        let padded_message = format!("{:width$} ", notification.message, width = max_message_len);

        let mut items = vec![
            styled(
                format!("{}", block_chars::LEFT_BORDER),
                Style::new().fg(colors::BORDER),
            ),
            styled(padded_icon, icon_style),
            styled(padded_message, Style::new().fg(colors::TEXT_PRIMARY)),
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
        // Top border width must match message row width for alignment.
        // Message row = border(1) + padded_icon(3) + message(max_msg_len) = 4 + max_msg_len
        // content_width = 4 + max_msg_len
        // Top border = TOP_LEFT(1) + TOP_EDGE × (content_width - 1) = content_width
        let border_chars = format!(
            "{}{}",
            block_chars::TOP_LEFT,
            block_chars::TOP_EDGE
                .to_string()
                .repeat(content_width.saturating_sub(1))
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

        // Calculate max message length for alignment
        let max_message_len = display_notifications
            .iter()
            .map(|(n, _)| n.message.len())
            .max()
            .unwrap_or(0);

        // Calculate content width based on longest message
        // +5 = border(1) + icon_padding(3) + trailing_space(1)
        let content_width = max_message_len.saturating_add(5).min(self.width).max(15);

        // Build the notification card
        let mut rows = vec![self.render_top_border(content_width)];

        for (notification, _) in display_notifications.iter().rev() {
            rows.push(self.render_notification(notification, max_message_len));
        }

        let card = col(rows);

        // Position from bottom-right (offset = 4: 3 blank lines + 1 statusline)
        // This places notification 3 lines above the input box to avoid overlap
        overlay_from_bottom_right(card, 4)
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

    #[test]
    fn all_notification_rows_have_same_visible_width() {
        // Given multiple notifications with different message lengths
        let mut area = NotificationArea::new().visible(true);
        area.add(Notification::toast("Short"));
        area.add(Notification::toast("A much longer message here"));
        area.add(Notification::toast("Medium text"));

        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        let rendered = render_to_plain_text(&node, 80);

        // When we render, all rows (excluding top border) should have same width
        let lines: Vec<&str> = rendered.lines().collect();
        let message_lines: Vec<&str> = lines.iter().filter(|l| l.contains('▌')).copied().collect();

        // Then all message lines should have identical length
        assert!(!message_lines.is_empty(), "Should have message lines");
        let first_len = message_lines[0].len();
        for (i, line) in message_lines.iter().enumerate() {
            assert_eq!(
                line.len(),
                first_len,
                "Line {} has length {}, expected {} (all should match)\nLine: '{}'",
                i,
                line.len(),
                first_len,
                line
            );
        }
    }

    #[test]
    fn left_border_aligned_across_all_messages() {
        // Given multiple notifications
        let mut area = NotificationArea::new().visible(true);
        area.add(Notification::toast("Short"));
        area.add(Notification::toast("Much longer"));

        let h = ComponentHarness::new(80, 24);
        let node = area.view(&ViewContext::new(h.focus()));
        let rendered = render_to_plain_text(&node, 80);

        // When we find the position of ▌ in each line
        let border_positions: Vec<usize> = rendered.lines().filter_map(|l| l.find('▌')).collect();

        // Then all ▌ characters should be at the same column
        assert!(
            border_positions.len() >= 2,
            "Should have multiple borders, got {}",
            border_positions.len()
        );
        let first_pos = border_positions[0];
        for (i, pos) in border_positions.iter().enumerate() {
            assert_eq!(
                *pos, first_pos,
                "Border at line {} is at column {}, expected column {}",
                i, pos, first_pos
            );
        }
    }
}
