//! Highlight groups — the open, linkable colour namespace.
//!
//! Colour and geometry are deliberately separate systems. Geometry (borders,
//! padding, sizes) is a **closed** enumerated set of surfaces, because it has to
//! be drawn by a renderer that knows those surfaces. Colour is an **open**
//! namespace, because plugins must be able to name their own, and a theme must
//! be able to retarget many at once with a single link.
//!
//! This mirrors the split Neovim settles on: `hl-Pmenu*` (open, linkable
//! highlight groups) versus `'pumheight'`/`'pumblend'` (closed, enumerated
//! options). Merging the two into one inheritable bag conflates "what colour is
//! this" with "how big is this", and the two have different extensibility needs.
//!
//! A group's colour is a literal (`"cyan"`, `"#282c34"`), an adaptive pair
//! (`{dark=…, light=…}`), or a **palette reference** — the name of a field in
//! the active theme's palette (`"popup_bg"`). Palette references are what keep
//! themes portable: swap the palette and every group that references it follows.

use crate::theme::ThemeConfig;
use crucible_oil::style::{AdaptiveColor, Color};
use std::collections::HashMap;

/// Maximum `link` hops before we call it a cycle.
///
/// A cycle must not hang or blow the stack — styling is not a gate, and a typo
/// in a theme must degrade, not deadlock the renderer.
const MAX_LINK_DEPTH: usize = 32;

/// A colour as written by a theme author, before it meets a palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HlColor {
    /// A literal colour, possibly differing by terminal background.
    Adaptive(AdaptiveColor),
    /// The name of a field in the theme palette, e.g. `"popup_bg"`.
    Palette(String),
}

impl HlColor {
    /// Parse the authoring form. A string that reads as a colour literal is one;
    /// anything else is taken as a palette reference and checked at resolve time
    /// against the active theme.
    ///
    /// The two vocabularies do not overlap — no palette field is named `cyan`,
    /// and no literal is named `popup_bg` — so the order is stable, and trying
    /// literals first pins the colour names permanently.
    pub fn parse(s: &str) -> Self {
        match crate::theme::parse_color_string(s) {
            Some(c) => HlColor::Adaptive(AdaptiveColor::from_single(c)),
            None => HlColor::Palette(s.to_string()),
        }
    }

    /// Resolve against a theme, honouring its dark/light setting.
    ///
    /// An unknown palette name yields `None`, which drops that one attribute
    /// rather than substituting a guess — a wrong colour reads as a bug in the
    /// renderer, whereas a missing one reads as "not styled".
    pub fn resolve(&self, theme: &ThemeConfig) -> Option<Color> {
        match self {
            HlColor::Adaptive(a) => Some(theme.resolve_color(*a)),
            HlColor::Palette(name) => palette_lookup(theme, name).map(|a| theme.resolve_color(a)),
        }
    }
}

/// One highlight group as authored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HlGroup {
    pub fg: Option<HlColor>,
    pub bg: Option<HlColor>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    /// When set, this group defers to another. Attributes set here still win
    /// over the target's, so `link` is a base to override rather than a rename.
    pub link: Option<String>,
}

/// A group with every reference resolved against a concrete theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResolvedHl {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
}

/// The highlight-group table.
pub type HlRegistry = HashMap<String, HlGroup>;

/// Resolve a group by name, following `link` hops.
///
/// Attributes closer to the requested group win, so a group can link for its
/// base and override one attribute. Returns `None` for an unknown group.
pub fn resolve(name: &str, registry: &HlRegistry, theme: &ThemeConfig) -> Option<ResolvedHl> {
    let mut out = ResolvedHl::default();
    let mut seen_fg = false;
    let mut seen_bg = false;
    let mut current = name;
    let mut visited: Vec<&str> = Vec::new();
    let mut found_any = false;

    for _ in 0..MAX_LINK_DEPTH {
        if visited.contains(&current) {
            tracing::warn!(
                group = name,
                "highlight link cycle at '{current}'; using what resolved before the cycle"
            );
            break;
        }
        visited.push(current);

        let Some(group) = registry.get(current) else {
            break;
        };
        found_any = true;

        // Nearest definition wins: only fill an attribute not already set.
        if !seen_fg {
            if let Some(ref c) = group.fg {
                out.fg = c.resolve(theme);
                seen_fg = true;
            }
        }
        if !seen_bg {
            if let Some(ref c) = group.bg {
                out.bg = c.resolve(theme);
                seen_bg = true;
            }
        }
        out.bold |= group.bold;
        out.dim |= group.dim;
        out.italic |= group.italic;
        out.underline |= group.underline;

        match group.link {
            Some(ref target) => current = target,
            None => break,
        }
    }

    found_any.then_some(out)
}

/// Look up a palette field by its authored name.
///
/// The list mirrors `ThemeColors`'s fields exactly; a field added there without
/// a row here is simply not referenceable, which the test below guards against
/// for the fields most likely to be used.
fn palette_lookup(theme: &ThemeConfig, name: &str) -> Option<AdaptiveColor> {
    let c = &theme.colors;
    Some(match name {
        "primary" => c.primary,
        "secondary" => c.secondary,
        "background" => c.background,
        "background_panel" => c.background_panel,
        "command_bg" => c.command_bg,
        "shell_bg" => c.shell_bg,
        "input_bg" => c.input_bg,
        "text" => c.text,
        "text_muted" => c.text_muted,
        "text_dim" => c.text_dim,
        "text_emphasized" => c.text_emphasized,
        "error" => c.error,
        "warning" => c.warning,
        "success" => c.success,
        "info" => c.info,
        "border" => c.border,
        "border_focused" => c.border_focused,
        "border_dim" => c.border_dim,
        "user_message" => c.user_message,
        "assistant_message" => c.assistant_message,
        "system_message" => c.system_message,
        "mode_normal" => c.mode_normal,
        "mode_insert" => c.mode_insert,
        "mode_plan" => c.mode_plan,
        "mode_auto" => c.mode_auto,
        "diff_added" => c.diff_added,
        "diff_removed" => c.diff_removed,
        "diff_added_bg" => c.diff_added_bg,
        "diff_removed_bg" => c.diff_removed_bg,
        "diff_context" => c.diff_context,
        "popup_bg" => c.popup_bg,
        "popup_selected_bg" => c.popup_selected_bg,
        "toast_bg" => c.toast_bg,
        "overlay_text" => c.overlay_text,
        "overlay_bright" => c.overlay_bright,
        "code_inline" => c.code_inline,
        "code_fallback" => c.code_fallback,
        "fence_marker" => c.fence_marker,
        "bullet_prefix" => c.bullet_prefix,
        "blockquote_prefix" => c.blockquote_prefix,
        "blockquote_text" => c.blockquote_text,
        "link" => c.link,
        "heading_1" => c.heading_1,
        "heading_2" => c.heading_2,
        "heading_3" => c.heading_3,
        _ => {
            tracing::warn!("unknown palette colour '{name}'; leaving that attribute unstyled");
            return None;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(pairs: Vec<(&str, HlGroup)>) -> HlRegistry {
        pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    fn group_fg(c: &str) -> HlGroup {
        HlGroup {
            fg: Some(HlColor::parse(c)),
            ..Default::default()
        }
    }

    #[test]
    fn literal_colour_names_parse_as_literals_not_palette_refs() {
        assert_eq!(
            HlColor::parse("cyan"),
            HlColor::Adaptive(AdaptiveColor::from_single(Color::Cyan))
        );
        assert_eq!(
            HlColor::parse("#282c34"),
            HlColor::Adaptive(AdaptiveColor::from_single(Color::Rgb(0x28, 0x2c, 0x34)))
        );
    }

    #[test]
    fn unrecognised_names_are_treated_as_palette_references() {
        assert_eq!(
            HlColor::parse("popup_bg"),
            HlColor::Palette("popup_bg".to_string())
        );
    }

    #[test]
    fn palette_references_resolve_through_the_active_theme() {
        let theme = ThemeConfig::default_dark();
        let registry = reg(vec![("Popup", group_fg("popup_bg"))]);

        let resolved = resolve("Popup", &registry, &theme).expect("group exists");
        assert_eq!(
            resolved.fg,
            Some(theme.resolve_color(theme.colors.popup_bg))
        );
    }

    /// The portability property: retarget the palette, and every group that
    /// references it follows without being rewritten.
    #[test]
    fn changing_the_palette_changes_every_group_that_references_it() {
        let registry = reg(vec![("Popup", group_fg("popup_bg"))]);

        let mut theme = ThemeConfig::default_dark();
        theme.colors.popup_bg = AdaptiveColor::from_single(Color::Magenta);

        assert_eq!(
            resolve("Popup", &registry, &theme).unwrap().fg,
            Some(Color::Magenta)
        );
    }

    #[test]
    fn an_unknown_palette_name_drops_only_that_attribute() {
        let theme = ThemeConfig::default_dark();
        let registry = reg(vec![(
            "Broken",
            HlGroup {
                fg: Some(HlColor::parse("chartreuse")),
                bg: Some(HlColor::parse("cyan")),
                bold: true,
                ..Default::default()
            },
        )]);

        let resolved = resolve("Broken", &registry, &theme).unwrap();
        assert_eq!(resolved.fg, None, "unknown palette name yields no colour");
        assert_eq!(resolved.bg, Some(Color::Cyan), "the rest still applies");
        assert!(resolved.bold);
    }

    #[test]
    fn a_link_inherits_the_targets_attributes() {
        let theme = ThemeConfig::default_dark();
        let registry = reg(vec![
            ("Visual", {
                let mut g = group_fg("cyan");
                g.bold = true;
                g
            }),
            (
                "PopupSelected",
                HlGroup {
                    link: Some("Visual".to_string()),
                    ..Default::default()
                },
            ),
        ]);

        let resolved = resolve("PopupSelected", &registry, &theme).unwrap();
        assert_eq!(resolved.fg, Some(Color::Cyan));
        assert!(resolved.bold);
    }

    /// `link` is a base to override, not a rename — otherwise a theme could not
    /// say "like Visual, but red".
    #[test]
    fn attributes_on_the_linking_group_beat_the_target() {
        let theme = ThemeConfig::default_dark();
        let registry = reg(vec![
            ("Visual", group_fg("cyan")),
            ("PopupSelected", {
                let mut g = group_fg("red");
                g.link = Some("Visual".to_string());
                g
            }),
        ]);

        assert_eq!(
            resolve("PopupSelected", &registry, &theme).unwrap().fg,
            Some(Color::Red)
        );
    }

    /// A typo must degrade, never hang the renderer.
    #[test]
    fn a_link_cycle_terminates_instead_of_hanging() {
        let theme = ThemeConfig::default_dark();
        let registry = reg(vec![
            ("A", {
                let mut g = group_fg("cyan");
                g.link = Some("B".to_string());
                g
            }),
            (
                "B",
                HlGroup {
                    link: Some("A".to_string()),
                    ..Default::default()
                },
            ),
        ]);

        let resolved = resolve("A", &registry, &theme).expect("resolves despite the cycle");
        assert_eq!(resolved.fg, Some(Color::Cyan));
    }

    #[test]
    fn a_link_to_a_missing_group_keeps_what_resolved() {
        let theme = ThemeConfig::default_dark();
        let registry = reg(vec![("A", {
            let mut g = group_fg("cyan");
            g.link = Some("DoesNotExist".to_string());
            g
        })]);

        assert_eq!(resolve("A", &registry, &theme).unwrap().fg, Some(Color::Cyan));
    }

    #[test]
    fn an_unknown_group_resolves_to_nothing() {
        let theme = ThemeConfig::default_dark();
        assert_eq!(resolve("Nope", &HlRegistry::new(), &theme), None);
    }

    #[test]
    fn adaptive_colours_follow_the_themes_background() {
        let registry = reg(vec![(
            "Text",
            HlGroup {
                fg: Some(HlColor::Adaptive(AdaptiveColor {
                    dark: Color::White,
                    light: Color::Black,
                })),
                ..Default::default()
            },
        )]);

        let mut theme = ThemeConfig::default_dark();
        assert_eq!(
            resolve("Text", &registry, &theme).unwrap().fg,
            Some(Color::White)
        );

        theme.is_dark = false;
        assert_eq!(
            resolve("Text", &registry, &theme).unwrap().fg,
            Some(Color::Black)
        );
    }
}
