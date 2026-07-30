use crate::tui::oil::component::Component;
use crate::tui::oil::components::status_bar::{NotificationToastKind, StatusBar};
use crate::tui::oil::ViewContext;
use crucible_lua::statusline_items::Region;
use crucible_oil::node::{col, Node};

/// View-only projection of the prompt region.
///
/// All state is owned by `OilChatApp`; this struct borrows snapshots of it.
#[derive(Default)]
pub struct StatusComponent<'a> {
    /// Mode id, not an enum: see `chat_app::state::DEFAULT_MODE`. Defaults to
    /// the empty string, which `mode_label` renders as a blank badge — every
    /// live construction goes through `.mode(...)`.
    pub mode: &'a str,
    pub model: &'a str,
    pub context_used: usize,
    pub context_total: usize,
    pub status: &'a str,
    pub toast: Option<(&'a str, NotificationToastKind)>,
    pub notification_counts: Vec<(NotificationToastKind, usize)>,
    /// Latest prompt-cache hit rate (0.0..=1.0), or `None` until a
    /// `message_complete` event has reported cache token counts.
    pub cache_hit_rate: Option<f64>,
    /// Whether a turn is streaming — readable by `sl.when("streaming", ...)`.
    pub streaming: bool,
}

impl<'a> StatusComponent<'a> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn mode(mut self, mode: &'a str) -> Self {
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

    pub fn toast(mut self, text: &'a str, kind: NotificationToastKind) -> Self {
        self.toast = Some((text, kind));
        self
    }

    pub fn counts(mut self, counts: Vec<(NotificationToastKind, usize)>) -> Self {
        self.notification_counts = counts;
        self
    }

    pub fn cache_hit_rate(mut self, rate: Option<f64>) -> Self {
        self.cache_hit_rate = rate;
        self
    }

    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// Render one region from this frame's values.
    ///
    /// Every region reads the snapshot this component owns, so rows in
    /// different regions cannot disagree within a frame.
    pub fn render_region(&self, region: Region, input_node: impl FnMut() -> Node) -> Vec<Node> {
        self.bar().render_region(region, self.streaming, input_node)
    }

    fn bar(&self) -> StatusBar {
        let mut bar = StatusBar::new()
            .mode(self.mode)
            .model(self.model)
            .context(self.context_used, self.context_total)
            .status(self.status);
        bar.cache_hit_rate = self.cache_hit_rate;
        if let Some((text, kind)) = self.toast {
            bar = bar.toast(text, kind);
        }
        if !self.notification_counts.is_empty() {
            bar = bar.counts(self.notification_counts.clone());
        }
        bar
    }
}

impl Component for StatusComponent<'_> {
    /// The prompt region with the input elided — the status rows on their own.
    ///
    /// Used where the bars are wanted without an editor: component tests, and
    /// any surface that shows status without accepting input.
    fn view(&self, _ctx: &ViewContext<'_>) -> Node {
        col(self
            .bar()
            .render_region(Region::Prompt, self.streaming, || Node::Empty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crucible_oil::render::render_to_plain_text;

    #[test]
    fn status_no_error_shows_bar_only() {
        let mut harness = ComponentHarness::new(80, 4);
        let comp = StatusComponent::new()
            .mode("normal")
            .model("gpt-4")
            .context(4000, 8000);
        harness.render_component(&comp);
        let plain = render_to_plain_text(&comp.view(&ViewContext::new(harness.focus())), 80);
        assert!(plain.contains("NORMAL"));
        assert!(plain.contains("gpt-4"));
        assert!(plain.contains("50% ctx"));
        assert!(!plain.contains("Error:"));
    }

    #[test]
    fn status_with_toast_renders_toast() {
        let harness = ComponentHarness::new(80, 4);
        let comp = StatusComponent::new()
            .mode("auto")
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
            .mode("plan")
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
}
