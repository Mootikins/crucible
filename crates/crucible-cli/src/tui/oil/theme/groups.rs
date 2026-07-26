//! Process-wide highlight-group table for the TUI.
//!
//! Groups arrive from `ui.config` alongside the theme and are stored
//! **unresolved**: a group whose colour is a palette reference re-resolves
//! against whatever theme is active, so the two stay coherent.
//!
//! Components ask for a group by name and fall back to their built-in style when
//! it is absent, which is what lets a theme restyle a surface without every
//! component having to know the theme exists.

use crucible_lua::hl::{HlRegistry, ResolvedHl};
use std::sync::OnceLock;

static GROUPS: OnceLock<HlRegistry> = OnceLock::new();

/// Install the highlight table. Set-once, mirroring the active theme, so a late
/// snapshot cannot restyle a half-drawn screen.
pub fn set(registry: HlRegistry) {
    let _ = GROUPS.set(registry);
}

/// The active highlight table; empty when none was delivered.
pub fn active() -> &'static HlRegistry {
    GROUPS.get_or_init(HlRegistry::new)
}

/// Resolve a group against the active theme.
///
/// `None` means "not themed" — the caller keeps its built-in style. This is the
/// difference between a surface a theme chose not to touch and one it broke.
pub fn get(name: &str) -> Option<ResolvedHl> {
    crucible_lua::hl::resolve(name, active(), super::active())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_lua::hl::{HlColor, HlGroup};
    use crucible_oil::style::Color;

    // As with the theme global, these rely on cargo-nextest's process-per-test.

    #[test]
    fn an_absent_group_is_none_so_callers_keep_their_own_style() {
        assert_eq!(get("NoSuchGroup"), None);
    }

    #[test]
    fn a_delivered_group_resolves_against_the_active_theme() {
        let mut registry = HlRegistry::new();
        registry.insert(
            "StatusMode".to_string(),
            HlGroup {
                fg: Some(HlColor::parse("black")),
                bg: Some(HlColor::parse("mode_normal")),
                bold: true,
                ..Default::default()
            },
        );
        set(registry);

        let resolved = get("StatusMode").expect("group resolves");
        assert_eq!(resolved.fg, Some(Color::Black));
        assert!(resolved.bold);

        let theme = super::super::active();
        assert_eq!(
            resolved.bg,
            Some(theme.resolve_color(theme.colors.mode_normal)),
            "palette references resolve through the active theme, not at author time"
        );
    }
}
