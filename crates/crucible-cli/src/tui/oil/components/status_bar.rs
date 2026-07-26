use crate::tui::oil::chat_app::ChatMode;
use crucible_oil::node::{row, spacer, styled, Node};
use crucible_oil::style::{AdaptiveColor, Color, Style};

use crate::tui::oil::utils::truncate_to_chars;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationToastKind {
    Info,
    Warning,
    Error,
}

impl NotificationToastKind {
    pub fn color(&self) -> Color {
        let t = crate::tui::oil::theme::active();
        match self {
            NotificationToastKind::Info => t.resolve_color(t.colors.info),
            NotificationToastKind::Warning => t.resolve_color(t.colors.warning),
            NotificationToastKind::Error => t.resolve_color(t.colors.error),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            NotificationToastKind::Info => "INFO",
            NotificationToastKind::Warning => "WARN",
            NotificationToastKind::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StatusBar {
    pub mode: ChatMode,
    pub model: String,
    pub context_used: usize,
    pub context_total: usize,
    pub status: String,
    pub notification_toast: Option<(String, NotificationToastKind)>,
    pub notification_counts: Vec<(NotificationToastKind, usize)>,
    /// Prompt-cache hit rate (0.0..=1.0) from the latest `message_complete`.
    /// `None` when no cache event has fired for this session yet.
    pub cache_hit_rate: Option<f64>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mode(mut self, mode: ChatMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn context(mut self, used: usize, total: usize) -> Self {
        self.context_used = used;
        self.context_total = total;
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = status.into();
        self
    }

    pub fn toast(mut self, text: impl Into<String>, kind: NotificationToastKind) -> Self {
        self.notification_toast = Some((text.into(), kind));
        self
    }

    pub fn counts(mut self, counts: Vec<(NotificationToastKind, usize)>) -> Self {
        self.notification_counts = counts;
        self
    }

    fn mode_style(&self) -> Style {
        let t = crate::tui::oil::theme::active();
        let mode_bg = |color: AdaptiveColor| if t.is_dark { color.dark } else { color.light };
        match self.mode {
            ChatMode::Normal => Style::new()
                .bg(mode_bg(t.colors.mode_normal))
                .fg(Color::Black)
                .bold(),
            ChatMode::Plan => Style::new()
                .bg(mode_bg(t.colors.mode_plan))
                .fg(Color::Black)
                .bold(),
            ChatMode::Auto => Style::new()
                .bg(mode_bg(t.colors.mode_auto))
                .fg(Color::Black)
                .bold(),
        }
    }

    fn mode_label(&self) -> &'static str {
        match self.mode {
            ChatMode::Normal => " NORMAL ",
            ChatMode::Plan => " PLAN ",
            ChatMode::Auto => " AUTO ",
        }
    }

    fn context_display(&self) -> String {
        if self.context_total > 0 {
            let percent =
                (self.context_used as f64 / self.context_total as f64 * 100.0).round() as usize;
            format!("{}% ctx", percent)
        } else if self.context_used > 0 {
            format!("{}k tok", self.context_used / 1000)
        } else {
            String::new()
        }
    }

    fn model_display(&self) -> String {
        if self.model.is_empty() {
            "...".to_string()
        } else {
            truncate_to_chars(&self.model, 20, true).into_owned()
        }
    }
}

impl StatusBar {
    /// Render the `main` bar from the active item tree.
    pub fn view_from_items(&self, streaming: bool) -> Node {
        use crate::tui::oil::components::status_items::{render_bar, ItemContext, StatusBarData};

        let data = StatusBarData {
            mode: self.mode,
            model: self.model.clone(),
            context_used: self.context_used,
            context_total: self.context_total,
            status: self.status.clone(),
            notification_toast: self.notification_toast.clone(),
            notification_counts: self.notification_counts.clone(),
            cache_hit_rate: self.cache_hit_rate,
        };

        let bars = crate::tui::oil::theme::bars::active();
        let Some(main) = bars.get("main").or_else(|| bars.values().next()) else {
            return Node::Empty;
        };

        let exprs = crate::tui::oil::theme::exprs::snapshot();
        render_bar(
            &main.items,
            &ItemContext {
                data: &data,
                streaming,
                exprs: &exprs,
            },
        )
    }

    pub fn emergency_view(&self) -> Node {
        let t = crate::tui::oil::theme::active();
        row(vec![
            styled(self.mode_label().to_string(), self.mode_style()).no_shrink(),
            styled(
                " ".to_string(),
                Style::new().fg(t.resolve_color(t.colors.text_muted)),
            ),
            styled(
                self.model_display(),
                Style::new().fg(t.resolve_color(t.colors.primary)),
            ),
            spacer(),
            styled(
                self.context_display(),
                Style::new().fg(t.resolve_color(t.colors.text_muted)),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::render::render_to_plain_text;

    #[test]
    fn emergency_view_shows_mode() {
        let bar = StatusBar::new().mode(ChatMode::Normal);
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("NORMAL"));
    }

    #[test]
    fn emergency_view_shows_model_name() {
        let bar = StatusBar::new().model("gpt-4o-mini");
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("gpt-4o-mini"));
    }

    #[test]
    fn emergency_view_truncates_long_model() {
        let bar = StatusBar::new().model("very-long-model-name-that-exceeds-twenty-characters");
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("…"));
        assert!(!plain.contains("twenty-characters"));
    }

    #[test]
    fn emergency_view_shows_context_percentage() {
        let bar = StatusBar::new().context(4000, 8000);
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("50% ctx"));
    }

    #[test]
    fn emergency_view_shows_token_count_without_total() {
        let bar = StatusBar::new().context(5000, 0);
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        assert!(plain.contains("5k tok"));
    }

    #[test]
    fn status_bar_modes_have_different_colors() {
        let normal = StatusBar::new().mode(ChatMode::Normal);
        let plan = StatusBar::new().mode(ChatMode::Plan);
        let auto = StatusBar::new().mode(ChatMode::Auto);

        assert_ne!(normal.mode_style().bg, plan.mode_style().bg);
        assert_ne!(plan.mode_style().bg, auto.mode_style().bg);
    }

    #[test]
    fn emergency_view_spacer_fills_width() {
        // Test that spacer() in row layout expands to fill remaining width
        let bar = StatusBar::new()
            .mode(ChatMode::Normal)
            .model("gpt-4o")
            .context(4000, 8000);
        let plain = render_to_plain_text(&bar.emergency_view(), 80);
        // The status bar should use the full 80 character width
        // Mode label + space + model + spacer + context should fill 80 chars
        assert_eq!(
            plain.len(),
            80,
            "Status bar should fill full width at 80 chars"
        );
    }

    #[test]
    fn emergency_view_spacer_fills_width_at_120() {
        // Test that spacer() expands correctly at different widths
        let bar = StatusBar::new()
            .mode(ChatMode::Normal)
            .model("gpt-4o")
            .context(4000, 8000);
        let plain = render_to_plain_text(&bar.emergency_view(), 120);
        assert_eq!(
            plain.len(),
            120,
            "Status bar should fill full width at 120 chars"
        );
    }

    // The `config_driven` module that lived here tested the pre-item
    // component-table renderer, which no longer exists. Its coverage moved to
    // `status_items::tests`: section splitting, literal text, built-in
    // rendering, the empty bar, and the right-hand gutter. `separator` has no
    // successor by design — separators are now explicit text items.
}
