use crate::tui::oil::chat_app::ChatMode;
use crate::tui::oil::component::Component;
use crate::tui::oil::components::status_bar::{NotificationToastKind, StatusBar};
use crate::tui::oil::node::{col, styled, Node};
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::ViewContext;

/// View-only component composing error display + [`StatusBar`].
///
/// Error (when present) renders above the status bar.  All state is
/// owned by `OilChatApp`; this struct borrows snapshots of it.
pub struct StatusComponent<'a> {
    pub mode: ChatMode,
    pub model: &'a str,
    pub context_used: usize,
    pub context_total: usize,
    pub status: &'a str,
    pub error: Option<&'a str>,
    pub toast: Option<(&'a str, NotificationToastKind)>,
    pub notification_counts: Vec<(NotificationToastKind, usize)>,
}

impl<'a> StatusComponent<'a> {
    pub fn new() -> Self {
        Self {
            mode: ChatMode::default(),
            model: "",
            context_used: 0,
            context_total: 0,
            status: "",
            error: None,
            toast: None,
            notification_counts: Vec::new(),
        }
    }

    pub fn mode(mut self, mode: ChatMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn model(mut self, model: &'a str) -> Self {
        self.model = model;
        self
    }

    pub fn context(mut self, used: usize, total: usize) -> Self {
        self.context_used = used;
        self.context_total = total;
        self
    }

    pub fn status(mut self, status: &'a str) -> Self {
        self.status = status;
        self
    }

    pub fn error(mut self, error: Option<&'a str>) -> Self {
        self.error = error;
        self
    }

    pub fn toast(mut self, text: &'a str, kind: NotificationToastKind) -> Self {
        self.toast = Some((text, kind));
        self
    }

    pub fn counts(mut self, counts: Vec<(NotificationToastKind, usize)>) -> Self {
        self.notification_counts = counts;
        self
    }
}

impl Component for StatusComponent<'_> {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        let error_node = if let Some(err) = self.error {
            styled(
                format!("Error: {}", err),
                ThemeTokens::default_ref().error_style(),
            )
        } else {
            Node::Empty
        };

        let mut status_bar = StatusBar::new()
            .mode(self.mode)
            .model(self.model)
            .context(self.context_used, self.context_total)
            .status(self.status);

        if let Some((text, kind)) = self.toast {
            status_bar = status_bar.toast(text, kind);
        }
        if !self.notification_counts.is_empty() {
            status_bar = status_bar.counts(self.notification_counts.clone());
        }

        col(vec![error_node, status_bar.view(ctx)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crate::tui::oil::render::render_to_plain_text;

    #[test]
    fn status_no_error_shows_bar_only() {
        let mut harness = ComponentHarness::new(80, 4);
        let comp = StatusComponent::new()
            .mode(ChatMode::Normal)
            .model("gpt-4")
            .context(4000, 8000)
            .status("Ready");
        harness.render_component(&comp);
        let plain = render_to_plain_text(&comp.view(&ViewContext::new(harness.focus())), 80);
        assert!(plain.contains("NORMAL"));
        assert!(plain.contains("gpt-4"));
        assert!(plain.contains("50% ctx"));
        assert!(plain.contains("Ready"));
        assert!(!plain.contains("Error:"));
    }

    #[test]
    fn status_with_error_shows_error_above_bar() {
        let mut harness = ComponentHarness::new(80, 4);
        let comp = StatusComponent::new()
            .mode(ChatMode::Normal)
            .model("gpt-4")
            .error(Some("connection failed"));
        harness.render_component(&comp);
        let plain = render_to_plain_text(&comp.view(&ViewContext::new(harness.focus())), 80);
        assert!(plain.contains("Error: connection failed"));
        assert!(plain.contains("NORMAL"));
        assert!(plain.contains("gpt-4"));
    }

    #[test]
    fn status_with_toast_renders_toast() {
        let harness = ComponentHarness::new(80, 4);
        let comp = StatusComponent::new()
            .mode(ChatMode::Auto)
            .model("claude")
            .toast("Processing", NotificationToastKind::Info);
        let plain = render_to_plain_text(&comp.view(&ViewContext::new(harness.focus())), 80);
        assert!(plain.contains("Processing"));
        assert!(plain.contains("INFO"));
        assert!(plain.contains("AUTO"));
    }

    #[test]
    fn status_with_notification_counts() {
        let harness = ComponentHarness::new(80, 4);
        let comp = StatusComponent::new()
            .mode(ChatMode::Plan)
            .model("gpt-4")
            .counts(vec![
                (NotificationToastKind::Warning, 3),
                (NotificationToastKind::Error, 1),
            ]);
        let plain = render_to_plain_text(&comp.view(&ViewContext::new(harness.focus())), 80);
        assert!(plain.contains("PLAN"));
        assert!(plain.contains("WARN"));
        assert!(plain.contains("3"));
        assert!(plain.contains("ERROR"));
        assert!(plain.contains("1"));
    }

    #[test]
    fn error_none_produces_no_error_text() {
        let harness = ComponentHarness::new(80, 4);
        let comp = StatusComponent::new().error(None);
        let plain = render_to_plain_text(&comp.view(&ViewContext::new(harness.focus())), 80);
        assert!(!plain.contains("Error:"));
    }
}
