use crucible_oil::node::{row, spinner, styled, text, Node};
use crucible_oil::style::Style;

use crate::tui::oil::app::ViewContext;
use crate::tui::oil::component::Component;

/// Turn-level activity indicator. Shows spinner + optional thinking word count.
#[derive(Default)]
pub struct TurnIndicator {
    pub active: bool,
    pub thinking_words: Option<usize>,
}

impl TurnIndicator {
    pub fn new() -> Self {
        Self {
            active: false,
            thinking_words: None,
        }
    }
}

impl Component for TurnIndicator {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        if !self.active {
            return Node::Empty;
        }
        let t = crate::tui::oil::theme::active();
        let spinner_style = Style::new().fg(t.resolve_color(t.colors.text));
        let muted = Style::new()
            .fg(t.resolve_color(t.colors.text_muted))
            .italic();

        match self.thinking_words {
            Some(words) if words > 0 => row([
                text(" "),
                spinner(None, ctx.spinner_frame).with_style(spinner_style),
                styled(format!(" Thinking\u{2026} ({words} words)"), muted),
            ]),
            _ => row([
                text(" "),
                spinner(None, ctx.spinner_frame).with_style(spinner_style),
            ]),
        }
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
        let ti = TurnIndicator {
            active: true,
            thinking_words: None,
        };
        let ctx = test_ctx(0);
        let node = ti.view(&ctx);
        let plain = render_to_plain_text(&node, 80);
        assert!(!plain.trim().is_empty());
    }

    #[test]
    fn active_with_thinking_words_shows_count() {
        let ti = TurnIndicator {
            active: true,
            thinking_words: Some(42),
        };
        let ctx = test_ctx(3);
        let node = ti.view(&ctx);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("42 words"), "should show word count");
        assert!(plain.contains("Thinking"), "should show thinking label");
    }
}
