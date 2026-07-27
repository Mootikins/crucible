//! `crucible.hl` — the Lua surface for highlight groups, and their wire form.
//!
//! ```lua
//! crucible.hl.set("StatusMode", { fg = "black", bg = "mode_normal", bold = true })
//! crucible.hl.link("PopupSelected", "Visual")
//! ```
//!
//! Split from [`crate::hl`] so the resolution logic stays free of mlua and can
//! be used by the TUI, which has no Lua VM.

use crate::error::LuaError;
use crate::hl::{HlColor, HlGroup, HlRegistry};
use crucible_oil::style::{AdaptiveColor, Color};
use mlua::{Lua, Table, Value};
use serde_json::{json, Map, Value as Json};

/// Read a colour field written in any of the authoring forms.
fn color_from_lua(table: &Table, key: &str) -> Option<HlColor> {
    match table.get::<Value>(key).ok()? {
        Value::String(s) => Some(HlColor::parse(&s.to_str().ok()?)),
        // `fg = 4` — a bare integer is a terminal palette index.
        Value::Integer(n) => u8::try_from(n)
            .ok()
            .map(|i| HlColor::Adaptive(AdaptiveColor::from_single(Color::Indexed(i)))),
        Value::Table(t) => {
            if let Ok(idx) = t.get::<u8>("idx") {
                return Some(HlColor::Adaptive(AdaptiveColor::from_single(
                    Color::Indexed(idx),
                )));
            }
            let dark = side(&t.get::<Value>("dark").ok()?)?;
            let light = side(&t.get::<Value>("light").ok()?)?;
            Some(HlColor::Adaptive(AdaptiveColor { dark, light }))
        }
        _ => None,
    }
}

/// One side of an adaptive pair: a name, a hex literal, or an index.
fn side(v: &Value) -> Option<Color> {
    match v {
        Value::String(s) => crate::theme::parse_color_string(&s.to_str().ok()?),
        Value::Integer(n) => u8::try_from(*n).ok().map(Color::Indexed),
        Value::Table(t) => t.get::<u8>("idx").ok().map(Color::Indexed),
        _ => None,
    }
}

fn group_from_lua(table: &Table) -> HlGroup {
    HlGroup {
        fg: color_from_lua(table, "fg"),
        bg: color_from_lua(table, "bg"),
        bold: table.get("bold").unwrap_or(false),
        dim: table.get("dim").unwrap_or(false),
        italic: table.get("italic").unwrap_or(false),
        underline: table.get("underline").unwrap_or(false),
        link: table.get::<String>("link").ok(),
    }
}

/// Register `crucible.hl` on an existing `crucible` table.
pub fn register_hl_namespace(lua: &Lua, crucible: &Table) -> Result<(), LuaError> {
    let hl = lua.create_table()?;

    // crucible.hl.set(name, spec)
    let set_fn = lua.create_function(|_, (name, spec): (String, Table)| {
        crate::config::set_hl_group(name, group_from_lua(&spec));
        Ok(())
    })?;
    hl.set("set", set_fn)?;

    // crucible.hl.link(from, to) — sugar for set(from, { link = to })
    let link_fn = lua.create_function(|_, (from, to): (String, String)| {
        crate::config::set_hl_group(
            from,
            HlGroup {
                link: Some(to),
                ..Default::default()
            },
        );
        Ok(())
    })?;
    hl.set("link", link_fn)?;

    crucible.set("hl", hl)?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Wire form
// ─────────────────────────────────────────────────────────────────────────────

fn color_to_wire(c: &HlColor) -> Json {
    match c {
        // Palette references cross the wire as names, unresolved — the client
        // resolves them against the theme it received in the same payload.
        HlColor::Palette(name) => Json::String(name.clone()),
        HlColor::Adaptive(a) if a.dark == a.light => Json::String(color_name(a.dark)),
        HlColor::Adaptive(a) => {
            json!({ "dark": color_name(a.dark), "light": color_name(a.light) })
        }
    }
}

fn color_name(c: Color) -> String {
    match c {
        Color::Black => "black".into(),
        Color::Red => "red".into(),
        Color::Green => "green".into(),
        Color::Yellow => "yellow".into(),
        Color::Blue => "blue".into(),
        Color::Magenta => "magenta".into(),
        Color::Cyan => "cyan".into(),
        Color::White => "white".into(),
        Color::Gray => "gray".into(),
        Color::DarkGray => "dark_gray".into(),
        Color::BrightRed => "bright_red".into(),
        Color::BrightGreen => "bright_green".into(),
        Color::BrightYellow => "bright_yellow".into(),
        Color::BrightBlue => "bright_blue".into(),
        Color::BrightMagenta => "bright_magenta".into(),
        Color::BrightCyan => "bright_cyan".into(),
        Color::BrightWhite => "bright_white".into(),
        // Indices cross the wire as numbers-in-strings, which `parse_color_string`
        // reads back as an index — so the round trip is lossless.
        Color::Indexed(i) => i.to_string(),
        Color::Reset => "reset".into(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

fn color_from_wire(v: &Json) -> Option<HlColor> {
    match v {
        Json::String(s) => Some(HlColor::parse(s)),
        Json::Object(o) => {
            let dark = crate::theme::parse_color_string(o.get("dark")?.as_str()?)?;
            let light = crate::theme::parse_color_string(o.get("light")?.as_str()?)?;
            Some(HlColor::Adaptive(AdaptiveColor { dark, light }))
        }
        _ => None,
    }
}

/// Serialize the highlight table for `ui.config`.
pub fn registry_to_wire(registry: &HlRegistry) -> Json {
    let mut out = Map::new();
    for (name, g) in registry {
        let mut m = Map::new();
        if let Some(ref fg) = g.fg {
            m.insert("fg".into(), color_to_wire(fg));
        }
        if let Some(ref bg) = g.bg {
            m.insert("bg".into(), color_to_wire(bg));
        }
        for (key, set) in [
            ("bold", g.bold),
            ("dim", g.dim),
            ("italic", g.italic),
            ("underline", g.underline),
        ] {
            if set {
                m.insert(key.into(), Json::Bool(true));
            }
        }
        if let Some(ref link) = g.link {
            m.insert("link".into(), Json::String(link.clone()));
        }
        out.insert(name.clone(), Json::Object(m));
    }
    Json::Object(out)
}

/// Parse the highlight table from `ui.config`. Unparseable entries are skipped
/// rather than failing the payload — one bad group must not cost the rest.
pub fn registry_from_wire(v: &Json) -> HlRegistry {
    let mut registry = HlRegistry::new();
    let Some(obj) = v.as_object() else {
        return registry;
    };

    for (name, spec) in obj {
        let Some(m) = spec.as_object() else { continue };
        let flag = |key: &str| m.get(key).and_then(Json::as_bool).unwrap_or(false);
        registry.insert(
            name.clone(),
            HlGroup {
                fg: m.get("fg").and_then(color_from_wire),
                bg: m.get("bg").and_then(color_from_wire),
                bold: flag("bold"),
                dim: flag("dim"),
                italic: flag("italic"),
                underline: flag("underline"),
                link: m
                    .get("link")
                    .and_then(Json::as_str)
                    .map(std::string::ToString::to_string),
            },
        );
    }
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hl::resolve;
    use crate::theme::ThemeConfig;

    fn lua_with_hl() -> Lua {
        let lua = Lua::new();
        let crucible = lua.create_table().unwrap();
        lua.globals().set("crucible", crucible.clone()).unwrap();
        register_hl_namespace(&lua, &crucible).unwrap();
        lua
    }

    #[test]
    fn set_defines_a_group_reachable_from_the_store() {
        let lua = lua_with_hl();
        lua.load(r#"crucible.hl.set("Mode", { fg = "black", bg = "mode_normal", bold = true })"#)
            .exec()
            .unwrap();

        let registry = crate::config::get_hl_registry();
        let group = registry.get("Mode").expect("group was stored");
        assert_eq!(group.bg, Some(HlColor::Palette("mode_normal".into())));
        assert!(group.bold);
    }

    #[test]
    fn link_is_sugar_for_a_link_only_group() {
        let lua = lua_with_hl();
        lua.load(r#"crucible.hl.link("PopupSel", "Visual")"#)
            .exec()
            .unwrap();

        let registry = crate::config::get_hl_registry();
        assert_eq!(
            registry.get("PopupSel").unwrap().link,
            Some("Visual".to_string())
        );
    }

    #[test]
    fn adaptive_colours_survive_the_lua_table_form() {
        let lua = lua_with_hl();
        lua.load(r#"crucible.hl.set("T", { fg = { dark = "white", light = "black" } })"#)
            .exec()
            .unwrap();

        let registry = crate::config::get_hl_registry();
        assert_eq!(
            registry.get("T").unwrap().fg,
            Some(HlColor::Adaptive(AdaptiveColor {
                dark: Color::White,
                light: Color::Black
            }))
        );
    }

    #[test]
    fn a_registry_survives_a_wire_round_trip() {
        let mut registry = HlRegistry::new();
        registry.insert(
            "Mode".into(),
            HlGroup {
                fg: Some(HlColor::parse("black")),
                bg: Some(HlColor::parse("mode_normal")),
                bold: true,
                ..Default::default()
            },
        );
        registry.insert(
            "Sel".into(),
            HlGroup {
                link: Some("Mode".into()),
                ..Default::default()
            },
        );

        assert_eq!(registry_from_wire(&registry_to_wire(&registry)), registry);
    }

    /// Palette references must stay names across the wire, or the client cannot
    /// re-resolve them when the palette changes.
    #[test]
    fn palette_references_cross_the_wire_unresolved() {
        let mut registry = HlRegistry::new();
        registry.insert(
            "Mode".into(),
            HlGroup {
                bg: Some(HlColor::parse("mode_normal")),
                ..Default::default()
            },
        );

        assert_eq!(registry_to_wire(&registry)["Mode"]["bg"], "mode_normal");
    }

    /// End to end through the wire: a group authored against the palette
    /// resolves to whatever the receiving theme says that palette entry is.
    #[test]
    fn a_group_resolves_after_crossing_the_wire() {
        let mut registry = HlRegistry::new();
        registry.insert(
            "Mode".into(),
            HlGroup {
                bg: Some(HlColor::parse("mode_normal")),
                ..Default::default()
            },
        );

        let restored = registry_from_wire(&registry_to_wire(&registry));
        let theme = ThemeConfig::default_dark();

        assert_eq!(
            resolve("Mode", &restored, &theme).unwrap().bg,
            Some(theme.resolve_color(theme.colors.mode_normal))
        );
    }

    #[test]
    fn a_malformed_group_is_skipped_not_fatal() {
        let wire = json!({ "Good": { "fg": "cyan" }, "Bad": "not a table" });
        let registry = registry_from_wire(&wire);

        assert!(registry.contains_key("Good"));
        assert!(!registry.contains_key("Bad"));
    }
}
