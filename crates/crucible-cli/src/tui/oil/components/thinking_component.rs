//! Thinking block component.
//!
//! Owns state for a single thinking block and renders it based on
//! whether it's live (streaming) or graduated (scrollback).

use std::borrow::Cow;

use crate::tui::oil::markdown::{markdown_to_node_styled, Margins, RenderStyle};
use crate::tui::oil::node::{col, row, styled, Node};
use crate::tui::oil::render_state::RenderState;
use crate::tui::oil::style::Style;

/// A thinking block that owns its state and rendering.
///
/// State transitions:
/// - Live: streaming tokens, renders full or collapsed based on `show_thinking`
/// - Graduated: always renders collapsed summary (no spinner)
#[derive(Debug, Clone)]
pub struct ThinkingComponent {
    pub(crate) content: String,
    pub(crate) token_count: usize,
    graduated: bool,
}

impl ThinkingComponent {
    pub fn new(content: String, token_count: usize) -> Self {
        Self {
            content,
            token_count,
            graduated: false,
        }
    }

    /// Append streaming thinking content.
    pub fn append(&mut self, delta: &str) {
        self.content.push_str(delta);
        self.token_count += 1;
    }

    /// Replace content wholesale (used by set_thinking).
    pub fn replace(&mut self, content: String, token_count: usize) {
        self.content = content;
        self.token_count = token_count;
    }

    /// Transition to graduated state. Render will always produce
    /// collapsed output after this call.
    pub fn graduate(&mut self) {
        self.graduated = true;
    }

    pub fn is_graduated(&self) -> bool {
        self.graduated
    }

    /// Word count across all content.
    pub fn word_count(&self) -> usize {
        self.content.split_whitespace().count()
    }

    /// Render this thinking block.
    ///
    /// - Graduated: always collapsed summary, no spinner
    /// - Live + show_thinking: full expanded content
    /// - Live + !show_thinking: collapsed summary with spinner
    pub fn render(&self, state: &RenderState, is_complete: bool) -> Node {
        if self.graduated {
            self.render_collapsed_complete()
        } else if state.show_thinking {
            self.render_expanded(state, is_complete)
        } else {
            self.render_collapsed_live(state, is_complete)
        }
    }

    /// Dim + muted style pair used by collapsed renderings.
    fn thinking_styles() -> (Style, Style) {
        let t = crate::tui::oil::theme::active();
        let dim = Style::new().fg(t.resolve_color(t.colors.text_dim));
        let muted = Style::new()
            .fg(t.resolve_color(t.colors.text_muted))
            .italic();
        (dim, muted)
    }

    /// Graduated or complete: "◇ Thought (N words)"
    fn render_collapsed_complete(&self) -> Node {
        let (dim, muted) = Self::thinking_styles();
        let words = self.word_count();
        row([
            styled(" \u{25C7} ", dim),
            styled("Thought", dim),
            styled(format!(" ({} words)", words), muted),
        ])
    }

    /// Full thinking content with header (show_thinking=true, not graduated).
    fn render_expanded(&self, state: &RenderState, is_complete: bool) -> Node {
        let t = crate::tui::oil::theme::active();
        let words = self.word_count();

        let label = if is_complete {
            format!(
                "  \u{250C}{} Thought ({} words)",
                t.decorations.divider_char, words
            )
        } else {
            format!("  \u{250C}{} Thinking\u{2026}", t.decorations.divider_char)
        };
        let header = styled(
            label,
            Style::new()
                .fg(t.resolve_color(t.colors.text_muted))
                .italic(),
        );

        // Truncate long content to last ~1200 chars for display
        let display_content: Cow<'_, str> = if self.content.len() > 1200 {
            let char_count = self.content.chars().count();
            let start = if char_count > 1200 {
                self.content
                    .char_indices()
                    .nth(char_count - 1200)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            } else {
                0
            };
            let boundary = self.content[start..]
                .find(char::is_whitespace)
                .map(|i| start + i + 1)
                .unwrap_or(start);
            Cow::Owned(format!("\u{2026}{}", &self.content[boundary..]))
        } else {
            Cow::Borrowed(&self.content)
        };

        let md_style = RenderStyle::viewport_with_margins(
            state.width().saturating_sub(4),
            Margins {
                left: 4,
                right: 0,
                show_bullet: false,
            },
        );
        let content_node = markdown_to_node_styled(&display_content, md_style);

        col([header, content_node])
    }

    /// Collapsed summary while live (show_thinking=false, not graduated).
    ///
    /// No spinners — spinners are viewport chrome only (prevents scrollback
    /// leaks). The turn spinner covers all spinner display.
    fn render_collapsed_live(&self, _state: &RenderState, is_complete: bool) -> Node {
        let (_, muted) = Self::thinking_styles();
        let words = self.word_count();

        if !is_complete && words == 0 {
            // Just started thinking, no words yet — show label only
            styled(" Thinking\u{2026}", muted)
        } else if is_complete {
            // Response finished but component wasn't graduated yet (viewport render).
            self.render_collapsed_complete()
        } else {
            // Still thinking, accumulating words
            styled(format!(" Thinking\u{2026} ({} words)", words), muted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::render::render_to_plain_text;

    fn default_state() -> RenderState {
        RenderState {
            terminal_width: 80,
            spinner_frame: 0,
            show_thinking: false,
        }
    }

    #[test]
    fn new_component_is_not_graduated() {
        let tc = ThinkingComponent::new("hello".into(), 1);
        assert!(!tc.is_graduated());
        assert_eq!(tc.word_count(), 1);
    }

    #[test]
    fn append_accumulates_content() {
        let mut tc = ThinkingComponent::new("hello".into(), 1);
        tc.append(" world");
        assert_eq!(tc.content, "hello world");
        assert_eq!(tc.token_count, 2);
        assert_eq!(tc.word_count(), 2);
    }

    #[test]
    fn replace_overwrites_content() {
        let mut tc = ThinkingComponent::new("old".into(), 1);
        tc.replace("new content here".into(), 50);
        assert_eq!(tc.content, "new content here");
        assert_eq!(tc.token_count, 50);
    }

    #[test]
    fn graduate_transitions_state() {
        let mut tc = ThinkingComponent::new("thinking about things".into(), 10);
        assert!(!tc.is_graduated());
        tc.graduate();
        assert!(tc.is_graduated());
    }

    #[test]
    fn graduated_renders_collapsed() {
        let mut tc = ThinkingComponent::new("thinking about many things here".into(), 10);
        tc.graduate();

        let state = default_state();
        let node = tc.render(&state, false);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thought"));
        assert!(plain.contains("words)"));
        assert!(!plain.contains("Thinking"));
    }

    #[test]
    fn live_collapsed_no_words_shows_spinner_without_count() {
        let tc = ThinkingComponent::new(String::new(), 0);
        let state = default_state();
        let node = tc.render(&state, false);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thinking"));
        assert!(
            !plain.contains("words"),
            "Zero-word spinner should not show word count, got: {plain}"
        );
    }

    #[test]
    fn live_collapsed_with_words_shows_count() {
        let tc = ThinkingComponent::new("one two three".into(), 3);
        let state = default_state();
        let node = tc.render(&state, false);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thinking"));
        assert!(plain.contains("3 words"));
    }

    #[test]
    fn live_expanded_shows_full_content() {
        let tc = ThinkingComponent::new("detailed reasoning here".into(), 3);
        let mut state = default_state();
        state.show_thinking = true;
        let node = tc.render(&state, false);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("detailed reasoning here"));
    }

    #[test]
    fn complete_collapsed_shows_thought() {
        let tc = ThinkingComponent::new("some analysis".into(), 5);
        let state = default_state();
        let node = tc.render(&state, true);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thought"));
        assert!(plain.contains("words)"));
    }

    // --- UTF-8 boundary tests (moved from message_list.rs) ---

    #[test]
    fn expanded_boundary_1200_chars() {
        let tc = ThinkingComponent::new("a".repeat(1200), 100);
        let mut state = default_state();
        state.show_thinking = true;
        let node = tc.render(&state, true);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thought"));
        assert!(!plain.contains("\u{2026}"));
    }

    #[test]
    fn expanded_over_1200_chars_truncates() {
        let tc = ThinkingComponent::new("a".repeat(1201), 100);
        let mut state = default_state();
        state.show_thinking = true;
        let node = tc.render(&state, true);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thought"));
        assert!(plain.contains("\u{2026}"));
    }

    #[test]
    fn expanded_cjk_does_not_panic() {
        let content = "\u{4F60}\u{597D}\u{4E16}\u{754C}".repeat(125);
        assert!(content.len() > 1200);
        let tc = ThinkingComponent::new(content, 100);
        let mut state = default_state();
        state.show_thinking = true;
        let node = tc.render(&state, true);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thought"));
        assert!(plain.contains("\u{2026}"));
    }

    #[test]
    fn expanded_emoji_boundary() {
        let content = "\u{1F525}\u{1F30A}\u{26A1}".repeat(200);
        assert!(content.len() > 1200);
        let tc = ThinkingComponent::new(content, 100);
        let mut state = default_state();
        state.show_thinking = true;
        let node = tc.render(&state, true);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thought"));
        assert!(plain.contains("\u{2026}"));
    }

    #[test]
    fn expanded_mixed_utf8() {
        let content = "Hello \u{4F60}\u{597D} \u{1F525} ".repeat(200);
        assert!(content.len() > 1200);
        let tc = ThinkingComponent::new(content, 100);
        let mut state = default_state();
        state.show_thinking = true;
        let node = tc.render(&state, true);
        let plain = render_to_plain_text(&node, 80);
        assert!(plain.contains("Thought"));
        assert!(plain.contains("\u{2026}"));
    }
}
