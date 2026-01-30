use crate::tui::oil::component::Component;
use crate::tui::oil::node::Node;
use crate::tui::oil::style::Color;
use crate::tui::oil::theme::ThemeTokens;
use crate::tui::oil::ViewContext;
use crucible_oil::InputStyle;

// Re-export Oil's InputArea and related items
pub use crucible_oil::{
    clamp_input_lines, wrap_content, InputArea as OilInputArea, INPUT_MAX_CONTENT_LINES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Command,
    Shell,
}

impl InputMode {
    pub fn bg_color(&self) -> Color {
        let theme = ThemeTokens::default_ref();
        match self {
            InputMode::Normal => theme.input_bg,
            InputMode::Command => theme.command_bg,
            InputMode::Shell => theme.shell_bg,
        }
    }

    pub fn prompt(&self) -> &'static str {
        match self {
            InputMode::Normal => " > ",
            InputMode::Command => " : ",
            InputMode::Shell => " ! ",
        }
    }

    pub fn from_content(content: &str) -> Self {
        if content.starts_with(':') {
            InputMode::Command
        } else if content.starts_with('!') {
            InputMode::Shell
        } else {
            InputMode::Normal
        }
    }
}

impl InputStyle for InputMode {
    fn bg_color(&self) -> Color {
        self.bg_color()
    }

    fn prompt(&self) -> &'static str {
        self.prompt()
    }

    fn display_content<'a>(&self, content: &'a str) -> &'a str {
        match self {
            InputMode::Command => content.strip_prefix(':').unwrap_or(content),
            InputMode::Shell => content.strip_prefix('!').unwrap_or(content),
            InputMode::Normal => content,
        }
    }

    fn display_cursor(&self, cursor: usize) -> usize {
        let offset = if matches!(self, InputMode::Command | InputMode::Shell) {
            1
        } else {
            0
        };
        cursor.saturating_sub(offset)
    }
}

/// CLI wrapper around Oil's InputArea that implements Component trait
#[derive(Debug, Clone)]
pub struct InputArea {
    inner: OilInputArea,
}

impl InputArea {
    pub fn new(content: impl Into<String>, cursor: usize, width: usize) -> Self {
        Self {
            inner: OilInputArea::new(content, cursor, width),
        }
    }

    pub fn set_focused(mut self, focused: bool) -> Self {
        self.inner = self.inner.focused(focused);
        self
    }

    pub fn with_popup(mut self, show_popup: bool) -> Self {
        self.inner = self.inner.with_popup(show_popup);
        self
    }

    pub fn content(&self) -> &str {
        &self.inner.content
    }

    pub fn cursor(&self) -> usize {
        self.inner.cursor
    }

    pub fn width(&self) -> usize {
        self.inner.width
    }

    pub fn is_focused(&self) -> bool {
        self.inner.focused
    }

    pub fn show_popup(&self) -> bool {
        self.inner.show_popup
    }
}

impl Component for InputArea {
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        let mode = InputMode::from_content(&self.inner.content);
        self.inner.view(&mode, ctx.focus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::component::ComponentHarness;
    use crate::tui::oil::render::render_to_plain_text;

    #[test]
    fn input_mode_detection() {
        assert_eq!(InputMode::from_content("hello"), InputMode::Normal);
        assert_eq!(InputMode::from_content(":set model"), InputMode::Command);
        assert_eq!(InputMode::from_content("!ls -la"), InputMode::Shell);
    }

    #[test]
    fn input_modes_have_different_colors() {
        assert_ne!(InputMode::Normal.bg_color(), InputMode::Command.bg_color());
        assert_ne!(InputMode::Command.bg_color(), InputMode::Shell.bg_color());
    }

    #[test]
    fn input_area_renders_prompt() {
        let input = InputArea::new("hello", 5, 80);
        let mut h = ComponentHarness::new(80, 5);
        h.render_component(&input);
        let plain = render_to_plain_text(
            &input.view(&crate::tui::oil::ViewContext::new(h.focus())),
            80,
        );
        assert!(plain.contains(">"));
        assert!(plain.contains("hello"));
    }

    #[test]
    fn command_mode_shows_colon_prompt() {
        let input = InputArea::new(":set model gpt-4", 16, 80);
        let mut h = ComponentHarness::new(80, 5);
        h.render_component(&input);
        let plain = render_to_plain_text(
            &input.view(&crate::tui::oil::ViewContext::new(h.focus())),
            80,
        );
        assert!(plain.contains(":"));
        assert!(plain.contains("set model"));
    }

    #[test]
    fn shell_mode_shows_bang_prompt() {
        let input = InputArea::new("!ls -la", 7, 80);
        let mut h = ComponentHarness::new(80, 5);
        h.render_component(&input);
        let plain = render_to_plain_text(
            &input.view(&crate::tui::oil::ViewContext::new(h.focus())),
            80,
        );
        assert!(plain.contains("!"));
        assert!(plain.contains("ls -la"));
    }
}
