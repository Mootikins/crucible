use crate::tui::oil::component::Component;
use crate::tui::oil::components::status_bar::NotificationToastKind;
use crate::tui::oil::components::{Drawer, DrawerKind};
use crate::tui::oil::node::{row, styled, Node};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::ViewContext;

/// A pre-computed notification entry for view-only rendering.
///
/// Converts `(Notification, Instant)` pairs into a format the component
/// can render without accessing `Instant` (which is hard to mock in tests).
#[derive(Debug, Clone)]
pub struct NotificationEntry {
    pub message: String,
    pub kind: NotificationToastKind,
    pub elapsed_secs: u64,
}

impl NotificationEntry {
    pub fn new(message: impl Into<String>, kind: NotificationToastKind, elapsed_secs: u64) -> Self {
        Self {
            message: message.into(),
            kind,
            elapsed_secs,
        }
    }

    fn timestamp_label(&self) -> String {
        let secs = self.elapsed_secs;
        if secs < 60 {
            format!("{:>2}s ago", secs)
        } else if secs < 3600 {
            format!("{:>2}m ago", secs / 60)
        } else {
            format!("{:>2}h ago", secs / 3600)
        }
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

        let theme = ThemeTokens::default_ref();

        let content_rows: Vec<Node> = self
            .entries
            .iter()
            .map(|entry| {
                let bg = theme.input_bg;
                let text_style = Style::new().bg(bg).fg(theme.overlay_text);
                let badge_style = theme.notification_badge(entry.kind.color());

                let timestamp_part = format!(" {}: ", entry.timestamp_label());
                let badge_text = format!(" {} ", entry.kind_label());
                let message_part = format!(" {}", entry.message);

                let used = timestamp_part.chars().count()
                    + badge_text.chars().count()
                    + message_part.chars().count();
                let padding = if self.width > used {
                    " ".repeat(self.width - used)
                } else {
                    String::new()
                };

                row([
                    styled(timestamp_part, text_style),
                    styled(badge_text, badge_style),
                    styled(message_part, text_style),
                    styled(padding, Style::new().bg(bg)),
                ])
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

    fn info_entry(msg: &str, secs: u64) -> NotificationEntry {
        NotificationEntry::new(msg, NotificationToastKind::Info, secs)
    }

    fn warning_entry(msg: &str, secs: u64) -> NotificationEntry {
        NotificationEntry::new(msg, NotificationToastKind::Warning, secs)
    }

    fn error_entry(msg: &str, secs: u64) -> NotificationEntry {
        NotificationEntry::new(msg, NotificationToastKind::Error, secs)
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
        let comp = NotificationComponent::new(vec![info_entry("hello", 5)])
            .visible(false)
            .width(80);
        let harness = ComponentHarness::new(80, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.trim().is_empty());
    }

    #[test]
    fn single_info_notification() {
        let comp = NotificationComponent::new(vec![info_entry("Session saved", 10)])
            .visible(true)
            .width(80);
        let harness = ComponentHarness::new(80, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("INFO"));
        assert!(plain.contains("Session saved"));
        assert!(plain.contains("10s ago"));
    }

    #[test]
    fn multiple_notifications_different_kinds() {
        let entries = vec![
            info_entry("Session saved", 5),
            warning_entry("Context at 85%", 120),
            error_entry("Connection failed", 7200),
        ];
        let comp = NotificationComponent::new(entries).visible(true).width(100);
        let harness = ComponentHarness::new(100, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 100);

        assert!(plain.contains("INFO"));
        assert!(plain.contains("Session saved"));
        assert!(plain.contains("5s ago"));

        assert!(plain.contains("WARN"));
        assert!(plain.contains("Context at 85%"));
        assert!(plain.contains("2m ago"));

        assert!(plain.contains("ERROR"));
        assert!(plain.contains("Connection failed"));
        assert!(plain.contains("2h ago"));
    }

    #[test]
    fn timestamp_formatting() {
        let entry_secs = NotificationEntry::new("a", NotificationToastKind::Info, 30);
        assert_eq!(entry_secs.timestamp_label(), "30s ago");

        let entry_mins = NotificationEntry::new("b", NotificationToastKind::Info, 150);
        assert_eq!(entry_mins.timestamp_label(), " 2m ago");

        let entry_hours = NotificationEntry::new("c", NotificationToastKind::Info, 7200);
        assert_eq!(entry_hours.timestamp_label(), " 2h ago");
    }

    #[test]
    fn drawer_has_borders() {
        let comp = NotificationComponent::new(vec![info_entry("test", 1)])
            .visible(true)
            .width(60);
        let harness = ComponentHarness::new(60, 24);
        let node = comp.view(&ViewContext::new(harness.focus()));
        let plain = render_to_plain_text(&node, 60);
        assert!(plain.contains('▄'));
        assert!(plain.contains('▀'));
    }
}
