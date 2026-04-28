use crucible_oil::node::{row, spinner, text, Node};
use crucible_oil::style::Style;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;

/// Turn-level activity indicator. A bare spinner — the active reasoning
/// content (and its word count) is rendered inline by the in-progress
/// `AssistantResponse`, so duplicating it here would just confuse the eye.
#[derive(Default)]
pub struct TurnIndicator {
    pub active: bool,
}

impl TurnIndicator {
    pub fn new() -> Self {
        Self { active: false }
    }
}

impl Component for TurnIndicator {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        if !self.active {
            return Node::Empty;
        }
        let t = crate::tui::oil::theme::active();
        let spinner_style = Style::new().fg(t.resolve_color(t.colors.text));
        row([
            text(" "),
            spinner(None, ctx.spinner_frame).with_style(spinner_style),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::focus::FocusContext;
    use crucible_oil::render::render_to_plain_text;

    fn test_ctx(spinner_frame: usize) -> ViewContext<'static> {
        use std::sync::LazyLock;
        static FOCUS: LazyLock<FocusContext> = LazyLock::new(FocusContext::new);
        let mut ctx = ViewContext::new(&FOCUS);
        ctx.spinner_frame = spinner_frame;
        ctx
    }

    #[test]
    fn inactive_returns_empty() {
        let ti = TurnIndicator::new();
        let ctx = test_ctx(0);
        assert!(matches!(ti.view(&ctx), Node::Empty));
    }

    #[test]
    fn active_shows_spinner() {
        let ti = TurnIndicator { active: true };
        let ctx = test_ctx(0);
        let node = ti.view(&ctx);
        let plain = render_to_plain_text(&node, 80);
        assert!(!plain.trim().is_empty());
    }

    #[test]
    fn active_does_not_render_thinking_label() {
        // Thinking content is rendered inline by the AssistantResponse; the
        // turn indicator stays minimal (spinner only) to avoid duplication.
        let ti = TurnIndicator { active: true };
        let ctx = test_ctx(3);
        let node = ti.view(&ctx);
        let plain = render_to_plain_text(&node, 80);
        assert!(!plain.contains("Thinking"));
        assert!(!plain.contains("words"));
    }
}
