//! Wire representation of [`ThemeConfig`] for the `ui.config` RPC.
//!
//! The wire form is deliberately **the authoring form** — the same JSON shape a
//! `runtime/themes/*.lua` file writes as a Lua table. Two consequences, both
//! load-bearing:
//!
//! 1. `crucible_oil::style::Color` (`Rgb(u8,u8,u8)`, named variants, `Reset`) is
//!    an internal rendering detail. Serializing it directly would promote it to a
//!    cross-process contract that could never change independently of clients.
//!
//! 2. Colors cross **unresolved**. An [`AdaptiveColor`] whose dark and light
//!    variants differ stays a `{dark, light}` pair on the wire; the client picks
//!    using its own terminal background and color depth. Resolving daemon-side
//!    would defeat the `ui.config` handshake, since the daemon cannot know the
//!    terminal a client is attached to — and a remote daemon certainly cannot.
//!
//! Both directions are generated from a single field list per struct, so a field
//! added to [`ThemeColors`] cannot be silently dropped from one side only.

use crate::theme::{
    BorderStyle, StatusBarPosition, ThemeColors, ThemeConfig, ThemeDecorations, ThemeIcons,
    ThemeLayout, ThemeSpinnerStyle,
};
use crucible_oil::style::{AdaptiveColor, Color};
use serde_json::{json, Map, Value};

/// Version of the `ui.config` payload.
///
/// Versioned from commit one: this crosses a process boundary, so a TUI and a
/// daemon at different versions will meet in the wild. Clients ignore fields
/// they do not know and fall back per-surface rather than wholesale.
pub const UI_CONFIG_VERSION: u32 = 1;

// ─────────────────────────────────────────────────────────────────────────────
// Colors
// ─────────────────────────────────────────────────────────────────────────────

/// Render a [`Color`] in the authoring vocabulary that `parse_color_string`
/// accepts, so every value round-trips.
fn color_to_name(color: Color) -> String {
    match color {
        Color::Black => "black".to_string(),
        Color::Red => "red".to_string(),
        Color::Green => "green".to_string(),
        Color::Yellow => "yellow".to_string(),
        Color::Blue => "blue".to_string(),
        Color::Magenta => "magenta".to_string(),
        Color::Cyan => "cyan".to_string(),
        Color::White => "white".to_string(),
        Color::Gray => "gray".to_string(),
        Color::DarkGray => "dark_gray".to_string(),
        Color::BrightRed => "bright_red".to_string(),
        Color::BrightGreen => "bright_green".to_string(),
        Color::BrightYellow => "bright_yellow".to_string(),
        Color::BrightBlue => "bright_blue".to_string(),
        Color::BrightMagenta => "bright_magenta".to_string(),
        Color::BrightCyan => "bright_cyan".to_string(),
        Color::BrightWhite => "bright_white".to_string(),
        // Indices cross the wire as numbers-in-strings, which `parse_color_string`
        // reads back as an index — so the round trip is lossless.
        Color::Indexed(i) => i.to_string(),
        Color::Reset => "reset".to_string(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

/// Collapse a uniform adaptive color to a single string, matching how themes are
/// written by hand. Only genuinely-adaptive values pay for the `{dark, light}`
/// form, which keeps the payload readable when inspected.
fn adaptive_to_wire(c: AdaptiveColor) -> Value {
    if c.dark == c.light {
        Value::String(color_to_name(c.dark))
    } else {
        json!({ "dark": color_to_name(c.dark), "light": color_to_name(c.light) })
    }
}

/// Inverse of [`adaptive_to_wire`]. Mirrors `theme::parse_adaptive_color`, but
/// over JSON instead of Lua values.
fn adaptive_from_wire(v: &Value) -> Option<AdaptiveColor> {
    match v {
        Value::String(s) => crate::theme::parse_color_string(s).map(AdaptiveColor::from_single),
        Value::Object(o) => {
            let dark = crate::theme::parse_color_string(o.get("dark")?.as_str()?)?;
            let light = crate::theme::parse_color_string(o.get("light")?.as_str()?)?;
            Some(AdaptiveColor { dark, light })
        }
        _ => None,
    }
}

/// Generates both directions from one field list. Adding a color to
/// [`ThemeColors`] and forgetting it here is a compile error on neither side —
/// which is why the round-trip test below asserts on the whole struct rather
/// than on sampled fields.
macro_rules! color_wire {
    ($($field:ident),+ $(,)?) => {
        fn colors_to_wire(c: &ThemeColors) -> Value {
            let mut m = Map::new();
            $( m.insert(stringify!($field).to_string(), adaptive_to_wire(c.$field)); )+
            Value::Object(m)
        }

        fn colors_from_wire(v: &Value) -> ThemeColors {
            let mut c = ThemeColors::default();
            let Some(m) = v.as_object() else { return c };
            $(
                if let Some(parsed) = m.get(stringify!($field)).and_then(adaptive_from_wire) {
                    c.$field = parsed;
                }
            )+
            c
        }
    };
}

color_wire!(
    primary,
    secondary,
    background,
    background_panel,
    command_bg,
    shell_bg,
    input_bg,
    text,
    text_muted,
    text_dim,
    text_emphasized,
    error,
    warning,
    success,
    info,
    border,
    border_focused,
    border_dim,
    user_message,
    assistant_message,
    system_message,
    mode_normal,
    mode_insert,
    mode_plan,
    mode_auto,
    diff_added,
    diff_removed,
    diff_added_bg,
    diff_removed_bg,
    diff_context,
    popup_bg,
    popup_selected_bg,
    toast_bg,
    overlay_text,
    overlay_bright,
    code_inline,
    code_fallback,
    fence_marker,
    bullet_prefix,
    blockquote_prefix,
    blockquote_text,
    link,
    heading_1,
    heading_2,
    heading_3,
);

// ─────────────────────────────────────────────────────────────────────────────
// String-valued sections
// ─────────────────────────────────────────────────────────────────────────────

macro_rules! string_wire {
    ($to:ident, $from:ident, $ty:ty, $($field:ident),+ $(,)?) => {
        fn $to(s: &$ty) -> Map<String, Value> {
            let mut m = Map::new();
            $( m.insert(stringify!($field).to_string(), Value::String(s.$field.clone())); )+
            m
        }

        fn $from(v: &Value, target: &mut $ty) {
            let Some(m) = v.as_object() else { return };
            $(
                if let Some(s) = m.get(stringify!($field)).and_then(Value::as_str) {
                    target.$field = s.to_string();
                }
            )+
        }
    };
}

string_wire!(
    decoration_strings_to_wire,
    decoration_strings_from_wire,
    ThemeDecorations,
    message_user_indicator,
    message_assistant_indicator,
    tool_pending_icon,
    tool_success_icon,
    tool_error_icon,
    bullet_char,
    divider_char,
    check_char,
    error_char,
    separator_char,
);

string_wire!(
    icons_to_wire_inner,
    icons_from_wire_inner,
    ThemeIcons,
    check,
    error,
    warning,
    info,
    loading,
    arrow_right,
);

// ─────────────────────────────────────────────────────────────────────────────
// Enum names
// ─────────────────────────────────────────────────────────────────────────────

fn border_style_name(s: BorderStyle) -> &'static str {
    match s {
        BorderStyle::Rounded => "rounded",
        BorderStyle::Sharp => "sharp",
        BorderStyle::Double => "double",
        BorderStyle::Thick => "thick",
        BorderStyle::Ascii => "ascii",
        BorderStyle::Hidden => "hidden",
    }
}

fn spinner_name(s: ThemeSpinnerStyle) -> &'static str {
    match s {
        ThemeSpinnerStyle::Braille => "braille",
        ThemeSpinnerStyle::BrailleMinidot => "braille_minidot",
        ThemeSpinnerStyle::Ascii => "ascii",
        ThemeSpinnerStyle::Pulse => "pulse",
        ThemeSpinnerStyle::None => "none",
    }
}

fn status_bar_position_name(p: StatusBarPosition) -> &'static str {
    match p {
        StatusBarPosition::Top => "top",
        StatusBarPosition::Bottom => "bottom",
        StatusBarPosition::Hidden => "hidden",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sections
// ─────────────────────────────────────────────────────────────────────────────

fn decorations_to_wire(d: &ThemeDecorations) -> Value {
    let mut m = decoration_strings_to_wire(d);
    m.insert(
        "border_style".to_string(),
        Value::String(border_style_name(d.border_style).to_string()),
    );
    // Single chars ride as one-char strings; JSON has no char type.
    m.insert(
        "half_block_top".to_string(),
        Value::String(d.half_block_top.to_string()),
    );
    m.insert(
        "half_block_bottom".to_string(),
        Value::String(d.half_block_bottom.to_string()),
    );
    Value::Object(m)
}

fn decorations_from_wire(v: &Value) -> ThemeDecorations {
    let mut d = ThemeDecorations::default();
    decoration_strings_from_wire(v, &mut d);
    let Some(m) = v.as_object() else { return d };
    if let Some(style) = m
        .get("border_style")
        .and_then(Value::as_str)
        .and_then(BorderStyle::from_name)
    {
        d.border_style = style;
    }
    if let Some(c) = m
        .get("half_block_top")
        .and_then(Value::as_str)
        .and_then(|s| s.chars().next())
    {
        d.half_block_top = c;
    }
    if let Some(c) = m
        .get("half_block_bottom")
        .and_then(Value::as_str)
        .and_then(|s| s.chars().next())
    {
        d.half_block_bottom = c;
    }
    d
}

fn icons_to_wire(i: &ThemeIcons) -> Value {
    Value::Object(icons_to_wire_inner(i))
}

fn icons_from_wire(v: &Value) -> ThemeIcons {
    let mut i = ThemeIcons::default();
    icons_from_wire_inner(v, &mut i);
    i
}

fn layout_to_wire(l: &ThemeLayout) -> Value {
    json!({
        "status_bar_position": status_bar_position_name(l.status_bar_position),
        "message_spacing": l.message_spacing,
        "code_block_margin": l.code_block_margin,
        "input_max_lines": l.input_max_lines,
    })
}

fn layout_from_wire(v: &Value) -> ThemeLayout {
    let mut l = ThemeLayout::default();
    let Some(m) = v.as_object() else { return l };
    if let Some(p) = m
        .get("status_bar_position")
        .and_then(Value::as_str)
        .and_then(StatusBarPosition::from_name)
    {
        l.status_bar_position = p;
    }
    let u16_field = |key: &str| m.get(key).and_then(Value::as_u64).map(|n| n as u16);
    if let Some(n) = u16_field("message_spacing") {
        l.message_spacing = n;
    }
    if let Some(n) = u16_field("code_block_margin") {
        l.code_block_margin = n;
    }
    if let Some(n) = u16_field("input_max_lines") {
        l.input_max_lines = n;
    }
    l
}

// ─────────────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/// Serialize a theme into the `ui.config` wire form.
pub fn theme_to_wire(t: &ThemeConfig) -> Value {
    json!({
        "name": t.name,
        "is_dark": t.is_dark,
        "colors": colors_to_wire(&t.colors),
        "decorations": decorations_to_wire(&t.decorations),
        "icons": icons_to_wire(&t.icons),
        "spinner": spinner_name(t.spinner),
        "layout": layout_to_wire(&t.layout),
    })
}

/// Parse a theme from the `ui.config` wire form.
///
/// Fails to a **complete default**, never to a partial theme. A missing or
/// malformed field keeps the corresponding default rather than leaving a hole:
/// applying `bg` without `fg` yields invisible text, which is worse than
/// ignoring the customization entirely.
pub fn theme_from_wire(v: &Value) -> ThemeConfig {
    let mut t = ThemeConfig::default_dark();
    let Some(m) = v.as_object() else { return t };

    if let Some(name) = m.get("name").and_then(Value::as_str) {
        t.name = name.to_string();
    }
    if let Some(is_dark) = m.get("is_dark").and_then(Value::as_bool) {
        t.is_dark = is_dark;
    }
    if let Some(colors) = m.get("colors") {
        t.colors = colors_from_wire(colors);
    }
    if let Some(d) = m.get("decorations") {
        t.decorations = decorations_from_wire(d);
    }
    if let Some(i) = m.get("icons") {
        t.icons = icons_from_wire(i);
    }
    if let Some(s) = m
        .get("spinner")
        .and_then(Value::as_str)
        .and_then(ThemeSpinnerStyle::from_name)
    {
        t.spinner = s;
    }
    if let Some(l) = m.get("layout") {
        t.layout = layout_from_wire(l);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole-struct round-trip is the drift guard. A color added to
    /// `ThemeColors` but omitted from the `color_wire!` list fails here, because
    /// the omitted field falls back to its default and the comparison breaks —
    /// provided the value under test differs from that default.
    #[test]
    fn default_dark_survives_a_wire_round_trip() {
        let original = ThemeConfig::default_dark();
        let restored = theme_from_wire(&theme_to_wire(&original));

        assert_eq!(restored.name, original.name);
        assert_eq!(restored.is_dark, original.is_dark);
        assert_eq!(restored.colors, original.colors);
        assert_eq!(restored.decorations, original.decorations);
        assert_eq!(restored.icons, original.icons);
        assert_eq!(restored.spinner, original.spinner);
        assert_eq!(restored.layout, original.layout);
    }

    #[test]
    fn named_colors_cross_the_wire_as_names() {
        let mut t = ThemeConfig::default_dark();
        t.colors.primary = AdaptiveColor::from_single(Color::Cyan);
        assert_eq!(theme_to_wire(&t)["colors"]["primary"], "cyan");
    }

    #[test]
    fn rgb_colors_cross_the_wire_as_hex() {
        let mut t = ThemeConfig::default_dark();
        t.colors.background = AdaptiveColor::from_single(Color::Rgb(0x28, 0x2c, 0x34));
        assert_eq!(theme_to_wire(&t)["colors"]["background"], "#282c34");
    }

    /// The point of the handshake: a genuinely adaptive value must stay a pair
    /// so the client can pick using its own terminal background.
    /// Terminal palette entries must survive the wire, or a theme written
    /// against the user's terminal colours silently degrades to fixed RGB.
    #[test]
    fn palette_indices_round_trip() {
        let mut t = ThemeConfig::default_dark();
        t.colors.primary = AdaptiveColor::from_single(Color::Indexed(4));
        t.colors.secondary = AdaptiveColor::from_single(Color::BrightMagenta);

        let restored = theme_from_wire(&theme_to_wire(&t));
        assert_eq!(restored.colors.primary.dark, Color::Indexed(4));
        assert_eq!(restored.colors.secondary.dark, Color::BrightMagenta);
    }

    #[test]
    fn adaptive_colors_stay_unresolved_as_a_pair() {
        let mut t = ThemeConfig::default_dark();
        t.colors.text = AdaptiveColor {
            dark: Color::White,
            light: Color::Black,
        };

        let wire = theme_to_wire(&t);
        assert_eq!(wire["colors"]["text"]["dark"], "white");
        assert_eq!(wire["colors"]["text"]["light"], "black");

        let restored = theme_from_wire(&wire);
        assert_eq!(restored.colors.text.dark, Color::White);
        assert_eq!(restored.colors.text.light, Color::Black);
    }

    /// Fail to a complete default, never to a partial theme.
    #[test]
    fn garbage_input_yields_a_complete_default_theme() {
        assert_eq!(
            theme_from_wire(&json!("not a theme")).colors,
            ThemeConfig::default_dark().colors
        );
        assert_eq!(
            theme_from_wire(&json!({ "colors": "nonsense" })).colors,
            ThemeConfig::default_dark().colors
        );
    }

    /// An unknown color name keeps that field's default rather than blanking it.
    #[test]
    fn unknown_color_name_keeps_the_default_for_that_field() {
        let wire = json!({ "colors": { "primary": "chartreuse" } });
        assert_eq!(
            theme_from_wire(&wire).colors.primary,
            ThemeConfig::default_dark().colors.primary
        );
    }

    /// Forward compatibility: a newer daemon may send surfaces this client does
    /// not know. Ignore them, keep everything else.
    #[test]
    fn unknown_fields_are_ignored_not_fatal() {
        let wire = json!({
            "name": "future",
            "surfaces_from_a_later_version": { "hologram": { "fg": "cyan" } },
        });
        assert_eq!(theme_from_wire(&wire).name, "future");
    }
}
