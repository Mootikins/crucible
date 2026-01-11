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
/// - `context_usage`: Optional (used, total) tokens for context window percentage
/// - `notification`: Optional (message, level) tuple for right-aligned notification
/// - `provider_model`: Optional (provider, model) tuple to display
///
/// # Layout
///
/// ```text
/// ▸ Plan │ ollama/llama3.2 │ 12% ctx │ Ready            File saved
/// └─┬──┘   └────┬────────┘   └──┬──┘   └──┬──┘           └───┬───┘
///   mode    provider/model   context   status          notification
/// ```
///
/// The notification is right-aligned if present.
pub struct StatusBarWidget<'a> {
    mode_id: &'a str,
    status_text: &'a str,
    /// Context usage as (used_tokens, context_window_size)
    context_usage: Option<(usize, usize)>,
    notification: Option<(&'a str, NotificationLevel)>,
    provider_model: Option<(&'a str, &'a str)>,
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
            context_usage: None,
            notification: None,
            provider_model: None,
        }
    }

    /// Set the context usage (displayed as percentage after model)
    ///
    /// # Arguments
    /// * `used` - Number of tokens used
    /// * `total` - Total context window size
    pub fn context_usage(mut self, used: usize, total: usize) -> Self {
        self.context_usage = Some((used, total));
        self
    }

    /// Set a notification to display on the right
    pub fn notification(mut self, notification: Option<(&'a str, NotificationLevel)>) -> Self {
        self.notification = notification;
        self
    }

    /// Set the provider and model to display
    pub fn provider_model(mut self, provider: &'a str, model: &'a str) -> Self {
        self.provider_model = Some((provider, model));
        self
    }
}

/// Truncate provider/model display to fit within max_width.
///
/// Strategy:
/// 1. If "provider/model" fits, return as-is
/// 2. Drop provider, show just model name
/// 3. Strip common quantization suffixes (q4_0, Q4_K_M, etc.)
/// 4. If still too long, truncate with ellipsis
fn truncate_provider_model(provider: &str, model: &str, max_width: usize) -> String {
    let full = format!("{}/{}", provider, model);
    if full.chars().count() <= max_width {
        return full;
    }

    // Step 2: Drop provider, just show model
    if model.chars().count() <= max_width {
        return model.to_string();
    }

    // Step 3: Strip common quantization suffixes
    let stripped = strip_quantization_suffix(model);
    if stripped.chars().count() <= max_width {
        return stripped.to_string();
    }

    // Step 4: Truncate with ellipsis
    let truncated: String = stripped.chars().take(max_width.saturating_sub(1)).collect();
    format!("{}…", truncated)
}

/// Strip common quantization suffixes from model names.
///
/// Handles patterns like:
/// - `-q4_k_m`, `-q8_0`, `-Q4_K_M` (hyphen-separated quantization)
/// - `-GGUF`, `-gguf` (GGUF format indicator)
/// - `:latest` (Ollama default tag)
fn strip_quantization_suffix(model: &str) -> &str {
    // Common suffix patterns to strip (longest first)
    let suffixes = [
        // Ollama tags
        ":latest",
        // Speculative decoding
        "-speculative",
        // GGUF indicators
        "-GGUF",
        "-gguf",
        // Extended quantization (XL variants)
        "-q8_k_xl",
        "-q4_k_xl",
        // Standard quantization suffixes
        "-q4_k_m",
        "-q4_k_s",
        "-q5_k_m",
        "-q5_k_s",
        "-q6_k",
        "-q8_0",
        "-q4_0",
        "-q4_1",
        "-q5_0",
        "-q5_1",
        "-fp16",
        "-f16",
    ];

    for suffix in &suffixes {
        if let Some(stripped) = model.strip_suffix(suffix) {
            return stripped;
        }
    }
    model
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

        // Build left-side spans: mode, provider/model (optional), token count (optional), status
        let mut left_spans = vec![
            Span::styled(indicators::MODE_ARROW, presets::dim()),
            Span::raw(" "),
            Span::styled(mode_name, mode_style),
        ];

        if let Some((provider, model)) = self.provider_model {
            left_spans.push(Span::styled(" │ ", presets::dim()));
            left_spans.push(Span::styled(
                truncate_provider_model(provider, model, 20),
                presets::dim(),
            ));
        }

        if let Some((used, total)) = self.context_usage {
            let percent = if total > 0 {
                (used as f64 / total as f64 * 100.0).round() as usize
            } else {
                0
            };
            left_spans.push(Span::styled(" │ ", presets::dim()));
            left_spans.push(Span::styled(
                format!("{}% ctx", percent),
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
        assert_eq!(widget.context_usage, None);
        assert_eq!(widget.notification, None);
    }

    #[test]
    fn test_context_usage_builder() {
        let widget = StatusBarWidget::new("act", "Processing").context_usage(1000, 8192);
        assert_eq!(widget.context_usage, Some((1000, 8192)));
    }

    #[test]
    fn test_notification_builder() {
        let widget = StatusBarWidget::new("auto", "Idle")
            .notification(Some(("File saved", NotificationLevel::Info)));
        assert!(widget.notification.is_some());
    }

    #[test]
    fn test_provider_model_builder() {
        let widget = StatusBarWidget::new("plan", "Ready").provider_model("ollama", "llama3.2");
        assert!(widget.provider_model.is_some());
    }

    #[test]
    fn test_truncate_short_model() {
        // Fits within 20 chars: "ollama/gpt-4" = 12 chars
        let result = truncate_provider_model("ollama", "gpt-4", 20);
        assert_eq!(result, "ollama/gpt-4");
    }

    #[test]
    fn test_truncate_drops_provider() {
        // "ollama/model-name-here" = 22 chars, drops provider
        let result = truncate_provider_model("ollama", "model-name-here", 20);
        assert_eq!(result, "model-name-here");
    }

    #[test]
    fn test_truncate_strips_quantization() {
        // Model with quantization suffix that's too long (21 chars)
        let result = truncate_provider_model("ollama", "longer-model-name-q8_0", 20);
        // Should strip -q8_0 to fit (17 chars after strip)
        assert_eq!(result, "longer-model-name");
    }

    #[test]
    fn test_truncate_with_ellipsis() {
        // Even after stripping, still too long - truncate with ellipsis
        let result = truncate_provider_model("provider", "very-long-model-name-that-wont-fit", 20);
        assert!(result.chars().count() <= 20);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_strip_speculative_suffix() {
        let result = strip_quantization_suffix("model-speculative");
        assert_eq!(result, "model");
    }

    #[test]
    fn test_strip_q8_k_xl_suffix() {
        let result = strip_quantization_suffix("model-q8_k_xl");
        assert_eq!(result, "model");
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
        fn with_context_usage() {
            let widget = StatusBarWidget::new("plan", "Ready").context_usage(1000, 8192);
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_with_context", terminal.backend());
        }

        #[test]
        fn without_context_usage() {
            let widget = StatusBarWidget::new("act", "Processing");
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_no_context", terminal.backend());
        }

        #[test]
        fn with_info_notification() {
            let widget = StatusBarWidget::new("plan", "Ready")
                .context_usage(4096, 8192)
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

        #[test]
        fn with_provider_model() {
            let widget = StatusBarWidget::new("plan", "Ready")
                .provider_model("openai", "gpt-4o")
                .context_usage(8000, 128000);
            let terminal = render_widget(widget);
            assert_snapshot!("status_bar_with_provider", terminal.backend());
        }
    }
}
