use crate::tui::oil::theme;
use crucible_oil::style::Color;
use crucible_oil::InputStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    Normal,
    Command,
    Shell,
}

impl InputMode {
    pub fn bg_color(&self) -> Color {
        let t = theme::active();
        let (group, fallback) = match self {
            InputMode::Normal => ("PromptNormal", t.colors.input_bg),
            InputMode::Command => ("PromptCommand", t.colors.command_bg),
            InputMode::Shell => ("PromptShell", t.colors.shell_bg),
        };
        theme::groups::bg_or(group, t.resolve_color(fallback))
    }

    /// The prompt glyph, themed if `crucible.ui.setup{ prompt = ... }` set one.
    ///
    /// Stays `&'static str`: the geometry store leaks on install, so a themed
    /// glyph borrows from it for the life of the process — no allocation per
    /// frame, and no signature change rippling through `InputStyle`.
    pub fn prompt(&self) -> &'static str {
        let themed = &theme::geometry::active().prompt;
        let override_glyph = match self {
            InputMode::Normal => themed.normal.as_deref(),
            InputMode::Command => themed.command.as_deref(),
            InputMode::Shell => themed.shell.as_deref(),
        };
        override_glyph.unwrap_or(match self {
            InputMode::Normal => " > ",
            InputMode::Command => " : ",
            InputMode::Shell => " ! ",
        })
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

#[cfg(test)]
mod tests {
    use super::*;

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

    /// A theme that names a surface group must reach the component that draws
    /// it — the whole point of groups being open rather than a fixed struct.
    #[test]
    fn a_prompt_group_overrides_the_surface_background() {
        use crucible_lua::hl::{HlColor, HlGroup, HlRegistry};

        let untouched = InputMode::Shell.bg_color();

        let mut registry = HlRegistry::new();
        registry.insert(
            "PromptNormal".to_string(),
            HlGroup {
                bg: Some(HlColor::parse("magenta")),
                ..Default::default()
            },
        );
        theme::groups::set(registry);

        assert_eq!(InputMode::Normal.bg_color(), Color::Magenta);
        assert_eq!(
            InputMode::Shell.bg_color(),
            untouched,
            "a mode the theme did not name keeps its palette colour"
        );
    }
}
