use crate::tui::oil::chat_app::ChatMode;
use crate::tui::oil::component::Component;
use crate::tui::oil::node::{row, spacer, styled, Node};
use crate::tui::oil::style::{Color, Style};
use crate::tui::oil::theme::{colors, styles};
use crate::tui::oil::utils::truncate_to_chars;
use crate::tui::oil::ViewContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationToastKind {
    Info,
    Warning,
    Error,
}

impl NotificationToastKind {
    pub fn color(&self) -> Color {
        match self {
            NotificationToastKind::Info => colors::INFO,
            NotificationToastKind::Warning => colors::WARNING,
            NotificationToastKind::Error => colors::ERROR,
        }
    }

    fn label(&self) -> &'static str {
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
        match self.mode {
            ChatMode::Normal => styles::mode_normal(),
            ChatMode::Plan => styles::mode_plan(),
            ChatMode::Auto => styles::mode_auto(),
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

impl Component for StatusBar {
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        let mut items = vec![
            styled(self.mode_label().to_string(), self.mode_style()),
            styled(" ".to_string(), styles::muted()),
            styled(self.model_display(), styles::model_name()),
            styled(" ".to_string(), styles::muted()),
            styled(self.context_display(), styles::muted()),
        ];

        if !self.status.is_empty() {
            items.push(styled(" ".to_string(), styles::muted()));
            items.push(styled(self.status.clone(), styles::muted()));
        }

        if let Some((text, kind)) = &self.notification_toast {
            items.push(spacer());
            items.push(styled(text.clone(), styles::overlay_bright()));
            items.push(styled(" ".to_string(), Style::new()));
            items.push(styled(
                format!(" {} ", kind.label()),
                styles::notification_badge(kind.color()),
            ));
        } else if !self.notification_counts.is_empty() {
            items.push(spacer());
            for (kind, count) in &self.notification_counts {
                items.push(styled(
                    format!(" {} ", kind.label()),
                    styles::notification_badge(kind.color()),
                ));
                items.push(styled(
                    format!(" {} ", count),
                    Style::new().fg(kind.color()).bold(),
                ));
            }
        }

        row(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crate::tui::oil::render::render_to_plain_text;

    #[test]
    fn status_bar_shows_mode() {
        let bar = StatusBar::new().mode(ChatMode::Normal);
        let mut h = ComponentHarness::new(80, 1);
        h.render_component(&bar);
        assert!(h.viewport().contains("NORMAL"));
    }

    #[test]
    fn status_bar_shows_model_name() {
        let bar = StatusBar::new().model("gpt-4o-mini");
        let mut h = ComponentHarness::new(80, 1);
        h.render_component(&bar);
        let plain = render_to_plain_text(&bar.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains("gpt-4o-mini"));
    }

    #[test]
    fn status_bar_truncates_long_model() {
        let bar = StatusBar::new().model("very-long-model-name-that-exceeds-twenty-characters");
        let mut h = ComponentHarness::new(80, 1);
        h.render_component(&bar);
        let plain = render_to_plain_text(&bar.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains("…"));
        assert!(!plain.contains("twenty-characters"));
    }

    #[test]
    fn status_bar_shows_context_percentage() {
        let bar = StatusBar::new().context(4000, 8000);
        let mut h = ComponentHarness::new(80, 1);
        h.render_component(&bar);
        let plain = render_to_plain_text(&bar.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains("50% ctx"));
    }

    #[test]
    fn status_bar_shows_token_count_without_total() {
        let bar = StatusBar::new().context(5000, 0);
        let mut h = ComponentHarness::new(80, 1);
        h.render_component(&bar);
        let plain = render_to_plain_text(&bar.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains("5k tok"));
    }

    #[test]
    fn status_bar_shows_status_message() {
        let bar = StatusBar::new().status("Streaming...");
        let mut h = ComponentHarness::new(80, 1);
        h.render_component(&bar);
        let plain = render_to_plain_text(&bar.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains("Streaming..."));
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
    fn notification_toast_renders_text_and_badge() {
        let bar = StatusBar::new().toast("Processing", NotificationToastKind::Info);
        let h = ComponentHarness::new(80, 1);
        let plain = render_to_plain_text(&bar.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains("Processing"));
        assert!(plain.contains("INFO"));
    }

    #[test]
    fn notification_counts_render_badges_with_numbers() {
        let bar = StatusBar::new().counts(vec![
            (NotificationToastKind::Warning, 3),
            (NotificationToastKind::Error, 1),
        ]);
        let h = ComponentHarness::new(80, 1);
        let plain = render_to_plain_text(&bar.view(&ViewContext::new(h.focus())), 80);
        assert!(plain.contains("WARN"));
        assert!(plain.contains("3"));
        assert!(plain.contains("ERROR"));
        assert!(plain.contains("1"));
    }

    #[test]
    fn no_notification_when_empty() {
        let bar = StatusBar::new();
        let h = ComponentHarness::new(80, 1);
        let plain = render_to_plain_text(&bar.view(&ViewContext::new(h.focus())), 80);
        assert!(!plain.contains("INFO"));
        assert!(!plain.contains("WARN"));
        assert!(!plain.contains("ERRO"));
    }
}
