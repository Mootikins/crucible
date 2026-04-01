use crucible_oil::node::{row, spinner, styled, text, Node};
use crucible_oil::style::Style;

/// Turn-level activity indicator. Lives in chrome, never in scrollable content.
/// Shows spinner + optional thinking word count while turn is active.
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

    pub fn view(&self, spinner_frame: usize) -> Node {
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
                spinner(None, spinner_frame).with_style(spinner_style),
                styled(format!(" Thinking\u{2026} ({words} words)"), muted),
            ]),
            _ => row([
                text(" "),
                spinner(None, spinner_frame).with_style(spinner_style),
            ]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_oil::render::render_to_plain_text;

    #[test]
    fn inactive_returns_empty() {
        let ti = TurnIndicator::new();
        assert!(matches!(ti.view(0), Node::Empty));
    }

    #[test]
    fn active_shows_spinner() {
        let ti = TurnIndicator {
            active: true,
            thinking_words: None,
        };
        let node = ti.view(0);
        let plain = render_to_plain_text(&node, 80);
        // Spinner renders a braille char; just confirm non-empty
        assert!(!plain.trim().is_empty());
    }

    #[test]
    fn active_with_thinking_words_shows_count() {
        let ti = TurnIndicator {
            active: true,
            thinking_words: Some(42),
        };
        let node = ti.view(3);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("42 words"), "should show word count");
        assert!(plain.contains("Thinking"), "should show thinking label");
    }
}
