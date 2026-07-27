//! Deriving a syntect theme from the UI colour palette.
//!
//! Code blocks sit inside a themed chat view, so a syntax theme picked
//! independently of the palette clashes with everything around it. This builds
//! one from the active colours instead, mapping semantic slots to the scopes
//! every TextMate grammar emits.
//!
//! ## Carrying terminal colours through syntect
//!
//! syntect's colour type is RGB-only — it has no way to say "terminal palette
//! entry 4". A palette written against terminal colours would therefore be
//! flattened to fixed RGB the moment it reached a code block, and code would
//! stop following the user's terminal theme while the rest of the TUI followed
//! it.
//!
//! syntect never inspects or blends the alpha channel (`Style::apply` replaces
//! colours wholesale), so alpha survives a highlight pass untouched. Palette
//! entries are therefore encoded as `Color { r: index, a: PALETTE_MARKER }` and
//! decoded on the way out. Real themes are fully opaque, so the marker cannot
//! collide with a genuine colour.

use crucible_oil::style::Color as OilColor;
use syntect::highlighting::{
    Color as SynColor, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};

/// Alpha value marking an encoded palette index rather than an RGB colour.
///
/// Themes on disk are opaque (`a = 255`); nothing legitimate is `1`.
const PALETTE_MARKER: u8 = 1;

/// Encode an oil colour for syntect, preserving palette entries.
fn to_syntect(color: OilColor) -> SynColor {
    match color {
        OilColor::Rgb(r, g, b) => SynColor { r, g, b, a: 255 },
        other => match other.palette_index() {
            Some(index) => SynColor {
                r: index,
                g: 0,
                b: 0,
                a: PALETTE_MARKER,
            },
            // `Reset` has no representation syntect understands; fall back to
            // the terminal's default foreground rather than inventing a colour.
            None => SynColor {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
        },
    }
}

/// Decode a syntect colour back, recovering palette entries.
pub fn from_syntect(color: SynColor) -> OilColor {
    match color.a {
        PALETTE_MARKER => OilColor::Indexed(color.r),
        0 => OilColor::Reset,
        _ => OilColor::Rgb(color.r, color.g, color.b),
    }
}

/// One derived scope rule.
fn rule(scope: &str, color: OilColor, italic: bool) -> Option<ThemeItem> {
    Some(ThemeItem {
        scope: scope.parse::<ScopeSelectors>().ok()?,
        style: StyleModifier {
            foreground: Some(to_syntect(color)),
            background: None,
            font_style: italic.then_some(FontStyle::ITALIC),
        },
    })
}

/// Colours for the syntax scopes, however the theme chose to supply them.
#[derive(Debug, Clone)]
pub struct SyntaxColors {
    pub keyword: OilColor,
    pub string: OilColor,
    pub comment: OilColor,
    pub number: OilColor,
    pub function: OilColor,
    pub type_name: OilColor,
    pub variable: OilColor,
    pub punctuation: OilColor,
}

impl SyntaxColors {
    /// Derive from the UI palette.
    ///
    /// The mapping is conventional rather than arbitrary — green strings, dim
    /// comments, accent keywords is what most themes do — but it is a guess, so
    /// `crucible.syntax.setup{}` can override any slot.
    pub fn derived_from(theme: &crucible_lua::theme::ThemeConfig) -> Self {
        let c = |a| theme.resolve_color(a);
        Self {
            keyword: c(theme.colors.primary),
            string: c(theme.colors.success),
            comment: c(theme.colors.text_dim),
            number: c(theme.colors.warning),
            function: c(theme.colors.secondary),
            type_name: c(theme.colors.info),
            variable: c(theme.colors.text),
            punctuation: c(theme.colors.text_muted),
        }
    }
}

/// Build a syntect theme from a set of syntax colours.
pub fn build_theme(colors: &SyntaxColors, background: OilColor, name: &str) -> Theme {
    let scopes = [
        // Comments are italic where the terminal supports it — the one font
        // style worth deriving, since it is near-universal in real themes.
        rule("comment, punctuation.definition.comment", colors.comment, true),
        rule("string, constant.character, constant.other.symbol", colors.string, false),
        rule("constant.numeric, constant.language, constant.character.escape", colors.number, false),
        rule("keyword, storage, storage.type, keyword.control, keyword.operator", colors.keyword, false),
        rule("entity.name.function, support.function, meta.function-call", colors.function, false),
        rule("entity.name.type, entity.name.class, support.type, support.class, entity.name.namespace", colors.type_name, false),
        rule("variable, variable.parameter, meta.definition.variable", colors.variable, false),
        rule("punctuation, meta.brace, keyword.operator.assignment", colors.punctuation, false),
    ]
    .into_iter()
    .flatten()
    .collect();

    Theme {
        name: Some(name.to_string()),
        author: Some("derived from the Crucible colorscheme".to_string()),
        settings: ThemeSettings {
            foreground: Some(to_syntect(colors.variable)),
            background: Some(to_syntect(background)),
            ..ThemeSettings::default()
        },
        scopes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_lua::theme::ThemeConfig;

    /// The property that makes terminal colours reach code blocks at all.
    #[test]
    fn palette_entries_survive_the_syntect_round_trip() {
        for color in [
            OilColor::Indexed(4),
            OilColor::Blue,
            OilColor::BrightMagenta,
            OilColor::DarkGray,
        ] {
            let restored = from_syntect(to_syntect(color));
            assert_eq!(
                restored.palette_index(),
                color.palette_index(),
                "{color:?} lost its palette slot crossing syntect"
            );
        }
    }

    #[test]
    fn rgb_colors_survive_unchanged() {
        let rgb = OilColor::Rgb(0x61, 0xaf, 0xef);
        assert_eq!(from_syntect(to_syntect(rgb)), rgb);
    }

    /// A real theme is opaque, so the marker cannot be mistaken for one.
    #[test]
    fn an_opaque_color_is_read_as_rgb_not_a_palette_index() {
        let opaque = SynColor {
            r: 4,
            g: 20,
            b: 30,
            a: 255,
        };
        assert_eq!(from_syntect(opaque), OilColor::Rgb(4, 20, 30));
    }

    #[test]
    fn derivation_follows_the_palette() {
        let mut theme = ThemeConfig::default_dark();
        theme.colors.primary =
            crucible_oil::style::AdaptiveColor::from_single(OilColor::Indexed(5));

        let colors = SyntaxColors::derived_from(&theme);
        assert_eq!(colors.keyword, OilColor::Indexed(5));
    }

    /// Every scope selector must parse, or that rule is silently dropped and
    /// the token renders unstyled.
    #[test]
    fn every_derived_scope_rule_is_valid() {
        let theme = ThemeConfig::default_dark();
        let built = build_theme(
            &SyntaxColors::derived_from(&theme),
            OilColor::Reset,
            "derived",
        );
        assert_eq!(
            built.scopes.len(),
            8,
            "a scope selector failed to parse and was dropped"
        );
    }

    #[test]
    fn the_derived_theme_carries_a_name() {
        let theme = ThemeConfig::default_dark();
        let built = build_theme(
            &SyntaxColors::derived_from(&theme),
            OilColor::Reset,
            "crucible-derived",
        );
        assert_eq!(built.name.as_deref(), Some("crucible-derived"));
    }
}
