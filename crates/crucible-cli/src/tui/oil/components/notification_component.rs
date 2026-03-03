use crate::tui::oil::component::Component;
use crate::tui::oil::components::status_bar::NotificationToastKind;
use crate::tui::oil::components::{Drawer, DrawerKind};
use crate::tui::oil::node::{row, styled, Node};
use crate::tui::oil::style::Style;

use crate::tui::oil::utils::wrap::wrap_to_width;
use crate::tui::oil::ViewContext;

/// A pre-computed notification entry for view-only rendering.
///
/// Converts `(Notification, Instant)` pairs into a format the component
/// can render without accessing `Instant` (which is hard to mock in tests).
#[derive(Debug, Clone)]
pub struct NotificationEntry {
    pub message: String,
    pub kind: NotificationToastKind,
    pub timestamp: String,
}

impl NotificationEntry {
    pub fn new(
        message: impl Into<String>,
        kind: NotificationToastKind,
        timestamp: impl Into<String>,
    ) -> Self {
        Self {
            message: message.into(),
            kind,
            timestamp: timestamp.into(),
        }
    }

    fn timestamp_label(&self) -> &str {
        &self.timestamp
    }

    fn kind_label(&self) -> &'static str {
        match self.kind {
            NotificationToastKind::Info => "INFO",
            NotificationToastKind::Warning => "WARN",
            NotificationToastKind::Error => "ERROR",
        }
    }
}

/// View-only component that renders the notification/messages drawer.
///
/// All notification state (history, visibility, mutations) remains on
/// `OilChatApp`. This component receives pre-computed props and renders
/// the drawer overlay.
pub struct NotificationComponent {
    pub visible: bool,
    pub entries: Vec<NotificationEntry>,
    pub width: usize,
}

impl NotificationComponent {
    pub fn new(entries: Vec<NotificationEntry>) -> Self {
        Self {
            visible: true,
            entries,
            width: 80,
        }
    }

    #[must_use]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    #[must_use]
    pub fn width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }
}

impl Component for NotificationComponent {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        if !self.visible {
            return Node::Empty;
        }

        let t = crate::tui::oil::theme::active();
        let bg = t.resolve_color(t.colors.background);
        let text_style = Style::new()
            .bg(bg)
            .fg(t.resolve_color(t.colors.overlay_text));

        let content_rows: Vec<Node> = self
            .entries
            .iter()
            .flat_map(|entry| {
                let badge_style = Style::new().fg(entry.kind.color()).bold().reverse();

                let timestamp_part = format!(" {}: ", entry.timestamp_label());
                let badge_text = format!(" {} ", entry.kind_label());

                // Calculate available width for message
                let prefix_width = timestamp_part.chars().count() + badge_text.chars().count();
                let msg_width = self.width.saturating_sub(prefix_width + 1); // +1 for space before message

                // Wrap message to available width
                let wrapped_lines = if msg_width == 0 {
                    // Fallback: render unsplit if no space available
                    vec![entry.message.clone()]
                } else {
                    wrap_to_width(&entry.message, msg_width)
                        .lines()
                        .map(|s| s.to_string())
                        .collect()
                };

                // Build rows: first line with timestamp+badge, continuation lines with indent
                let mut rows = Vec::new();
                for (i, line) in wrapped_lines.iter().enumerate() {
                    if i == 0 {
                        // First line: timestamp + badge + message
                        let message_part = format!(" {}", line);
                        let used = timestamp_part.chars().count()
                            + badge_text.chars().count()
                            + message_part.chars().count();
                        let padding = if self.width > used {
                            " ".repeat(self.width - used)
                        } else {
                            String::new()
                        };

                        rows.push(row([
                            styled(timestamp_part.clone(), text_style),
                            styled(badge_text.clone(), badge_style),
                            styled(message_part, text_style),
                            styled(padding, Style::new().bg(bg)),
                        ]));
                    } else {
                        // Continuation lines: indent + message
                        let indent = " ".repeat(prefix_width + 1);
                        let message_part = format!("{}{}", indent, line);
                        let used = message_part.chars().count();
                        let padding = if self.width > used {
                            " ".repeat(self.width - used)
                        } else {
                            String::new()
                        };

                        rows.push(row([
                            styled(message_part, text_style),
                            styled(padding, Style::new().bg(bg)),
                        ]));
                    }
                }

                rows
            })
            .collect();

        Drawer::new(DrawerKind::Messages)
            .content_rows(content_rows)
            .width(self.width)
            .view()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crate::tui::oil::render::render_to_plain_text;

    fn info_entry(msg: &str, ts: &str) -> NotificationEntry {
        NotificationEntry::new(msg, NotificationToastKind::Info, ts)
    }

    fn warning_entry(msg: &str, ts: &str) -> NotificationEntry {
        NotificationEntry::new(msg, NotificationToastKind::Warning, ts)
    }

    fn error_entry(msg: &str, ts: &str) -> NotificationEntry {
        NotificationEntry::new(msg, NotificationToastKind::Error, ts)
    }

    #[test]
    fn empty_entries_still_renders_drawer() {
        let comp = NotificationComponent::new(vec![]).visible(true).width(80);
        let harness = ComponentHarness::new(80, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("MESSAGES"));
        assert!(plain.contains("ESC/q"));
    }

    #[test]
    fn hidden_renders_empty() {
        let comp = NotificationComponent::new(vec![info_entry("hello", "00:00:05")])
            .visible(false)
            .width(80);
        let harness = ComponentHarness::new(80, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.trim().is_empty());
    }

    #[test]
    fn single_info_notification() {
        let comp = NotificationComponent::new(vec![info_entry("Session saved", "14:30:12")])
            .visible(true)
            .width(80);
        let harness = ComponentHarness::new(80, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("INFO"));
        assert!(plain.contains("Session saved"));
        assert!(plain.contains("14:30:12"));
    }

    #[test]
    fn multiple_notifications_different_kinds() {
        let entries = vec![
            info_entry("Session saved", "14:30:12"),
            warning_entry("Context at 85%", "14:32:00"),
            error_entry("Connection failed", "16:30:12"),
        ];
        let comp = NotificationComponent::new(entries).visible(true).width(100);
        let harness = ComponentHarness::new(100, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 100);

        assert!(plain.contains("INFO"));
        assert!(plain.contains("Session saved"));
        assert!(plain.contains("14:30:12"));

        assert!(plain.contains("WARN"));
        assert!(plain.contains("Context at 85%"));
        assert!(plain.contains("14:32:00"));

        assert!(plain.contains("ERROR"));
        assert!(plain.contains("Connection failed"));
        assert!(plain.contains("16:30:12"));
    }

    #[test]
    fn timestamp_formatting() {
        let entry_secs = NotificationEntry::new("a", NotificationToastKind::Info, "09:05:30");
        assert_eq!(entry_secs.timestamp_label(), "09:05:30");

        let entry_mins = NotificationEntry::new("b", NotificationToastKind::Info, "14:32:00");
        assert_eq!(entry_mins.timestamp_label(), "14:32:00");

        let entry_hours = NotificationEntry::new("c", NotificationToastKind::Info, "16:30:12");
        assert_eq!(entry_hours.timestamp_label(), "16:30:12");
    }

    #[test]
    fn drawer_has_borders() {
        let comp = NotificationComponent::new(vec![info_entry("test", "12:00:01")])
            .visible(true)
            .width(60);
        let harness = ComponentHarness::new(60, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 60);
        assert!(plain.contains('▄'));
        assert!(plain.contains('▀'));
    }

    #[test]
    fn multiline_notification_renders_first_line() {
        // Now that wrapping happens in the component, verify that a message
        // with embedded newlines is wrapped and both lines appear in output.
        let entry = NotificationEntry::new(
            "Error: something\nDetails here",
            NotificationToastKind::Error,
            "14:30:12",
        );
        let comp = NotificationComponent::new(vec![entry])
            .visible(true)
            .width(80);
        let harness = ComponentHarness::new(80, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 80);

        // Both lines should appear in the output
        assert!(plain.contains("Error: something"));
        assert!(plain.contains("Details here"));
        assert!(plain.contains("14:30:12"));
    }

    #[test]
    fn long_notification_wraps_to_width() {
        // A 120-character message in an 80-column component should wrap
        let long_msg = "This is a very long notification message that definitely exceeds the available width and should be wrapped to multiple lines for proper display";
        let entry = NotificationEntry::new(long_msg, NotificationToastKind::Info, "12:00:00");
        let comp = NotificationComponent::new(vec![entry])
            .visible(true)
            .width(80);
        let harness = ComponentHarness::new(80, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 80);

        // Message should be present and wrapped across multiple lines
        assert!(plain.contains("This is a very long"));
        assert!(plain.contains("notification message"));
        // Verify it's actually wrapped (contains newlines in the content)
        let lines: Vec<&str> = plain.lines().collect();
        assert!(
            lines.len() > 2,
            "Long message should wrap to multiple lines"
        );
    }

    #[test]
    fn short_notification_no_wrap() {
        // A short message should fit on one line
        let entry = NotificationEntry::new("Short msg", NotificationToastKind::Info, "12:00:00");
        let comp = NotificationComponent::new(vec![entry])
            .visible(true)
            .width(80);
        let harness = ComponentHarness::new(80, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 80);

        assert!(plain.contains("Short msg"));
        assert!(plain.contains("12:00:00"));
    }

    #[test]
    fn wrap_preserves_continuation_indent() {
        // Continuation lines should be indented to align with the message start
        let long_msg =
            "This is a message that will wrap to show continuation line indentation behavior";
        let entry = NotificationEntry::new(long_msg, NotificationToastKind::Warning, "14:30:00");
        let comp = NotificationComponent::new(vec![entry])
            .visible(true)
            .width(60);
        let harness = ComponentHarness::new(60, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 60);

        // Should have multiple lines
        let lines: Vec<&str> = plain.lines().collect();
        assert!(lines.len() > 1, "Message should wrap to multiple lines");

        // Continuation lines should have leading spaces (indentation)
        // The indent should be: " HH:MM:SS: " (11 chars) + " WARN " (6 chars) = 17 chars
        if lines.len() > 1 {
            let continuation = lines[1];
            // Continuation should start with spaces (the indent)
            assert!(
                continuation.starts_with(" "),
                "Continuation line should be indented: {:?}",
                continuation
            );
        }
    }
}
