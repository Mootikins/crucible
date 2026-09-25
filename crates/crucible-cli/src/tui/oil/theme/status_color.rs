use crucible_core::status_color::StatusColorGroup;
use crucible_lua::theme::ThemeConfig;
use crucible_oil::style::Color;

/// Resolve a named status group through the active terminal palette.
pub fn color(group: StatusColorGroup, theme: &ThemeConfig) -> Color {
    use StatusColorGroup as Group;
    if theme.name == "ansi16" {
        // 0-15 address the emulator's own palette, including custom schemes.
        return Color::Indexed(match group {
            Group::Ok => 2,
            Group::Warn => 3,
            Group::Danger => 1,
            Group::Info => 6,
            Group::Hue0 => 1,
            Group::Hue1 => 3,
            Group::Hue2 => 2,
            Group::Hue3 => 6,
            Group::Hue4 => 4,
            Group::Hue5 => 5,
            Group::Hue6 => 9,
            Group::Hue7 => 12,
        });
    }

    match group {
        Group::Ok => theme.resolve_color(theme.colors.success),
        Group::Warn => theme.resolve_color(theme.colors.warning),
        Group::Danger => theme.resolve_color(theme.colors.error),
        Group::Info => theme.resolve_color(theme.colors.info),
        Group::Hue0 => Color::Rgb(221, 122, 118),
        Group::Hue1 => Color::Rgb(203, 145, 71),
        Group::Hue2 => Color::Rgb(205, 183, 95),
        Group::Hue3 => Color::Rgb(143, 196, 127),
        Group::Hue4 => Color::Rgb(107, 191, 187),
        Group::Hue5 => Color::Rgb(127, 167, 224),
        Group::Hue6 => Color::Rgb(174, 144, 214),
        Group::Hue7 => Color::Rgb(221, 133, 176),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::status_color::StatusColorGroup;
    use crucible_oil::style::Color;

    #[test]
    fn default_status_groups_are_colored_and_unknown_falls_back_to_info() {
        let theme = crate::tui::oil::theme::ThemeConfig::default_dark();
        for group in StatusColorGroup::all() {
            assert_ne!(color(group, &theme), Color::Reset);
        }
        assert_eq!(
            color(StatusColorGroup::from_name("bogus"), &theme),
            color(StatusColorGroup::Info, &theme)
        );
    }

    #[test]
    fn ansi16_status_groups_use_only_terminal_palette_slots() {
        let theme = crucible_lua::theme::load_theme_from_lua(include_str!(
            "../../../../../../runtime/themes/ansi16.luau"
        ))
        .expect("bundled ANSI16 theme loads");
        assert_eq!(theme.name, "ansi16");
        let colors: std::collections::HashSet<_> = StatusColorGroup::all()
            .map(|group| color(group, &theme).palette_index().unwrap())
            .collect();
        assert!(colors.iter().all(|slot| *slot < 16));
        assert_eq!(colors.len(), 8);
    }
}
