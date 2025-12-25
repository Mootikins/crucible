//! Status bar widget for displaying mode, status, and notifications
//!
//! This widget provides a status line typically shown at the bottom of the TUI,
//! displaying the current mode (plan/act/auto), token count, status text, and
//! optional notifications.

use crate::tui::{
    notification::NotificationLevel,
    styles::{indicators, presets},
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// Widget that renders a status bar with mode, status, and notification
///
/// # State
///
/// - `mode_id`: The current mode ("plan", "act", or "auto")
/// - `status_text`: Status message to display
/// - `token_count`: Optional token count to display
/// - `notification`: Optional (message, level) tuple for right-aligned notification
///
/// # Layout
///
/// ```text
/// ▸ Plan │ 127 tokens │ Ready            File saved
/// └─┬──┘   └────┬────┘   └──┬──┘           └───┬───┘
///   mode    tokens      status          notification
/// ```
///
/// The notification is right-aligned if present.
pub struct StatusBarWidget<'a> {
    mode_id: &'a str,
    status_text: &'a str,
    token_count: Option<usize>,
    notification: Option<(&'a str, NotificationLevel)>,
}

impl<'a> StatusBarWidget<'a> {
    /// Create a new status bar widget
    ///
    /// # Arguments
    ///
    /// * `mode_id` - The mode ("plan", "act", or "auto")
    /// * `status_text` - Status message to display
    pub fn new(mode_id: &'a str, status_text: &'a str) -> Self {
        Self {
            mode_id,
            status_text,
            token_count: None,
            notification: None,
        }
    }

    /// Set the token count (displayed after mode)
    pub fn token_count(mut self, count: usize) -> Self {
        self.token_count = Some(count);
        self
    }

    /// Set a notification to display on the right
    pub fn notification(mut self, notification: Option<(&'a str, NotificationLevel)>) -> Self {
        self.notification = notification;
        self
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode_style = presets::mode(self.mode_id);
        let mode_name = match self.mode_id {
            "plan" => "Plan",
            "act" => "Act",
            "auto" => "Auto",
            _ => self.mode_id,
        };

        // Build left-side spans: mode, token count (optional), status
        let mut left_spans = vec![
            Span::styled(indicators::MODE_ARROW, presets::dim()),
            Span::raw(" "),
            Span::styled(mode_name, mode_style),
        ];

        if let Some(count) = self.token_count {
            left_spans.push(Span::styled(" │ ", presets::dim()));
            left_spans.push(Span::styled(
                format!("{} tokens", count),
                presets::metrics(),
            ));
        }

        left_spans.push(Span::styled(" │ ", presets::dim()));
        left_spans.push(Span::styled(self.status_text.to_string(), presets::dim()));

        // Add notification on the right if present
        if let Some((msg, level)) = self.notification {
            let style = match level {
                NotificationLevel::Info => presets::dim(),
                NotificationLevel::Error => Style::default().fg(Color::Red),
            };

            // Calculate padding to right-align notification
            let left_text: String = left_spans.iter().map(|s| s.content.as_ref()).collect();
            let left_width = left_text.chars().count();
            let notif_text = format!(" {}", msg);
            let notif_width = notif_text.chars().count();
            let available_width = area.width as usize;

            if left_width + notif_width < available_width {
                let padding = available_width - left_width - notif_width;
                left_spans.push(Span::raw(" ".repeat(padding)));
                left_spans.push(Span::styled(notif_text, style));
            }
        }

        let line = Line::from(left_spans);
        let paragraph = Paragraph::new(line).style(presets::status_line());
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn test_widget_creation() {
        let widget = StatusBarWidget::new("plan", "Ready");
        assert_eq!(widget.mode_id, "plan");
        assert_eq!(widget.status_text, "Ready");
        assert_eq!(widget.token_count, None);
        assert_eq!(widget.notification, None);
    }

    #[test]
    fn test_token_count_builder() {
        let widget = StatusBarWidget::new("act", "Processing").token_count(127);
        assert_eq!(widget.token_count, Some(127));
    }

    #[test]
    fn test_notification_builder() {
        let widget =
            StatusBarWidget::new("auto", "Idle").notification(Some(("File saved", NotificationLevel::Info)));
        assert!(widget.notification.is_some());
    }

    #[test]
    fn test_basic_render() {
        let widget = StatusBarWidget::new("plan", "Ready");

        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|f| {
                let area = f.area();
                f.render_widget(widget, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..buffer.area().width)
            .map(|x| buffer.cell((x, 0)).map(|c| c.symbol()).unwrap_or(" "))
            .collect();

        // Should contain mode indicator, mode name, and status
        assert!(content.contains("Plan"));
        assert!(content.contains("Ready"));
    }

    // =============================================================================
    // Snapshot Tests
    // =============================================================================

    mod snapshot_tests {
        use super::*;
        use insta::assert_snapshot;

        const TEST_WIDTH: u16 = 80;
        const TEST_HEIGHT: u16 = 1;

        fn test_terminal() -> Terminal<TestBackend> {
            Terminal::new(TestBackend::new(TEST_WIDTH, TEST_HEIGHT)).unwrap()
        }

        fn render_widget(widget: StatusBarWidget) -> Terminal<TestBackend> {
            let mut terminal = test_terminal();
            terminal
                .draw(|f| {
                    f.render_widget(widget, f.area());
                })
                .unwrap();
            terminal
        }

        #[test]
        fn mode_plan_basic() {
            let widget = StatusBarWidget::new("plan", "Ready");
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_plan_basic", terminal.backend());
        }

        #[test]
        fn mode_act_basic() {
            let widget = StatusBarWidget::new("act", "Executing");
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_act_basic", terminal.backend());
        }

        #[test]
        fn mode_auto_basic() {
            let widget = StatusBarWidget::new("auto", "Idle");
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_auto_basic", terminal.backend());
        }

        #[test]
        fn with_token_count() {
            let widget = StatusBarWidget::new("plan", "Ready").token_count(127);
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_with_tokens", terminal.backend());
        }

        #[test]
        fn without_token_count() {
            let widget = StatusBarWidget::new("act", "Processing");
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_no_tokens", terminal.backend());
        }

        #[test]
        fn with_info_notification() {
            let widget = StatusBarWidget::new("plan", "Ready")
                .token_count(50)
                .notification(Some(("File saved", NotificationLevel::Info)));
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_info_notification", terminal.backend());
        }

        #[test]
        fn with_error_notification() {
            let widget = StatusBarWidget::new("act", "Error")
                .notification(Some(("Parse failed", NotificationLevel::Error)));
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_error_notification", terminal.backend());
        }

        #[test]
        fn long_status_text() {
            let widget = StatusBarWidget::new(
                "auto",
                "Processing a very long status message that might overflow",
            );
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_long_status", terminal.backend());
        }
    }
}
